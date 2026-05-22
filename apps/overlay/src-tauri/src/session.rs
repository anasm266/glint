//! Session model + state machine for hook events.
//!
//! We map raw hook events from Codex into a small, stable shape the UI
//! consumes. The mapping is intentionally lossy: we strip everything the
//! compact view doesn't need.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum App {
    Codex,
    Cursor,
    Claude,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Working,
    Done,
    Errored,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffStat {
    pub adds: u32,
    pub dels: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Normal,
    Success,
    Failure,
}

impl Default for ActivityKind {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub seq: u64,
    pub at_ms: u64,
    pub summary: String,
    #[serde(default = "default_activity_count")]
    pub count: u32,
    #[serde(default)]
    pub kind: ActivityKind,
}

fn default_activity_count() -> u32 {
    1
}

const MAX_ACTIVITY_ENTRIES: usize = 8;
const PARALLEL_MAX_ITEMS: usize = 3;
const PARALLEL_MAX_CHARS: usize = 120;

#[derive(Debug, Clone)]
struct BufferedActivity {
    summary: String,
    dedupe_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub app: App,
    pub project: String,
    pub cwd: String,
    pub status: Status,
    pub current_action: String,
    pub started_at_ms: u64,
    pub last_event_at_ms: u64,
    pub acknowledged_done: bool,
    pub last_prompt: String,
    pub files_edited: Vec<(String, DiffStat)>,
    pub recent_activity: Vec<ActivityEntry>,
    pub model: String,
    pub last_commit_hash: Option<String>,
    pub done_summary: Option<String>,
    #[serde(skip)]
    pub last_turn_id: String,
    #[serde(skip)]
    pub turn_activity_turn_id: String,
    #[serde(skip)]
    turn_activity_entries: Vec<BufferedActivity>,
    #[serde(skip)]
    pub last_bash_command: String,
    #[serde(skip)]
    pub next_activity_seq: u64,
    #[serde(skip)]
    pub parent_pid: Option<u32>,
    #[serde(skip)]
    pub files_edited_map: HashMap<String, DiffStat>,
}

impl Session {
    pub fn new(id: String, cwd: String, parent_pid: Option<u32>, ts_ms: u64) -> Self {
        let project = project_basename(&cwd);
        Self {
            id,
            app: App::Codex,
            project,
            cwd,
            status: Status::Working,
            current_action: "Thinking…".to_string(),
            started_at_ms: ts_ms,
            last_event_at_ms: ts_ms,
            acknowledged_done: false,
            last_prompt: String::new(),
            files_edited: vec![],
            recent_activity: vec![],
            model: String::new(),
            last_commit_hash: None,
            done_summary: None,
            last_turn_id: String::new(),
            turn_activity_turn_id: String::new(),
            turn_activity_entries: Vec::new(),
            last_bash_command: String::new(),
            next_activity_seq: 0,
            parent_pid,
            files_edited_map: HashMap::new(),
        }
    }

    pub fn push_activity_with_kind(
        &mut self,
        summary: String,
        at_ms: u64,
        kind: ActivityKind,
        turn_id: &str,
    ) {
        if summary.is_empty() {
            return;
        }
        if let Some(first) = self.recent_activity.first_mut() {
            if first.summary == summary {
                if !turn_id.is_empty() && turn_id == self.last_turn_id {
                    first.count = first.count.saturating_add(1);
                    return;
                }
                return;
            }
        }
        self.next_activity_seq += 1;
        self.recent_activity.insert(
            0,
            ActivityEntry {
                seq: self.next_activity_seq,
                at_ms,
                summary,
                count: 1,
                kind,
            },
        );
        self.recent_activity.truncate(MAX_ACTIVITY_ENTRIES);
    }

    pub fn flatten_files(&mut self) {
        let mut v: Vec<(String, DiffStat)> = self
            .files_edited_map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        self.files_edited = v;
    }

    fn flush_turn_activity(&mut self, at_ms: u64, turn_id: &str) {
        if self.turn_activity_entries.is_empty() {
            return;
        }
        let summary = if self.turn_activity_entries.len() == 1 {
            self.turn_activity_entries[0].summary.clone()
        } else {
            let summaries: Vec<String> = self
                .turn_activity_entries
                .iter()
                .map(|e| e.summary.clone())
                .collect();
            format_parallel_summary(&summaries)
        };
        self.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, turn_id);
        self.turn_activity_entries.clear();
        self.turn_activity_turn_id.clear();
    }
}

fn format_parallel_summary(summaries: &[String]) -> String {
    let n = summaries.len();
    let show = n.min(PARALLEL_MAX_ITEMS);
    let joined = summaries[..show].join(" · ");
    let mut out = format!("Parallel: {joined}");
    if n > PARALLEL_MAX_ITEMS {
        out.push_str(&format!(" · +{} more", n - PARALLEL_MAX_ITEMS));
    }
    truncate(&out, PARALLEL_MAX_CHARS)
}

fn activity_dedupe_key(raw_cmd: &str) -> String {
    strip_shell_noise(raw_cmd)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn project_basename(cwd: &str) -> String {
    let p = PathBuf::from(cwd);
    p.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| cwd.to_string())
}

const MAX_PROMPT_CHARS: usize = 4000;

