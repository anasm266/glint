//! Session model + state machine for hook events.
//!
//! We map raw hook events from Codex into a small, stable shape the UI
//! consumes. The mapping is intentionally lossy: we strip everything the
//! compact view doesn't need.

use crate::state::SessionRouting;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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

#[derive(Debug, Clone)]
struct PendingStop {
    last_assistant_message: Option<String>,
    status: Option<String>,
    error_message: Option<String>,
}

impl PendingStop {
    fn from_payload(p: &serde_json::Value) -> Self {
        Self {
            last_assistant_message: p
                .get("last_assistant_message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            status: p
                .get("status")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            error_message: p
                .get("error_message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }
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
    /// Background Task subagents still running (Cursor multitask).
    #[serde(skip)]
    pub active_subagent_count: u32,
    /// Child conversation ids linked to this parent (multitask rollup).
    #[serde(skip)]
    pub active_child_conversations: HashSet<String>,
    /// Parent `stop` deferred until subagents/children quiesce.
    #[serde(skip)]
    pending_stop: Option<PendingStop>,
}

impl Session {
    pub fn new(
        id: String,
        cwd: String,
        app: App,
        parent_pid: Option<u32>,
        ts_ms: u64,
    ) -> Self {
        let project = project_basename(&cwd);
        Self {
            id,
            app,
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
            active_subagent_count: 0,
            active_child_conversations: HashSet::new(),
            pending_stop: None,
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
    label_command_segment(raw_cmd)
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
    let structural: &[&str] = &[
        "session_id",
        "conversation_id",
        "cwd",
        "hook_event_name",
        "workspace_roots",
    ];
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

fn normalize_hook_event(name: &str) -> String {
    match name {
        "sessionStart" | "SessionStart" => "SessionStart".to_string(),
        "sessionEnd" | "SessionEnd" => "SessionEnd".to_string(),
        "stop" | "Stop" => "Stop".to_string(),
        "preToolUse" | "PreToolUse" => "PreToolUse".to_string(),
        "postToolUse" | "PostToolUse" => "PostToolUse".to_string(),
        "beforeSubmitPrompt" | "UserPromptSubmit" => "UserPromptSubmit".to_string(),
        "afterFileEdit" | "AfterFileEdit" => "AfterFileEdit".to_string(),
        "subagentStart" | "SubagentStart" => "SubagentStart".to_string(),
        "subagentStop" | "SubagentStop" => "SubagentStop".to_string(),
        other => other.to_string(),
    }
}

fn raw_conversation_id(p: &serde_json::Value) -> String {
    p.get("conversation_id")
        .or_else(|| p.get("session_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn resolve_session_id(p: &serde_json::Value) -> String {
    let raw = raw_conversation_id(p);
    if let Some(parent) = p.get("parent_conversation_id").and_then(|v| v.as_str()) {
        if !parent.is_empty() && parent != raw {
            return parent.to_string();
        }
    }
    raw
}

fn task_run_in_background(p: &serde_json::Value) -> bool {
    p.get("tool_name")
        .and_then(|v| v.as_str())
        .map(|t| t == "Task")
        .unwrap_or(false)
        && p.get("tool_input")
            .and_then(|v| v.get("run_in_background"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
}

fn unlink_child(
    routing: &mut SessionRouting,
    entry: &mut Session,
    child_id: &str,
    parent_id: &str,
) {
    if child_id.is_empty() || child_id == parent_id {
        return;
    }
    routing.remove_child(child_id);
    routing.child_last_event_ms.remove(child_id);
    entry.active_child_conversations.remove(child_id);
}

fn track_child_activity(
    routing: &mut SessionRouting,
    entry: &mut Session,
    child_id: &str,
    parent_id: &str,
    ts: u64,
) {
    if child_id.is_empty() || child_id == parent_id {
        return;
    }
    routing.touch_child(child_id, ts);
    entry.active_child_conversations.insert(child_id.to_string());
}

fn refresh_action_label(entry: &mut Session) {
    if entry.status == Status::Done || entry.status == Status::Errored {
        return;
    }
    if entry.active_subagent_count > 0 {
        entry.current_action = format!("{} subagents running", entry.active_subagent_count);
        return;
    }
    if !entry.active_child_conversations.is_empty() {
        entry.current_action = "Subagents finishing…".to_string();
        return;
    }
    if entry.current_action != "Thinking…" && !entry.current_action.is_empty() {
        return;
    }
    if let Some(first) = entry.recent_activity.first() {
        entry.current_action = truncate(&first.summary, 40);
    } else {
        entry.current_action = "Thinking…".to_string();
    }
}

fn apply_pending_stop(entry: &mut Session, pending: &PendingStop) {
    entry.pending_stop = None;
    entry.active_subagent_count = 0;
    entry.active_child_conversations.clear();
    if let Some(msg) = pending.last_assistant_message.as_deref() {
        entry.status = Status::Done;
        let summary = extract_done_summary(msg);
        if !summary.is_empty() {
            entry.done_summary = Some(summary.clone());
            entry.current_action = summary;
        } else {
            entry.current_action = "Done".to_string();
        }
    } else if let Some(status) = pending.status.as_deref() {
        match status {
            "completed" => {
                entry.status = Status::Done;
                entry.current_action = "Done".to_string();
            }
            "aborted" => {
                entry.status = Status::Done;
                entry.done_summary = Some("Stopped".to_string());
                entry.current_action = "Stopped".to_string();
            }
            "error" => {
                entry.status = Status::Errored;
                let msg = pending
                    .error_message
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Error");
                entry.done_summary = Some(msg.to_string());
                entry.current_action = msg.to_string();
            }
            _ => {
                entry.status = Status::Done;
                entry.current_action = "Done".to_string();
            }
        }
    } else {
        entry.status = Status::Done;
        entry.current_action = "Done".to_string();
    }
}

fn try_complete_pending_stop(
    routing: &mut SessionRouting,
    entry: &mut Session,
    ts: u64,
) -> bool {
    if entry.pending_stop.is_none() {
        return false;
    }
    routing.prune_stale_children(entry, ts);
    if entry.active_subagent_count > 0 || !entry.active_child_conversations.is_empty() {
        refresh_action_label(entry);
        return false;
    }
    let pending = entry.pending_stop.take().expect("checked");
    apply_pending_stop(entry, &pending);
    true
}

/// Prune idle child conversations and apply deferred parent `stop` using wall-clock `now`.
pub fn reconcile_pending_stops(
    map: &mut HashMap<String, Session>,
    routing: &mut SessionRouting,
    now: u64,
) -> bool {
    let mut changed = false;
    for entry in map.values_mut() {
        if entry.pending_stop.is_none() {
            continue;
        }
        let pruned = routing.prune_stale_children(entry, now);
        if try_complete_pending_stop(routing, entry, now) {
            changed = true;
        } else if pruned {
            refresh_action_label(entry);
            changed = true;
        }
    }
    changed
}

fn resolve_cwd(p: &serde_json::Value) -> String {
    if let Some(cwd) = p.get("cwd").and_then(|v| v.as_str()) {
        if !cwd.is_empty() {
            return cwd.to_string();
        }
    }
    p.get("workspace_roots")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn detect_app(p: &serde_json::Value) -> App {
    if p.get("conversation_id").and_then(|v| v.as_str()).is_some() {
        App::Cursor
    } else {
        App::Codex
    }
}

fn turn_id_from_payload(p: &serde_json::Value) -> &str {
    p.get("turn_id")
        .or_else(|| p.get("generation_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

fn tool_response_str(p: &serde_json::Value) -> String {
    let raw = p
        .get("tool_response")
        .and_then(|v| v.as_str())
        .or_else(|| p.get("tool_output").and_then(|v| v.as_str()))
        .unwrap_or("");
    unwrap_shell_response(raw)
}

fn path_from_tool_input(input: Option<&serde_json::Value>) -> Option<String> {
    let obj = input?.as_object()?;
    for key in ["path", "file_path", "filePath", "target"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn apply_after_file_edit(entry: &mut Session, p: &serde_json::Value, ts: u64) {
    let path = p
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if path.is_empty() {
        return;
    }
    let mut adds = 0u32;
    let mut dels = 0u32;
    if let Some(edits) = p.get("edits").and_then(|v| v.as_array()) {
        for edit in edits {
            if let Some(old) = edit.get("old_string").and_then(|v| v.as_str()) {
                dels += old.lines().count() as u32;
            }
            if let Some(new) = edit.get("new_string").and_then(|v| v.as_str()) {
                adds += new.lines().count() as u32;
            }
        }
    }
    let stat = entry.files_edited_map.entry(path.to_string()).or_default();
    stat.adds = stat.adds.saturating_add(adds);
    stat.dels = stat.dels.saturating_add(dels);
    entry.flatten_files();

    let label = format!("Edited {}", basename(path));
    entry.current_action = label.clone();
    entry.push_activity_with_kind(label, ts, ActivityKind::Normal, "");
}

/// If overlay-hook could not parse stdin (e.g. BOM slipped through), payload may
/// be a JSON string; coerce so session_id and fields are readable.
fn coerce_payload(value: &serde_json::Value) -> serde_json::Value {
    let Some(s) = value.as_str() else {
        return value.clone();
    };
    let s = s.trim().trim_start_matches('\u{FEFF}');
    parse_hook_json(s).unwrap_or_else(|_| value.clone())
}

fn parse_hook_json(s: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(s.trim().trim_start_matches('\u{FEFF}'))
}

fn resolve_event_name(raw_event: &str, p: &serde_json::Value) -> String {
    if raw_event.eq_ignore_ascii_case("unknown") {
        if let Some(name) = p.get("hook_event_name").and_then(|v| v.as_str()) {
            return normalize_hook_event(name);
        }
    }
    normalize_hook_event(raw_event)
}

/// Apply a raw event to the session map. Returns the session id that was
/// affected (if any) so callers can do follow-up work.
pub fn apply(
    map: &mut HashMap<String, Session>,
    routing: &mut SessionRouting,
    raw: RawEvent,
) -> Option<String> {
    let payload = coerce_payload(&raw.payload);
    let p = &payload;
    let raw_conv = raw_conversation_id(p);
    let mut session_id = routing.resolve_parent(&raw_conv, p);
    if session_id.is_empty() {
        return None;
    }
    if session_id == raw_conv && !raw_conv.is_empty() {
        if let Some(parent) = routing.try_link_orphan_child(&raw_conv, raw.ts) {
            routing.link_child(&raw_conv, &parent);
            session_id = parent;
        }
    }
    if !raw_conv.is_empty() && raw_conv != session_id {
        routing.link_child(&raw_conv, &session_id);
    }

    let cwd = resolve_cwd(p);
    let app = detect_app(p);
    let event = resolve_event_name(&raw.event, p);

    let entry = map.entry(session_id.clone()).or_insert_with(|| {
        Session::new(
            session_id.clone(),
            cwd.clone(),
            app,
            raw.parent_pid,
            raw.ts,
        )
    });

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

    match event.as_str() {
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
            entry.active_subagent_count = 0;
            entry.active_child_conversations.clear();
            entry.pending_stop = None;
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
            entry.active_subagent_count = 0;
            entry.active_child_conversations.clear();
            entry.pending_stop = None;
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
            if task_run_in_background(p) {
                routing.note_task_spawn(&session_id, raw.ts);
            }
            if tool == "Bash" || tool == "Shell" {
                let cmd = p
                    .get("tool_input")
                    .and_then(|v| v.get("command"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                entry.last_bash_command = cmd.to_string();
            }
            let action = describe_pre_tool(p);
            let turn_id = turn_id_from_payload(p);
            if !entry.turn_activity_turn_id.is_empty() && turn_id != entry.turn_activity_turn_id {
                let flush_tid = entry.turn_activity_turn_id.clone();
                entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            }
            if !action.is_empty() {
                let dedupe_key = if tool == "Bash" || tool == "Shell" {
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
            let response = tool_response_str(p);
            if tool == "Bash" || tool == "Shell" {
                parse_gh_pr_post(entry, &response, raw.ts);
                parse_gh_issue_post(entry, &response, raw.ts);
                parse_rg_post(entry, &response, raw.ts);
                parse_git_log_post(entry, &response, raw.ts);
                parse_git_blame_post(entry, &response, raw.ts);
                parse_node_probe_post(entry, &response, raw.ts);
                if let Some((passed, failed)) = parse_test_result(&response) {
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
                    if let Some(hash) = parse_commit_hash(&response) {
                        entry.last_commit_hash = Some(hash);
                    }
                }
            }
            refresh_action_label(entry);
        }
        "SubagentStart" => {
            entry.status = Status::Working;
            entry.acknowledged_done = false;
            let parent_conv = p
                .get("parent_conversation_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !raw_conv.is_empty()
                && !parent_conv.is_empty()
                && raw_conv != parent_conv
            {
                routing.link_child(&raw_conv, &session_id);
                track_child_activity(routing, entry, &raw_conv, &session_id, raw.ts);
            }
            entry.active_subagent_count = entry.active_subagent_count.saturating_add(1);
            let action = describe_subagent_start(p);
            if !action.is_empty() {
                entry.current_action = action;
            } else if entry.active_subagent_count > 1 {
                entry.current_action =
                    format!("{} subagents running", entry.active_subagent_count);
            }
        }
        "SubagentStop" => {
            let flush_tid = entry.turn_activity_turn_id.clone();
            entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            entry.active_subagent_count = entry.active_subagent_count.saturating_sub(1);
            unlink_child(routing, entry, &raw_conv, &session_id);
            routing.prune_stale_children(entry, raw.ts);
            entry.status = Status::Working;
            let (summary, kind) = describe_subagent_stop(p);
            if !summary.is_empty() {
                entry.push_activity_with_kind(summary.clone(), raw.ts, kind, "");
            }
            if entry.active_subagent_count > 0 {
                entry.current_action =
                    format!("{} subagents running", entry.active_subagent_count);
            } else if !summary.is_empty() {
                entry.current_action = truncate(&summary, 40);
            } else {
                refresh_action_label(entry);
            }
            let _ = try_complete_pending_stop(routing, entry, raw.ts);
        }
        "SessionEnd" => {
            let flush_tid = entry.turn_activity_turn_id.clone();
            entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            if raw_conv != session_id {
                unlink_child(routing, entry, &raw_conv, &session_id);
                let _ = try_complete_pending_stop(routing, entry, raw.ts);
            } else {
                entry.active_subagent_count = 0;
                entry.active_child_conversations.clear();
                entry.pending_stop = None;
            }
            apply_session_end(entry, p);
            entry.flatten_files();
        }
        "Stop" => {
            let flush_tid = entry.turn_activity_turn_id.clone();
            entry.flush_turn_activity(raw.ts, flush_tid.as_str());
            if raw_conv != session_id {
                unlink_child(routing, entry, &raw_conv, &session_id);
                let _ = try_complete_pending_stop(routing, entry, raw.ts);
            } else {
                routing.prune_stale_children(entry, raw.ts);
                if entry.active_subagent_count > 0 || !entry.active_child_conversations.is_empty()
                {
                    entry.pending_stop = Some(PendingStop::from_payload(p));
                    entry.status = Status::Working;
                    refresh_action_label(entry);
                    if !try_complete_pending_stop(routing, entry, raw.ts) {
                        return Some(session_id);
                    }
                } else {
                    entry.pending_stop = None;
                    entry.active_subagent_count = 0;
                    entry.active_child_conversations.clear();
                    apply_pending_stop(entry, &PendingStop::from_payload(p));
                }
            }
            entry.flatten_files();
        }
        "AfterFileEdit" => {
            apply_after_file_edit(entry, p, raw.ts);
        }
        _ => {
            // Unknown event: still useful as a heartbeat.
        }
    }

    if !raw_conv.is_empty() && raw_conv != session_id {
        track_child_activity(routing, entry, &raw_conv, &session_id, raw.ts);
        routing.prune_stale_children(entry, raw.ts);
    }

    let _ = try_complete_pending_stop(routing, entry, raw.ts);

    if !raw_conv.is_empty() && raw_conv != session_id {
        map.remove(&raw_conv);
    }

    Some(session_id)
}

fn humanize_subagent_type(t: &str) -> &str {
    match t {
        "generalPurpose" => "agent",
        "explore" => "explore",
        "shell" => "shell",
        "best-of-n-runner" => "runner",
        other => other,
    }
}

fn describe_subagent_start(p: &serde_json::Value) -> String {
    let sub_type = p
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("subagent");
    let task = p
        .get("task")
        .or_else(|| p.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task.is_empty() {
        format!("Subagent: {}", humanize_subagent_type(sub_type))
    } else {
        format!(
            "Subagent ({}): {}",
            humanize_subagent_type(sub_type),
            truncate(task, 48)
        )
    }
}

fn describe_subagent_stop(p: &serde_json::Value) -> (String, ActivityKind) {
    let status = p.get("status").and_then(|v| v.as_str()).unwrap_or("completed");
    let sub_type = p
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("subagent");
    let summary = p
        .get("summary")
        .or_else(|| p.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let kind = match status {
        "error" => ActivityKind::Failure,
        _ => ActivityKind::Success,
    };
    let label = if !summary.is_empty() {
        format!(
            "Subagent done ({}): {}",
            humanize_subagent_type(sub_type),
            truncate(summary, 56)
        )
    } else {
        match status {
            "error" => format!("Subagent failed ({})", humanize_subagent_type(sub_type)),
            "aborted" => format!("Subagent stopped ({})", humanize_subagent_type(sub_type)),
            _ => format!("Subagent finished ({})", humanize_subagent_type(sub_type)),
        }
    };
    (label, kind)
}

fn apply_session_end(entry: &mut Session, p: &serde_json::Value) {
    match p.get("reason").and_then(|v| v.as_str()) {
        Some("error") => {
            entry.status = Status::Errored;
            let msg = p
                .get("error_message")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Error");
            entry.done_summary = Some(msg.to_string());
            entry.current_action = msg.to_string();
        }
        Some("aborted" | "window_close" | "user_close") => {
            entry.status = Status::Done;
            entry.done_summary = Some("Stopped".to_string());
            entry.current_action = "Stopped".to_string();
        }
        _ => {
            entry.status = Status::Done;
            if let Some(status) = p.get("final_status").and_then(|v| v.as_str()) {
                if !status.is_empty() {
                    entry.done_summary = Some(truncate(status, 100));
                    entry.current_action = entry.done_summary.clone().unwrap_or_default();
                } else {
                    entry.current_action = "Done".to_string();
                }
            } else {
                entry.current_action = "Done".to_string();
            }
        }
    }
}

fn describe_pre_tool(p: &serde_json::Value) -> String {
    let tool = p
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Tool");

    let input = p.get("tool_input");

    match tool {
        "Bash" | "Shell" => {
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
        "Read" => {
            if let Some(path) = path_from_tool_input(input) {
                format!("Reading: {}", truncate(&basename(&path), 56))
            } else {
                "Reading file".to_string()
            }
        }
        "Write" => {
            if let Some(path) = path_from_tool_input(input) {
                format!("Editing: {}", truncate(&basename(&path), 56))
            } else {
                "Editing file".to_string()
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
    if tool != "apply_patch" && tool != "Write" {
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
    let segments = substantive_command_segments(raw);
    if segments.is_empty() {
        return classify_bash(raw.trim());
    }
    segments
        .iter()
        .map(|seg| classify_bash(seg))
        .max_by_key(|label| label_priority(label))
        .unwrap_or_else(|| classify_bash(raw.trim()))
}

fn label_priority(label: &str) -> u8 {
    if label.starts_with("Running:") || label == "Running command" {
        return 10;
    }
    match label {
        "Pushing to remote" => 95,
        "Committing changes" => 90,
        "Fetching from remote" | "Cloning repository" => 85,
        "Creating PR" | "Updating PR" => 84,
        "Viewing PR" | "Viewing issue" => 50,
        "Searching git history" | "Blaming lines in file" => 55,
        "Checking git state" | "Listing branches" | "Deleting branch" => 40,
        "Searching code" | "Reading file" => 45,
        "Running inline script" | "Running Node" => 42,
        _ if label.starts_with("Found ") || label.starts_with("Match:") => 60,
        _ if label.contains("tests") => 70,
        _ => 50,
    }
}

fn substantive_command_segments(raw: &str) -> Vec<&str> {
    let s = raw.trim();
    let mut out = Vec::new();
    for segment in s.split(';') {
        let seg = segment.trim();
        if seg.is_empty() || is_shell_assignment(seg) || is_shell_navigation(seg) {
            continue;
        }
        out.push(seg);
    }
    out
}

/// Cursor Shell often prefixes `Set-Location <path>;` before the real command.
/// Pick the last substantive `;`-segment for display labels.
fn label_command_segment(raw: &str) -> &str {
    let s = raw.trim();
    let mut last_substantive: Option<&str> = None;
    let mut first_non_assign: Option<&str> = None;

    for segment in s.split(';') {
        let seg = segment.trim();
        if seg.is_empty() || is_shell_assignment(seg) {
            continue;
        }
        if first_non_assign.is_none() {
            first_non_assign = Some(seg);
        }
        if !is_shell_navigation(seg) {
            last_substantive = Some(seg);
        }
    }

    last_substantive
        .or(first_non_assign)
        .unwrap_or(s)
}

fn is_shell_assignment(seg: &str) -> bool {
    (seg.starts_with('$') && {
        let up_to_eq = seg.find('=').unwrap_or(0);
        let up_to_sp = seg.find(char::is_whitespace).unwrap_or(usize::MAX);
        up_to_eq > 0 && up_to_eq <= up_to_sp
    }) || (seg.starts_with("set ") && seg.contains('='))
}

fn is_shell_navigation(seg: &str) -> bool {
    let verb = match seg.split_whitespace().next() {
        Some(v) => v.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        verb.as_str(),
        "set-location" | "cd" | "chdir" | "push-location" | "pop-location" | "pushd" | "popd"
    )
}

/// Cursor `tool_output` is often JSON: `{"output":"...","exitCode":0}`.
fn unwrap_shell_response(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') {
        return raw.to_string();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return raw.to_string();
    };
    v.get("output")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| raw.to_string())
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
        if seg.is_empty() || is_shell_assignment(seg) {
            continue;
        }
        last_non_assign = seg;
        break;
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
    let cmd = last_cmd.to_ascii_lowercase();
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
    use crate::state::SessionRouting;

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
    fn describe_bash_cursor_set_location_then_git() {
        let cmd = r#"Set-Location "c:\proj\overlay-app"; git status -sb; git log -3 --oneline"#;
        assert_eq!(describe_bash(cmd), "Checking git state");
    }

    #[test]
    fn describe_bash_cursor_set_location_then_git_push() {
        let cmd = r#"Set-Location "c:\proj"; git add .; git commit -m "msg"; git push origin main; git status -sb"#;
        assert_eq!(describe_bash(cmd), "Pushing to remote");
    }

    #[test]
    fn unwrap_shell_response_extracts_cursor_json() {
        let raw = "{\"output\":\"## main...origin/main\\n\",\"exitCode\":0}";
        assert_eq!(unwrap_shell_response(raw), "## main...origin/main\n");
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
        let mut s = Session::new("t".into(), "/tmp".into(), App::Codex, None, 0);
        let json = r#"{"number":8737,"title":"Bug report","state":"closed"}"#;
        parse_gh_issue_post(&mut s, json, 1);
        assert_eq!(s.recent_activity.len(), 1);
        assert!(s.recent_activity[0].summary.contains("Issue #8737"));
        assert!(s.recent_activity[0].summary.contains("closed"));
    }

    #[test]
    fn parse_rg_post_counts_matches() {
        let mut s = Session::new("t".into(), "/tmp".into(), App::Codex, None, 0);
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

    #[test]
    fn normalize_hook_event_maps_cursor_names() {
        assert_eq!(normalize_hook_event("sessionStart"), "SessionStart");
        assert_eq!(normalize_hook_event("sessionEnd"), "SessionEnd");
        assert_eq!(normalize_hook_event("stop"), "Stop");
        assert_eq!(normalize_hook_event("afterFileEdit"), "AfterFileEdit");
        assert_eq!(normalize_hook_event("subagentStart"), "SubagentStart");
        assert_eq!(normalize_hook_event("subagentStop"), "SubagentStop");
        assert_eq!(normalize_hook_event("UnknownThing"), "UnknownThing");
    }

    #[test]
    fn resolve_session_id_prefers_conversation_id() {
        let p = serde_json::json!({
            "conversation_id": "conv-1",
            "session_id": "sess-2"
        });
        assert_eq!(resolve_session_id(&p), "conv-1");
    }

    #[test]
    fn resolve_cwd_from_workspace_roots() {
        let p = serde_json::json!({
            "workspace_roots": ["/home/user/proj"]
        });
        assert_eq!(resolve_cwd(&p), "/home/user/proj");
    }

    #[test]
    fn detect_app_cursor_vs_codex() {
        let cursor = serde_json::json!({ "conversation_id": "c1", "session_id": "s1" });
        let codex = serde_json::json!({ "session_id": "s1" });
        assert_eq!(detect_app(&cursor), App::Cursor);
        assert_eq!(detect_app(&codex), App::Codex);
    }

    #[test]
    fn apply_cursor_bom_string_payload_and_unknown_event() {
        let inner = r#"{"conversation_id":"c-bom","hook_event_name":"beforeSubmitPrompt","prompt":"hi","workspace_roots":["/C:/proj"]}"#;
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        let raw = RawEvent {
            event: "unknown".to_string(),
            payload: serde_json::Value::String(format!("\u{FEFF}{inner}")),
            ts: 1,
            hook_pid: None,
            parent_pid: None,
        };
        apply(&mut map, &mut routing, raw);
        let s = map.get("c-bom").expect("session created");
        assert_eq!(s.app, App::Cursor);
        assert_eq!(s.last_prompt, "hi");
    }

    #[test]
    fn after_file_edit_accumulates_line_counts() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        let raw = RawEvent {
            event: "afterFileEdit".to_string(),
            payload: serde_json::json!({
                "conversation_id": "c1",
                "file_path": "src/lib.rs",
                "edits": [
                    { "old_string": "line1\nline2\n", "new_string": "line1\nline2\nline3\n" }
                ]
            }),
            ts: 100,
            hook_pid: None,
            parent_pid: None,
        };
        apply(&mut map, &mut routing, raw);
        let s = map.get("c1").unwrap();
        assert_eq!(s.files_edited.len(), 1);
        assert_eq!(s.files_edited[0].1.adds, 3);
        assert_eq!(s.files_edited[0].1.dels, 2);
        assert!(s.recent_activity[0].summary.contains("lib.rs"));
    }

    #[test]
    fn stop_status_error_maps_to_errored() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        let raw = RawEvent {
            event: "stop".to_string(),
            payload: serde_json::json!({
                "conversation_id": "c1",
                "status": "error",
                "error_message": "Rate limited"
            }),
            ts: 1,
            hook_pid: None,
            parent_pid: None,
        };
        apply(&mut map, &mut routing, raw);
        let s = map.get("c1").unwrap();
        assert_eq!(s.status, Status::Errored);
        assert_eq!(s.current_action, "Rate limited");
    }

    #[test]
    fn stop_status_aborted_sets_stopped_summary() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        let raw = RawEvent {
            event: "stop".to_string(),
            payload: serde_json::json!({
                "conversation_id": "c1",
                "status": "aborted"
            }),
            ts: 1,
            hook_pid: None,
            parent_pid: None,
        };
        apply(&mut map, &mut routing, raw);
        let s = map.get("c1").unwrap();
        assert_eq!(s.status, Status::Done);
        assert_eq!(s.done_summary.as_deref(), Some("Stopped"));
    }

    #[test]
    fn subagent_start_stop_keeps_parent_working() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "subagentStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "parent_conversation_id": "parent-1",
                    "subagent_type": "explore",
                    "task": "Find auth middleware"
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("parent-1").unwrap();
        assert_eq!(s.status, Status::Working);
        assert_eq!(s.active_subagent_count, 1);
        assert!(s.current_action.contains("explore"));

        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "subagentStop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "subagent_type": "explore",
                    "status": "completed",
                    "summary": "Found middleware in src/auth"
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("parent-1").unwrap();
        assert_eq!(s.status, Status::Working);
        assert_eq!(s.active_subagent_count, 0);
        assert!(s.current_action.contains("Subagent done"));
        assert_eq!(s.recent_activity.len(), 1);
        assert!(s.recent_activity[0].summary.contains("Subagent done"));
    }

    #[test]
    fn subagent_stop_parallel_count_label() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        for ts in 1..=2 {
            apply(
                &mut map,
                &mut routing,
                RawEvent {
                    event: "subagentStart".to_string(),
                    payload: serde_json::json!({
                        "conversation_id": "p",
                        "subagent_type": "shell",
                        "task": format!("task {ts}")
                    }),
                    ts,
                    hook_pid: None,
                    parent_pid: None,
                },
            );
        }
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "subagentStop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "p",
                    "subagent_type": "shell",
                    "status": "completed"
                }),
                ts: 3,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("p").unwrap();
        assert_eq!(s.active_subagent_count, 1);
        assert_eq!(s.current_action, "1 subagents running");
    }

    #[test]
    fn session_end_marks_done_without_stop() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionEnd".to_string(),
                payload: serde_json::json!({
                    "session_id": "sess-9",
                    "conversation_id": "sess-9",
                    "reason": "completed",
                    "final_status": "All tasks finished"
                }),
                ts: 10,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("sess-9").unwrap();
        assert_eq!(s.status, Status::Done);
        assert_eq!(s.done_summary.as_deref(), Some("All tasks finished"));
    }

    #[test]
    fn resolve_session_id_prefers_parent_conversation_id() {
        let p = serde_json::json!({
            "conversation_id": "child",
            "parent_conversation_id": "parent"
        });
        assert_eq!(resolve_session_id(&p), "parent");
    }

    #[test]
    fn task_spawn_links_orphan_child_conversation() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "tool_name": "Task",
                    "tool_input": { "run_in_background": true, "description": "explore auth" }
                }),
                ts: 1000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "child-orphan",
                    "tool_name": "Grep",
                    "tool_input": { "pattern": "auth" }
                }),
                ts: 2000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert!(map.get("child-orphan").is_none());
        assert_eq!(routing.child_to_parent.get("child-orphan").map(String::as_str), Some("parent-1"));
        let parent = map.get("parent-1").expect("parent session");
        assert!(parent.current_action.contains("Grep") || parent.current_action.contains("auth"));
    }

    #[test]
    fn stop_deferred_when_active_subagent_count() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "subagentStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "parent_conversation_id": "parent-1",
                    "subagent_type": "explore",
                    "task": "scan routes"
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "status": "completed"
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("parent-1").unwrap();
        assert_eq!(s.status, Status::Working);
        assert_eq!(s.active_subagent_count, 1);
        assert_eq!(s.current_action, "1 subagents running");
    }

    #[test]
    fn stop_deferred_when_active_child_conversations() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.link_child("child-1", "parent-1");
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        map.get_mut("parent-1")
            .unwrap()
            .active_child_conversations
            .insert("child-1".to_string());
        routing.touch_child("child-1", 2);
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "status": "completed"
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("parent-1").unwrap();
        assert_eq!(s.status, Status::Working);
        assert_eq!(s.current_action, "Subagents finishing…");
    }

    #[test]
    fn parent_stop_completes_done_after_child_stop() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.link_child("child-1", "parent-1");
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        map.get_mut("parent-1")
            .unwrap()
            .active_child_conversations
            .insert("child-1".to_string());
        routing.touch_child("child-1", 2);
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "status": "completed"
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert_eq!(map.get("parent-1").unwrap().status, Status::Working);
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "child-1",
                    "status": "completed"
                }),
                ts: 3,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("parent-1").unwrap();
        assert_eq!(s.status, Status::Done);
        assert_eq!(s.current_action, "Done");
    }

    #[test]
    fn nested_child_events_roll_up_to_root_parent() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.link_child("worker-1", "parent-1");
        routing.note_task_spawn("parent-1", 1000);
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "explore-1",
                    "tool_name": "Grep",
                    "tool_input": { "pattern": "rollup" }
                }),
                ts: 2000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert!(map.get("explore-1").is_none());
        assert_eq!(
            routing.child_to_parent.get("explore-1").map(String::as_str),
            Some("parent-1")
        );
        let parent = map.get("parent-1").unwrap();
        assert!(parent.current_action.contains("Grep") || parent.current_action.contains("rollup"));
    }

    #[test]
    fn post_tool_use_keeps_pre_tool_action_on_pill() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "c1",
                    "tool_name": "Read",
                    "tool_input": { "path": "src/session.rs" }
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "postToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "c1",
                    "tool_name": "Read"
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        let s = map.get("c1").unwrap();
        assert!(s.current_action.contains("session.rs") || s.current_action.contains("Read"));
    }

    #[test]
    fn child_events_roll_up_to_parent() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.link_child("child-1", "parent-1");
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "child-1",
                    "tool_name": "Read",
                    "tool_input": { "path": "src/session.rs" }
                }),
                ts: 2,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert!(map.get("child-1").is_none());
        let parent = map.get("parent-1").unwrap();
        assert_eq!(parent.status, Status::Working);
        assert!(parent.current_action.contains("session.rs") || parent.current_action.contains("Read"));
    }

    #[test]
    fn pending_stop_applied_after_child_quiescence() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.link_child("child-1", "parent-1");
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1,
                hook_pid: None,
                parent_pid: None,
            },
        );
        routing.touch_child("child-1", 1_000);
        map.get_mut("parent-1")
            .unwrap()
            .active_child_conversations
            .insert("child-1".to_string());
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "status": "completed"
                }),
                ts: 1_000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert_eq!(map.get("parent-1").unwrap().status, Status::Working);
        assert!(reconcile_pending_stops(&mut map, &mut routing, 1_000 + 20_001));
        let parent = map.get("parent-1").unwrap();
        assert_eq!(parent.status, Status::Done);
        assert_eq!(parent.current_action, "Done");
        assert!(parent.active_child_conversations.is_empty());
    }

    #[test]
    fn explore_child_only_tool_events_allows_parent_done() {
        let mut map = HashMap::new();
        let mut routing = SessionRouting::default();
        routing.note_task_spawn("parent-1", 1_000);
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "sessionStart".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "workspace_roots": ["/proj"]
                }),
                ts: 1_000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "preToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "explore-1",
                    "tool_name": "Grep",
                    "tool_input": { "pattern": "auth" }
                }),
                ts: 6_000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "postToolUse".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "explore-1",
                    "tool_name": "Grep"
                }),
                ts: 7_000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        apply(
            &mut map,
            &mut routing,
            RawEvent {
                event: "stop".to_string(),
                payload: serde_json::json!({
                    "conversation_id": "parent-1",
                    "status": "completed"
                }),
                ts: 8_000,
                hook_pid: None,
                parent_pid: None,
            },
        );
        assert_eq!(map.get("parent-1").unwrap().status, Status::Working);
        let parent = map.get("parent-1").unwrap();
        assert!(parent.active_child_conversations.contains("explore-1"));
        assert!(reconcile_pending_stops(&mut map, &mut routing, 28_000));
        let parent = map.get("parent-1").unwrap();
        assert_eq!(parent.status, Status::Done);
        assert_eq!(parent.current_action, "Done");
        assert!(!parent.active_child_conversations.contains("explore-1"));
    }
}
