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
            parent_pid,
            files_edited_map: HashMap::new(),
        }
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

    match raw.event.as_str() {
        "SessionStart" => {
            entry.status = Status::Working;
            entry.current_action = "Thinking…".to_string();
            entry.acknowledged_done = false;
            entry.started_at_ms = raw.ts;
            entry.last_prompt = String::new();
        }
        "UserPromptSubmit" => {
            entry.status = Status::Working;
            entry.current_action = "Thinking…".to_string();
            entry.acknowledged_done = false;
            entry.started_at_ms = raw.ts;
            entry.last_prompt = String::new();
            if let Some(prompt) = extract_user_prompt(p) {
                let t = prompt.trim();
                if !t.is_empty() {
                    entry.last_prompt = truncate_prompt(t.to_string());
                }
            }
        }
        "PreToolUse" => {
            entry.status = Status::Working;
            entry.current_action = describe_pre_tool(p);
        }
        "PostToolUse" => {
            // Track file edits when we can derive them; otherwise leave action
            // alone (PreToolUse already set it).
            track_files_from_post(entry, p);
            // Keep working status; "Thinking…" between tools feels accurate.
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
                format!("Running: {}", truncate(cmd, 64))
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

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}