fn extract_user_prompt(p: &serde_json::Value) -> Option<String> {
    // Try top-level string fields — exact name depends on Codex version.
    let v = p
        .get("input")
        .or_else(|| p.get("content"))
        .or_else(|| p.get("prompt"))
        .or_else(|| p.get("user_input"))
        .or_else(|| p.get("user_message"))
        .or_else(|| p.get("message"))
        .or_else(|| p.get("text"));

    if let Some(v) = v {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        // Some hooks nest the text inside an object.
        if let Some(obj) = v.as_object() {
            for key in ["text", "content", "value"] {
                if let Some(s) = obj.get(key).and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }

    // Fallback: longest top-level string that isn't a known structural field.
    let structural: &[&str] = &["session_id", "cwd", "hook_event_name"];
    if let Some(obj) = p.as_object() {
        let best = obj
            .iter()
            .filter(|(k, _)| !structural.contains(&k.as_str()))
            .filter_map(|(_, v)| v.as_str())
            .max_by_key(|s| s.len());
        if let Some(s) = best {
            if s.len() > 3 {
                return Some(s.to_string());
            }
        }
    }

    None
}

fn truncate_prompt(s: String) -> String {
    if s.chars().count() <= MAX_PROMPT_CHARS {
        return s;
    }
    let mut out: String = s.chars().take(MAX_PROMPT_CHARS.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Raw event envelope as produced by `overlay-hook.exe`.
#[derive(Debug, Clone, Deserialize)]
pub struct RawEvent {
    pub event: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    pub ts: u64,
    #[serde(default)]
    #[allow(dead_code)]
    pub hook_pid: Option<u32>,
    #[serde(default)]
    pub parent_pid: Option<u32>,
}

/// Apply a raw event to the session map. Returns the session id that was
/// affected (if any) so callers can do follow-up work.
pub fn apply(map: &mut HashMap<String, Session>, raw: RawEvent) -> Option<String> {
    let p = &raw.payload;
    let session_id = p
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if session_id.is_empty() {
        return None;
    }

    let cwd = p
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let entry = map
        .entry(session_id.clone())
        .or_insert_with(|| Session::new(session_id.clone(), cwd.clone(), raw.parent_pid, raw.ts));

    if entry.cwd.is_empty() && !cwd.is_empty() {
        entry.cwd = cwd.clone();
        entry.project = project_basename(&cwd);
    }
    if entry.parent_pid.is_none() {
        entry.parent_pid = raw.parent_pid;
    }
    entry.last_event_at_ms = raw.ts;

    if let Some(model) = p.get("model").and_then(|v| v.as_str()) {
        entry.model = model.to_string();
    }

    match raw.event.as_str() {
        "SessionStart" => {
            entry.status = Status::Working;
            entry.current_action = "Thinking…".to_string();
            entry.acknowledged_done = false;
            entry.started_at_ms = raw.ts;
            entry.last_prompt = String::new();
            entry.recent_activity.clear();
            entry.next_activity_seq = 0;
            entry.turn_activity_entries.clear();
            entry.turn_activity_turn_id.clear();
            entry.done_summary = None;
        }
        "UserPromptSubmit" => {
            entry.status = Status::Working;
            entry.current_action = "Thinking…".to_string();
            entry.acknowledged_done = false;
            entry.started_at_ms = raw.ts;
            entry.last_prompt = String::new();
            entry.recent_activity.clear();
            entry.next_activity_seq = 0;
            entry.turn_activity_entries.clear();
            entry.turn_activity_turn_id.clear();
            entry.done_summary = None;
            if let Some(prompt) = extract_user_prompt(p) {
                let t = prompt.trim();
                if !t.is_empty() {
                    entry.last_prompt = truncate_prompt(t.to_string());
                }
            }
        }
        "PreToolUse" => {
            entry.status = Status::Working;
            let tool = p.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
            if tool == "Bash" {
                let cmd = p
                    .get("tool_input")
                    .and_then(|v| v.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                entry.last_bash_command = cmd.to_string();
            }
            let action = describe_pre_tool(p);
            let turn_id = p.get("turn_id").and_then(|v| v.as_str()).unwrap_or("");
            if !entry.turn_activity_turn_id.is_empty() && turn_id != entry.turn_activity_turn_id {
                let flush_tid = entry.turn_activity_turn_id.clone();
                entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            }
            if !action.is_empty() {
                let dedupe_key = if tool == "Bash" {
                    activity_dedupe_key(&entry.last_bash_command)
                } else {
                    action.clone()
                };
                let already = entry
                    .turn_activity_entries
                    .iter()
                    .any(|e| e.dedupe_key == dedupe_key);
                if !already {
                    entry.turn_activity_entries.push(BufferedActivity {
                        summary: action.clone(),
                        dedupe_key,
                    });
                    if entry.turn_activity_entries.len() > 8 {
                        let excess = entry.turn_activity_entries.len() - 8;
                        entry.turn_activity_entries.drain(0..excess);
                    }
                }
            }
            if !turn_id.is_empty() {
                entry.turn_activity_turn_id = turn_id.to_string();
            }
            entry.current_action = action;
            entry.last_turn_id = turn_id.to_string();
        }
        "PostToolUse" => {
            let flush_tid = entry.turn_activity_turn_id.clone();
            entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            track_files_from_post(entry, p);
            let tool = p.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
            let response = p
                .get("tool_response")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if tool == "Bash" {
                parse_gh_pr_post(entry, response, raw.ts);
                parse_gh_issue_post(entry, response, raw.ts);
                parse_rg_post(entry, response, raw.ts);
                parse_git_log_post(entry, response, raw.ts);
                parse_git_blame_post(entry, response, raw.ts);
                parse_node_probe_post(entry, response, raw.ts);
                if let Some((passed, failed)) = parse_test_result(response) {
                    let summary = if failed == 0 {
                        format!("{passed} tests passed")
                    } else {
                        format!("{failed} tests failed")
                    };
                    let kind = if failed == 0 {
                        ActivityKind::Success
                    } else {
                        ActivityKind::Failure
                    };
                    entry.push_activity_with_kind(summary, raw.ts, kind, "");
                }
                if should_parse_commit_hash(&entry.last_bash_command) {
                    if let Some(hash) = parse_commit_hash(response) {
                        entry.last_commit_hash = Some(hash);
                    }
                }
            }
            entry.current_action = "Thinking…".to_string();
        }
        "Stop" => {
            let flush_tid = entry.turn_activity_turn_id.clone();
            entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            entry.status = Status::Done;
            if let Some(msg) = p.get("last_assistant_message").and_then(|v| v.as_str()) {
                let summary = extract_done_summary(msg);
                if !summary.is_empty() {
                    entry.done_summary = Some(summary.clone());
                    entry.current_action = summary;
                } else {
                    entry.current_action = "Done".to_string();
                }
            } else {
                entry.current_action = "Done".to_string();
            }
            entry.flatten_files();
        }
        _ => {
            // Unknown event: still useful as a heartbeat.
        }
    }

    Some(session_id)
}

fn describe_pre_tool(p: &serde_json::Value) -> String {
    let tool = p
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Tool");

    let input = p.get("tool_input");

    match tool {
        "Bash" => {
            let cmd = input
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if cmd.is_empty() {
                "Running command".to_string()
            } else {
                describe_bash(cmd)
            }
        }
        "apply_patch" => {
            // tool_input.command is the patch text; the first `*** Update File:`
            // / `*** Add File:` / `*** Delete File:` line gives us the target.
            let cmd = input
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let first = first_patch_target(cmd);
            match first {
                Some((verb, path)) => format!("{verb} {}", truncate(&path, 56)),
                None => "Editing files".to_string(),
            }
        }
        other => {
            // Generic / MCP tool. Show humanized name.
            format!("Calling tool: {}", humanize_tool(other))
        }
    }
}

fn track_files_from_post(s: &mut Session, p: &serde_json::Value) {
    let tool = p
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if tool != "apply_patch" {
        return;
    }
    let patch = p
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if patch.is_empty() {
        return;
    }
    // Lightweight parser: walks `*** Update File: <path>` blocks and counts
    // `+` / `-` lines until the next `*** ` header or EOF.
    let mut current: Option<String> = None;
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("*** ") {
            current = parse_patch_header(rest);
            continue;
        }
        let Some(path) = current.clone() else {
            continue;
        };
        if line.starts_with('+') && !line.starts_with("+++") {
            s.files_edited_map.entry(path).or_default().adds += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            s.files_edited_map.entry(path).or_default().dels += 1;
        }
    }
    s.flatten_files();
}

fn parse_patch_header(rest: &str) -> Option<String> {
    for verb in ["Update File:", "Add File:", "Delete File:"] {
        if let Some(p) = rest.strip_prefix(verb) {
            return Some(p.trim().to_string());
        }
    }
    None
}

fn first_patch_target(patch: &str) -> Option<(&'static str, String)> {
    for line in patch.lines() {
        let Some(rest) = line.strip_prefix("*** ") else { continue };
        if let Some(p) = rest.strip_prefix("Update File:") {
            return Some(("Editing", p.trim().to_string()));
        }
        if let Some(p) = rest.strip_prefix("Add File:") {
            return Some(("Creating", p.trim().to_string()));
        }
        if let Some(p) = rest.strip_prefix("Delete File:") {
            return Some(("Deleting", p.trim().to_string()));
        }
    }
    None
}

fn humanize_tool(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("mcp__") {
        return rest.replace("__", " · ");
    }
    name.to_string()
}

// ── Bash command summarizer ──────────────────────────────────────────────────
//
// Two-phase, zero-regex approach. Phase 1 strips shell noise (PowerShell
// preambles, `cd X &&` prefixes, interpreter wrappers) returning a slice into
// the original string. Phase 2 matches the first real token against a static
// table. All sub-classifiers return &'static str so the only allocation is the
// single format! call that may occur at the call site.

fn describe_bash(raw: &str) -> String {
    if let Some(label) = classify_pipeline_or_heredoc(raw) {
        return label;
    }
    let cmd = strip_shell_noise(raw);
    classify_bash(cmd)
}

fn classify_pipeline_or_heredoc(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let heredoc = trimmed.starts_with("@'")
        || trimmed.contains("'@ |")
        || trimmed.contains("'@|");
    if heredoc || trimmed.contains('|') {
        if let Some(rhs) = trimmed.rsplit('|').next() {
            let rhs = strip_shell_noise(rhs.trim());
            let verb = rhs.split_whitespace().next()?.to_ascii_lowercase();
            if matches!(verb.as_str(), "node" | "ts-node" | "tsx" | "deno") {
                let rest = rhs[verb.len()..].trim();
                return Some(classify_node(rest));
            }
            if heredoc {
                return Some("Running inline script".to_string());
            }
        } else if heredoc {
            return Some("Running inline script".to_string());
        }
    }
    None
}

/// Strip PowerShell preamble, `cd X &&` prefixes and interpreter wrappers.
/// Returns a sub-slice of `raw` — no allocation.
fn strip_shell_noise(raw: &str) -> &str {
    let mut s = raw.trim();

    // 1. Split on `;` and skip pure assignment segments (PowerShell preamble).
    //    e.g. `$ErrorActionPreference='Stop'; $x = ...; actual-command`
    let mut last_non_assign: &str = s;
    for segment in s.split(';') {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }
        // A segment is a pure assignment if it starts with `$` and contains `=`
        // before any whitespace, or is a simple `set KEY=VALUE` style.
        let is_assignment = (seg.starts_with('$') && {
            let up_to_eq = seg.find('=').unwrap_or(0);
            let up_to_sp = seg.find(char::is_whitespace).unwrap_or(usize::MAX);
            up_to_eq > 0 && up_to_eq <= up_to_sp
        }) || seg.starts_with("set ") && seg.contains('=');
        if !is_assignment {
            last_non_assign = seg;
            break;
        }
    }
    s = last_non_assign;

    // 2. `cd X && cmd` — take after `&&`.
    if s.starts_with("cd ") {
        if let Some(pos) = s.find(" && ") {
            s = s[pos + 4..].trim();
        }
    }

    // 3. `timeout N cmd` — skip the verb and numeric arg.
    if s.starts_with("timeout ") {
        let mut parts = s.splitn(3, char::is_whitespace);
        parts.next(); // "timeout"
        parts.next(); // the number
        if let Some(rest) = parts.next() {
            s = rest.trim();
        }
    }

    // 4. Interpreter wrappers: `bash -c 'inner'`, `powershell -Command "inner"`.
    let verb = s.split_whitespace().next().unwrap_or("");
    if matches!(verb, "bash" | "sh" | "zsh" | "powershell" | "pwsh") {
        // Look for `-c` or `-Command` followed by a quoted string.
        if let Some(pos) = s.find(" -c ").or_else(|| s.find(" -Command ")) {
            let after = s[pos..].splitn(3, char::is_whitespace).nth(2).unwrap_or("").trim();
            let inner = after
                .strip_prefix('"')
                .and_then(|x| x.strip_suffix('"'))
                .or_else(|| after.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')))
                .unwrap_or(after);
            if !inner.is_empty() {
                s = inner;
            }
        }
    }

    s.trim()
}

/// Classify a noise-stripped command into a human label.
fn classify_bash(cmd: &str) -> String {
    let mut tokens = cmd.split_whitespace();
    let verb = match tokens.next() {
        Some(v) => v,
        None => return "Running command".to_string(),
    };
    // Lowercase the verb for case-insensitive matching (PowerShell uses
    // PascalCase aliases). Allocates only a tiny stack string for the verb.
    let verb_lc = verb.to_ascii_lowercase();
    let verb_lc = verb_lc.as_str();

    // Collect remaining tokens once; sub-classifiers borrow slices.
    let rest = cmd[verb.len()..].trim();

    match verb_lc {
        // ── Direct test runners ─────────────────────────────────────────────
        "pytest" | "py.test" => "Running Python tests".to_string(),
        "jest" | "vitest" | "mocha" | "jasmine" | "ava" => "Running JS tests".to_string(),
        "rspec" => "Running Ruby tests".to_string(),
        "phpunit" => "Running PHP tests".to_string(),

        // ── Package managers / build tools ──────────────────────────────────
        "npm" | "yarn" | "pnpm" | "bun" => classify_npm(rest),
        "npx" | "bunx" => {
            let target = rest.split_whitespace().find(|t| !t.starts_with('-')).unwrap_or(rest);
            format!("Running: {}", basename(target))
        }
        "cargo" => classify_cargo(first_non_flag(rest)),
        "go" => classify_go(first_non_flag(rest)),
        "dotnet" => classify_dotnet(first_non_flag(rest)),
        "make" | "cmake" | "ninja" => "Building".to_string(),
        "msbuild" => "Building .NET".to_string(),
        "gradle" | "mvn" | "ant" => "Building".to_string(),

        // ── Python ──────────────────────────────────────────────────────────
        "python" | "python3" | "py" => classify_python(rest),

        // ── Node / TypeScript ───────────────────────────────────────────────
        "node" | "ts-node" | "tsx" | "deno" => classify_node(rest),

        // ── Shell scripts ───────────────────────────────────────────────────
        "bash" | "sh" | "zsh" | "fish" => classify_script(rest),
        "powershell" | "pwsh" => classify_script(rest),

        // ── Git / GitHub CLI ─────────────────────────────────────────────────
        "git" => classify_git(rest),
        "gh" => classify_gh(cmd),

        // ── File reading ────────────────────────────────────────────────────
        "cat" | "type" | "head" | "tail" | "less" | "more" => {
            let target = first_non_flag(rest);
            if looks_like_path(target) {
                format!("Reading: {}", basename(target))
            } else {
                "Reading file".to_string()
            }
        }
        "get-content" | "gc" => classify_get_content(rest),

        // ── Directory listing ───────────────────────────────────────────────
        "ls" | "dir" | "get-childitem" | "gci" | "exa" | "lsd" => {
            "Listing directory".to_string()
        }

        // ── Search ──────────────────────────────────────────────────────────
        "grep" | "ag" | "ack" | "fgrep" | "egrep" => "Searching code".to_string(),
        "rg" | "ripgrep" => classify_rg(rest),
        "find" => "Searching files".to_string(),

        // ── File manipulation ───────────────────────────────────────────────
        "mkdir" | "md" | "new-item" => "Creating directory".to_string(),
        "rm" | "del" | "remove-item" | "ri" => "Removing files".to_string(),
        "cp" | "copy" | "copy-item" => "Copying files".to_string(),
        "mv" | "move" | "move-item" => "Moving files".to_string(),
        "touch" | "new-file" => "Creating file".to_string(),
        "chmod" | "chown" | "attrib" => "Setting permissions".to_string(),
        "rename" | "ren" => "Renaming file".to_string(),

        // ── Linting / formatting ─────────────────────────────────────────────
        "eslint" | "tslint" | "oxlint" => "Linting JS".to_string(),
        "prettier" => "Formatting code".to_string(),
        "black" | "ruff" | "autopep8" | "isort" => "Formatting Python".to_string(),
        "flake8" | "pylint" | "bandit" => "Linting Python".to_string(),
        "mypy" | "pyright" | "pytype" => "Type checking".to_string(),
        "tsc" => {
            if rest.contains("--noEmit") || rest.contains("--check") {
                "Type checking".to_string()
            } else {
                "Compiling TypeScript".to_string()
            }
        }
        "cargo-fmt" | "gofmt" | "rustfmt" => "Formatting code".to_string(),

        // ── Package install ──────────────────────────────────────────────────
        "pip" | "pip3" | "poetry" | "uv" | "pipenv" | "conda" => classify_pip(rest),

        // ── Docker / containers ──────────────────────────────────────────────
        "docker" => classify_docker(first_non_flag(rest)),
        "docker-compose" | "podman" | "kubectl" => "Container operation".to_string(),

        // ── Network ──────────────────────────────────────────────────────────
        "curl" | "wget" | "http" | "httpie" => "Fetching URL".to_string(),
        "ping" => "Checking connectivity".to_string(),

        // ── Environment / process inspection ─────────────────────────────────
        "which" | "where" | "command" => "Checking tool availability".to_string(),
        "env" | "printenv" | "set" => "Checking environment".to_string(),
        "ps" | "top" | "htop" | "tasklist" => "Checking processes".to_string(),
        "kill" | "taskkill" | "stop-process" => "Stopping process".to_string(),

        // ── Noise (silent) ───────────────────────────────────────────────────
        // echo/printf alone are not meaningful actions; keep the previous label.
        "echo" | "printf" | "write-output" | "write-host" => {
            "Running command".to_string()
        }

        // ── Fallback: show just the verb, not the full raw command ────────────
        _ => {
            // If the verb looks like a script path, describe it as such.
            if verb.ends_with(".sh") || verb.ends_with(".ps1") || verb.ends_with(".py") || verb.starts_with("./") || verb.starts_with(".\\") {
                format!("Running script: {}", basename(verb))
            } else {
                format!("Running: {}", truncate(verb, 32))
            }
        }
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn parse_digits_before(s: &str, suffix: &str) -> Option<u32> {
    let idx = s.find(suffix)?;
    let before = s[..idx].trim_end();
    let num: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if num.is_empty() {
        return None;
    }
    num.parse().ok()
}

fn parse_test_result(response: &str) -> Option<(u32, u32)> {
    let s = strip_ansi(response);
    let passed = parse_digits_before(&s, " passed");
    let failed = parse_digits_before(&s, " failed");
    match (passed, failed) {
        (Some(p), f) => Some((p, f.unwrap_or(0))),
        (None, Some(f)) => Some((0, f)),
        _ => None,
    }
}

fn should_parse_commit_hash(last_cmd: &str) -> bool {
    if last_cmd.is_empty() {
        return false;
    }
    let cmd = strip_shell_noise(last_cmd).to_ascii_lowercase();
    cmd.contains("git commit")
        || cmd.contains("git push")
        || cmd.contains("git cherry-pick")
        || cmd.contains("git merge")
        || cmd.contains("git rebase")
        || cmd.contains("git am")
}

fn parse_commit_hash(response: &str) -> Option<String> {
    for line in response.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let inner = &line[1..line.len() - 1];
            let mut parts = inner.split_whitespace();
            let _branch = parts.next()?;
            let hash = parts.next()?;
            if (6..=12).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(hash.chars().take(8).collect());
            }
            continue;
        }
        let hash: String = line
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if (7..=12).contains(&hash.len()) {
            return Some(hash.chars().take(8).collect());
        }
    }
    None
}

fn parse_gh_pr_post(entry: &mut Session, response: &str, at_ms: u64) {
    let trimmed = response.trim();
    if !trimmed.contains("\"number\":") || !trimmed.contains("\"headRefOid\":") {
        return;
    }

    let number = extract_json_u32(trimmed, "\"number\":");
    let oid = extract_json_string(trimmed, "\"headRefOid\":");
    let Some(number) = number else { return };

    if let Some(ref oid_full) = oid {
        let short: String = oid_full
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(8)
            .collect();
        if short.len() >= 7 && entry.last_commit_hash.is_none() {
            entry.last_commit_hash = Some(short);
        }
    }

    let mut summary = format!("PR #{number}");
    if let Some(label) = extract_first_label_name(trimmed) {
        summary.push_str(" · ");
        summary.push_str(&label);
    }
    entry.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, "");
}

fn parse_gh_issue_post(entry: &mut Session, response: &str, at_ms: u64) {
    let trimmed = response.trim();
    if !trimmed.contains("\"title\":") || !trimmed.contains("\"state\":") {
        return;
    }
    if trimmed.contains("\"headRefOid\":") {
        return;
    }

    let number = extract_json_u32(trimmed, "\"number\":");
    let title = extract_json_string(trimmed, "\"title\":");
    let state = extract_json_string(trimmed, "\"state\":");

    let mut summary = if let Some(n) = number {
        format!("Issue #{n}")
    } else {
        "Issue".to_string()
    };
    if let Some(ref st) = state {
        summary.push_str(" · ");
        summary.push_str(st);
    }
    if let Some(ref t) = title {
        summary.push_str(" · ");
        summary.push_str(&truncate(t, 40));
    }
    entry.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, "");
}

fn is_rg_match_line(line: &str) -> bool {
    if line
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() && line.contains(':'))
    {
        return true;
    }
    let parts: Vec<&str> = line.splitn(3, ':').collect();
    parts.len() >= 2
        && !parts[1].is_empty()
        && parts[1].chars().all(|c| c.is_ascii_digit())
}

