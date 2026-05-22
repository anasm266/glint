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
    #[serde(skip)]
    pub last_turn_id: String,
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
            last_turn_id: String::new(),
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
        }
        "UserPromptSubmit" => {
            entry.status = Status::Working;
            entry.current_action = "Thinking…".to_string();
            entry.acknowledged_done = false;
            entry.started_at_ms = raw.ts;
            entry.last_prompt = String::new();
            entry.recent_activity.clear();
            entry.next_activity_seq = 0;
            if let Some(prompt) = extract_user_prompt(p) {
                let t = prompt.trim();
                if !t.is_empty() {
                    entry.last_prompt = truncate_prompt(t.to_string());
                }
            }
        }
        "PreToolUse" => {
            entry.status = Status::Working;
            let action = describe_pre_tool(p);
            let turn_id = p.get("turn_id").and_then(|v| v.as_str()).unwrap_or("");
            entry.current_action = action.clone();
            entry.push_activity_with_kind(action, raw.ts, ActivityKind::Normal, turn_id);
            entry.last_turn_id = turn_id.to_string();
        }
        "PostToolUse" => {
            track_files_from_post(entry, p);
            let tool = p.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
            let response = p
                .get("tool_response")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if tool == "Bash" {
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
                if let Some(hash) = parse_commit_hash(response) {
                    entry.last_commit_hash = Some(hash);
                }
            }
            entry.current_action = "Thinking…".to_string();
        }
        "Stop" => {
            entry.status = Status::Done;
            entry.current_action = "Done".to_string();
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
    let cmd = strip_shell_noise(raw);
    classify_bash(cmd)
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

        // ── Git ─────────────────────────────────────────────────────────────
        "git" => classify_git(first_non_flag(rest)),

        // ── File reading ────────────────────────────────────────────────────
        "cat" | "type" | "get-content" | "gc" | "head" | "tail" | "less" | "more" => {
            let target = first_non_flag(rest);
            if looks_like_path(target) {
                format!("Reading: {}", basename(target))
            } else {
                "Reading file".to_string()
            }
        }

        // ── Directory listing ───────────────────────────────────────────────
        "ls" | "dir" | "get-childitem" | "gci" | "exa" | "lsd" => {
            "Listing directory".to_string()
        }

        // ── Search ──────────────────────────────────────────────────────────
        "grep" | "rg" | "ripgrep" | "ag" | "ack" | "fgrep" | "egrep" => {
            "Searching code".to_string()
        }
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

fn parse_commit_hash(response: &str) -> Option<String> {
    for line in response.lines() {
        let line = line.trim();
        if !line.starts_with('[') || !line.ends_with(']') {
            continue;
        }
        let inner = &line[1..line.len() - 1];
        let mut parts = inner.split_whitespace();
        let _branch = parts.next()?;
        let hash = parts.next()?;
        if (6..=12).contains(&hash.len()) && hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hash.chars().take(8).collect());
        }
    }
    None
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
    match sub {
        "status" | "diff" | "log" | "show" | "blame" | "shortlog" => {
            "Checking git state".to_string()
        }
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