fn parse_rg_post(entry: &mut Session, response: &str, at_ms: u64) {
    let mut matches: Vec<&str> = Vec::new();
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if is_rg_match_line(line) {
            matches.push(line);
        }
    }
    if matches.is_empty() {
        return;
    }
    let summary = if matches.len() == 1 {
        let content = matches[0]
            .split_once(':')
            .map(|(_, r)| r.trim())
            .unwrap_or(matches[0]);
        format!("Match: {}", truncate(content, 60))
    } else {
        format!("Found {} matches", matches.len())
    };
    entry.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, "");
}

fn parse_git_log_post(entry: &mut Session, response: &str, at_ms: u64) {
    let mut commits = 0u32;
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((hash, _msg)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if (7..=12).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            commits += 1;
        }
    }
    if commits == 0 {
        return;
    }
    let summary = format!("Git history ({commits} commits)");
    entry.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, "");
}

fn parse_git_blame_post(entry: &mut Session, response: &str, at_ms: u64) {
    if !response.contains('(') || !response.to_ascii_lowercase().contains("author") {
        return;
    }
    let path = response
        .lines()
        .find_map(|l| {
            let l = l.trim();
            if looks_like_path(l) {
                Some(basename(l).to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "file".to_string());
    let summary = format!("Blamed lines in {path}");
    entry.push_activity_with_kind(summary, at_ms, ActivityKind::Normal, "");
}

fn parse_node_probe_post(entry: &mut Session, response: &str, at_ms: u64) {
    for line in response.lines() {
        let line = line.trim();
        let Some(case_name) = line.strip_prefix("CASE:") else {
            continue;
        };
        let case_name = case_name.trim();
        if case_name.is_empty() {
            continue;
        }
        let summary = format!("Type check: {}", truncate(case_name, 48));
        entry.push_activity_with_kind(summary, at_ms, ActivityKind::Success, "");
        return;
    }
}

fn extract_json_u32(s: &str, key: &str) -> Option<u32> {
    let idx = s.find(key)?;
    let rest = s[idx + key.len()..].trim_start();
    let num: String = rest
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num.parse().ok()
}

fn extract_json_string(s: &str, key: &str) -> Option<String> {
    let idx = s.find(key)?;
    let rest = s[idx + key.len()..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_first_label_name(s: &str) -> Option<String> {
    let labels_idx = s.find("\"labels\"")?;
    let slice = &s[labels_idx..];
    let name_key = "\"name\":\"";
    let idx = slice.find(name_key)?;
    let rest = &slice[idx + name_key.len()..];
    let end = rest.find('"')?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_done_summary(msg: &str) -> String {
    let first = msg
        .trim()
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if first.is_empty() {
        return String::new();
    }
    truncate_at_word_boundary(first, 100)
}

fn truncate_at_word_boundary(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    if let Some(pos) = truncated.rfind(char::is_whitespace) {
        if pos > max_chars / 2 {
            let mut out: String = truncated.chars().take(pos).collect();
            out.push('…');
            return out.trim().to_string();
        }
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn rest_after_first_token(s: &str) -> &str {
    let s = s.trim();
    let pos = s.find(char::is_whitespace).unwrap_or(s.len());
    s[pos..].trim()
}

fn next_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((&s[..end], s[end..].trim_start()))
}

fn strip_npm_workspace_flags(mut rest: &str) -> &str {
    loop {
        let (token, remainder) = match next_token(rest) {
            Some(t) => t,
            None => return rest,
        };
        match token {
            "-C" | "--filter" => {
                let (_, after_arg) = next_token(remainder).unwrap_or((token, remainder));
                rest = after_arg;
            }
            "--workspace-root" | "-w" => {
                rest = remainder;
            }
            _ => return rest,
        }
    }
}

fn classify_nx(sub: &str) -> String {
    match sub {
        "typecheck" | "type-check" => "Type checking".to_string(),
        "lint" => "Linting".to_string(),
        "test" => "Running tests".to_string(),
        "build" => "Building".to_string(),
        "serve" | "dev" => "Starting dev server".to_string(),
        _ => format!("Running: {}", truncate(sub, 24)),
    }
}

fn classify_exec(binary: &str) -> String {
    classify_bash(binary)
}

fn classify_npm(rest: &str) -> String {
    let rest = strip_npm_workspace_flags(rest);
    let sub = first_non_flag(rest);
    match sub {
        "test" => "Running tests".to_string(),
        "build" => "Building".to_string(),
        "install" | "i" | "ci" | "add" => "Installing dependencies".to_string(),
        "start" => "Starting server".to_string(),
        "publish" => "Publishing package".to_string(),
        "lint" => "Linting".to_string(),
        "run" => {
            // `npm run <script>` — the script name is the meaningful bit.
            let script = rest.split_whitespace()
                .skip_while(|t| t.starts_with('-') || *t == "run")
                .next()
                .unwrap_or("");
            match script {
                "dev" | "start" | "serve" | "preview" => "Starting dev server".to_string(),
                "build" => "Building".to_string(),
                "test" | "jest" | "vitest" => "Running tests".to_string(),
                "lint" | "check" => "Linting".to_string(),
                "format" | "fmt" | "prettier" => "Formatting code".to_string(),
                "typecheck" | "type-check" | "tsc" | "ts" => "Type checking".to_string(),
                "migrate" | "db:migrate" | "migration" => "Running migration".to_string(),
                "seed" | "db:seed" => "Seeding database".to_string(),
                "storybook" => "Starting Storybook".to_string(),
                "" => "Running npm".to_string(),
                name => format!("Running: {}", truncate(name, 24)),
            }
        }
        "nx" | "turbo" => classify_nx(first_non_flag(rest_after_first_token(rest))),
        "exec" | "x" => classify_exec(first_non_flag(rest_after_first_token(rest))),
        _ => "Running npm".to_string(),
    }
}

fn classify_cargo(sub: &str) -> String {
    match sub {
        "test" => "Running tests".to_string(),
        "build" | "b" => "Building with Cargo".to_string(),
        "run" | "r" => "Running Rust binary".to_string(),
        "check" | "c" => "Checking Rust code".to_string(),
        "clippy" => "Linting Rust code".to_string(),
        "fmt" => "Formatting Rust code".to_string(),
        "doc" => "Generating docs".to_string(),
        "publish" => "Publishing crate".to_string(),
        "install" => "Installing Rust binary".to_string(),
        "bench" => "Running benchmarks".to_string(),
        _ => "Running Cargo".to_string(),
    }
}

fn classify_go(sub: &str) -> String {
    match sub {
        "test" => "Running Go tests".to_string(),
        "build" => "Building Go binary".to_string(),
        "run" => "Running Go program".to_string(),
        "fmt" => "Formatting Go code".to_string(),
        "vet" => "Checking Go code".to_string(),
        "get" | "install" => "Installing Go package".to_string(),
        "mod" => "Managing Go modules".to_string(),
        _ => "Running Go".to_string(),
    }
}

fn classify_dotnet(sub: &str) -> String {
    match sub {
        "test" => "Running tests".to_string(),
        "build" => "Building .NET".to_string(),
        "run" => "Running .NET app".to_string(),
        "publish" => "Publishing .NET".to_string(),
        "restore" => "Restoring packages".to_string(),
        "format" => "Formatting .NET code".to_string(),
        _ => "Running dotnet".to_string(),
    }
}

fn classify_python(rest: &str) -> String {
    let mut tokens = rest.split_whitespace();
    let arg1 = tokens.next().unwrap_or("");
    let arg2 = tokens.next().unwrap_or("");
    match arg1 {
        "-m" => match arg2 {
            "pytest" | "unittest" | "nose" | "nose2" => "Running Python tests".to_string(),
            "pip" => classify_pip(tokens.next().unwrap_or("")),
            "http.server" | "SimpleHTTPServer" => "Starting HTTP server".to_string(),
            "black" | "ruff" => "Formatting Python".to_string(),
            "mypy" | "pyright" => "Type checking".to_string(),
            name => format!("Running: {}", truncate(name, 24)),
        },
        "-c" => "Running inline script".to_string(),
        a if a.ends_with(".py") => format!("Running: {}", basename(a)),
        _ => "Running Python".to_string(),
    }
}

fn classify_node(rest: &str) -> String {
    let arg = first_non_flag(rest);
    if arg.ends_with(".js") || arg.ends_with(".ts") || arg.ends_with(".mjs") || arg.ends_with(".cjs") {
        format!("Running: {}", basename(arg))
    } else if arg == "-e" || arg == "--eval" {
        "Running inline script".to_string()
    } else {
        "Running Node".to_string()
    }
}

fn classify_script(rest: &str) -> String {
    let arg = first_non_flag(rest);
    if arg.ends_with(".sh") {
        format!("Running script: {}", basename(arg))
    } else if arg.ends_with(".ps1") {
        format!("Running script: {}", basename(arg))
    } else if arg.ends_with(".py") {
        format!("Running: {}", basename(arg))
    } else if arg == "-c" || arg == "-Command" || arg == "-command" {
        "Running command".to_string()
    } else {
        "Running script".to_string()
    }
}

fn classify_git(sub: &str) -> String {
    let sub_lc = sub.to_ascii_lowercase();
    if sub_lc.starts_with("log") && sub.split_whitespace().any(|t| t.eq_ignore_ascii_case("-s")) {
        return "Searching git history".to_string();
    }
    if sub_lc.starts_with("blame") && sub.split_whitespace().any(|t| t.eq_ignore_ascii_case("-l")) {
        if let Some(path) = git_blame_path(sub) {
            return format!("Blaming lines in {}", basename(&path));
        }
        return "Blaming lines".to_string();
    }
    let verb = first_non_flag(sub);
    match verb {
        "status" | "diff" | "log" | "show" | "blame" | "shortlog" => {
            "Checking git state".to_string()
        }
        "branch" => classify_git_branch(sub),
        "add" | "commit" => "Committing changes".to_string(),
        "push" => "Pushing to remote".to_string(),
        "pull" | "fetch" => "Fetching from remote".to_string(),
        "clone" => "Cloning repository".to_string(),
        "checkout" | "switch" => "Switching branch".to_string(),
        "merge" | "rebase" | "cherry-pick" => "Merging branches".to_string(),
        "stash" => "Stashing changes".to_string(),
        "reset" | "revert" => "Undoing changes".to_string(),
        "tag" => "Tagging commit".to_string(),
        "remote" => "Managing remotes".to_string(),
        _ => "Git operation".to_string(),
    }
}

fn git_blame_path(sub: &str) -> Option<String> {
    let tokens: Vec<&str> = sub.split_whitespace().collect();
    let mut after_dashes = false;
    for t in tokens.iter().skip(1) {
        if *t == "--" {
            after_dashes = true;
            continue;
        }
        if after_dashes && !t.starts_with('-') && looks_like_path(t) {
            return Some((*t).to_string());
        }
    }
    tokens
        .iter()
        .rev()
        .find(|t| !t.starts_with('-') && looks_like_path(t))
        .map(|s| (*s).to_string())
}

fn classify_get_content(rest: &str) -> String {
    let mut skip: Option<u32> = None;
    let mut first: Option<u32> = None;
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].to_ascii_lowercase();
        if t == "-skip" && i + 1 < tokens.len() {
            skip = tokens[i + 1].parse().ok();
            i += 2;
            continue;
        }
        if t == "-first" && i + 1 < tokens.len() {
            first = tokens[i + 1].parse().ok();
            i += 2;
            continue;
        }
        i += 1;
    }
    let path = tokens
        .iter()
        .rev()
        .find(|t| looks_like_path(t))
        .map(|p| basename(p).to_string())
        .unwrap_or_else(|| "file".to_string());
    if let (Some(s), Some(f)) = (skip, first) {
        let start = s + 1;
        let end = s + f;
        return format!("Reading lines {start}–{end} of {path}");
    }
    if let Some(s) = skip {
        return format!("Reading from line {} of {path}", s + 1);
    }
    format!("Reading: {path}")
}

fn classify_rg(rest: &str) -> String {
    let path = rest
        .split_whitespace()
        .rev()
        .find(|t| looks_like_path(t))
        .map(|p| basename(p).to_string());
    if let Some(p) = path {
        format!("Searching {p}")
    } else {
        "Searching code".to_string()
    }
}

fn classify_git_branch(rest: &str) -> String {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let has_delete = tokens.iter().any(|t| *t == "-d" || *t == "-D");
    if has_delete {
        if let Some(name) = git_branch_name_after_delete_flag(rest) {
            return format!("Deleting branch {}", truncate(&name, 40));
        }
        return "Deleting branch".to_string();
    }
    let has_force = tokens.iter().any(|t| *t == "-f" || *t == "-M" || *t == "-m");
    if has_force {
        return "Updating branch".to_string();
    }
    "Listing branches".to_string()
}

fn git_branch_name_after_delete_flag(rest: &str) -> Option<String> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if *t == "-d" || *t == "-D" {
            for next in tokens.iter().skip(i + 1) {
                if !next.starts_with('-') && !next.is_empty() {
                    return Some(next.to_string());
                }
            }
        }
    }
    tokens
        .iter()
        .rev()
        .find(|t| !t.starts_with('-') && **t != "branch")
        .map(|s| s.to_string())
}

fn parse_gh_issue_number(cmd: &str) -> Option<u32> {
    let lower = cmd.to_ascii_lowercase();
    if let Some(idx) = lower.find("issues/") {
        let rest = &lower[idx + 7..];
        let num: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return num.parse().ok();
    }
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if t.eq_ignore_ascii_case("issue") && i + 1 < tokens.len() {
            let sub = tokens[i + 1];
            if sub.eq_ignore_ascii_case("view") && i + 2 < tokens.len() {
                let n = tokens[i + 2].trim_matches(|c: char| !c.is_ascii_digit());
                if let Ok(num) = n.parse::<u32>() {
                    return Some(num);
                }
            }
        }
    }
    None
}

fn extract_gh_repo_flag(cmd: &str) -> Option<String> {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if *t == "--repo" && i + 1 < tokens.len() {
            return Some(tokens[i + 1].to_string());
        }
    }
    None
}

fn parse_gh_pr_number(cmd: &str) -> Option<u32> {
    let lower = cmd.to_ascii_lowercase();
    if let Some(idx) = lower.find("pulls/") {
        let rest = &lower[idx + 6..];
        let num: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return num.parse().ok();
    }
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        if t.eq_ignore_ascii_case("pr") && i + 1 < tokens.len() {
            let sub = tokens[i + 1];
            if matches!(
                sub.to_ascii_lowercase().as_str(),
                "view" | "edit" | "comment"
            ) && i + 2 < tokens.len()
            {
                let n = tokens[i + 2].trim_matches(|c: char| !c.is_ascii_digit());
                if let Ok(num) = n.parse::<u32>() {
                    return Some(num);
                }
            }
        }
    }
    None
}

fn classify_gh(cmd: &str) -> String {
    let lower = cmd.to_ascii_lowercase();
    if lower.contains("pr create") {
        return "Creating PR".to_string();
    }
    if lower.contains("pr view") {
        if let Some(n) = parse_gh_pr_number(cmd) {
            return format!("Viewing PR #{n}");
        }
        return "Viewing PR".to_string();
    }
    if lower.contains("issue view") {
        if let Some(n) = parse_gh_issue_number(cmd) {
            let mut label = format!("Viewing issue #{n}");
            if let Some(repo) = extract_gh_repo_flag(cmd) {
                label.push_str(" · ");
                label.push_str(&truncate(&repo, 40));
            }
            return label;
        }
        return "Viewing issue".to_string();
    }
    if lower.contains("pr edit") {
        if let Some(n) = parse_gh_pr_number(cmd) {
            return format!("Updating PR #{n}");
        }
        return "Updating PR".to_string();
    }
    if lower.contains("api") {
        if lower.contains("requested_reviewers") {
            return "Requesting PR review".to_string();
        }
        if let Some(n) = parse_gh_pr_number(cmd) {
            return format!("GitHub API · PR #{n}");
        }
        return "GitHub API".to_string();
    }
    "GitHub CLI".to_string()
}

fn classify_pip(rest: &str) -> String {
    let sub = first_non_flag(rest);
    match sub {
        "install" => "Installing dependencies".to_string(),
        "uninstall" => "Removing package".to_string(),
        "freeze" | "list" => "Listing packages".to_string(),
        "show" => "Checking package info".to_string(),
        _ => "Running pip".to_string(),
    }
}

fn classify_docker(sub: &str) -> String {
    match sub {
        "build" => "Building Docker image".to_string(),
        "run" => "Running container".to_string(),
        "ps" => "Listing containers".to_string(),
        "pull" => "Pulling image".to_string(),
        "push" => "Pushing image".to_string(),
        "exec" => "Running in container".to_string(),
        "logs" => "Reading container logs".to_string(),
        "compose" => "Docker Compose operation".to_string(),
        "stop" | "kill" | "rm" | "rmi" => "Managing containers".to_string(),
        _ => "Docker operation".to_string(),
    }
}

/// First whitespace-delimited token that does not start with `-`.
fn first_non_flag(s: &str) -> &str {
    s.split_whitespace()
        .find(|t| !t.starts_with('-'))
        .unwrap_or("")
}

/// Last path component of a file path (handles both `/` and `\`).
fn basename(p: &str) -> &str {
    p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p)
}

/// Heuristic: does this token look like a file/directory path?
fn looks_like_path(s: &str) -> bool {
    !s.is_empty()
        && (s.contains('.')
            || s.starts_with('/')
            || s.starts_with('\\')
            || s.starts_with("..")
            || s.contains('/')
            || s.contains('\\'))
}

// ── end Bash command summarizer ──────────────────────────────────────────────

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_bash_gh_issue_view() {
        let cmd = "gh issue view 8737 --repo typescript-eslint/typescript-eslint";
        assert_eq!(describe_bash(cmd), "Viewing issue #8737 · typescript-eslint/typescript-eslint");
    }

    #[test]
    fn describe_bash_heredoc_pipe_node() {
        let cmd = "@'\nconsole.log(1)\n'@ | node";
        let label = describe_bash(cmd);
        assert!(
            label.contains("inline script") || label.contains("Node") || label.contains("Running"),
            "unexpected: {label}"
        );
    }

    #[test]
    fn describe_bash_git_log_s() {
        assert_eq!(describe_bash("git log -S foo --oneline"), "Searching git history");
    }

    #[test]
    fn describe_bash_git_blame_l() {
        assert_eq!(
            describe_bash("git blame -L 10,20 -- src/foo.ts"),
            "Blaming lines in foo.ts"
        );
    }

    #[test]
    fn describe_bash_get_content_line_range() {
        assert_eq!(
            describe_bash("Get-Content foo.ts -Skip 680 -First 260"),
            "Reading lines 681–940 of foo.ts"
        );
    }

    #[test]
    fn describe_bash_rg_basename() {
        assert_eq!(
            describe_bash("rg no-unnecessary-type-assertion src/rules/no-unnecessary-type-assertion.ts"),
            "Searching no-unnecessary-type-assertion.ts"
        );
    }

    #[test]
    fn format_parallel_summary_joins_and_caps() {
        let summaries = vec![
            "Searching a.ts".to_string(),
            "Reading lines 1–2 of b.ts".to_string(),
            "Viewing issue #1".to_string(),
            "Listing directory".to_string(),
        ];
        let out = format_parallel_summary(&summaries);
        assert!(out.starts_with("Parallel: "));
        assert!(out.contains(" · "));
        assert!(out.contains("+1 more"));
    }

    #[test]
    fn activity_dedupe_key_normalizes() {
        let a = activity_dedupe_key("  GIT   status  ");
        let b = activity_dedupe_key("git status");
        assert_eq!(a, b);
    }

    #[test]
    fn should_parse_commit_hash_only_for_commit_ops() {
        assert!(should_parse_commit_hash("git commit -m msg"));
        assert!(should_parse_commit_hash("git push origin main"));
        assert!(!should_parse_commit_hash("git log --oneline -5"));
    }

    #[test]
    fn parse_gh_issue_post_pushes_activity() {
        let mut s = Session::new("t".into(), "/tmp".into(), None, 0);
        let json = r#"{"number":8737,"title":"Bug report","state":"closed"}"#;
        parse_gh_issue_post(&mut s, json, 1);
        assert_eq!(s.recent_activity.len(), 1);
        assert!(s.recent_activity[0].summary.contains("Issue #8737"));
        assert!(s.recent_activity[0].summary.contains("closed"));
    }

    #[test]
    fn parse_rg_post_counts_matches() {
        let mut s = Session::new("t".into(), "/tmp".into(), None, 0);
        let out = "src/a.ts:10: match one\nsrc/a.ts:20: match two\n";
        parse_rg_post(&mut s, out, 1);
        assert_eq!(s.recent_activity.len(), 1);
        assert!(s.recent_activity[0].summary.contains("2 matches"));
    }

    #[test]
    fn parse_commit_hash_from_log_output_denied() {
        let log_out = "a1b2c3d Fix something\n";
        assert!(parse_commit_hash(log_out).is_some());
        assert!(!should_parse_commit_hash("git log --oneline"));
    }
}
