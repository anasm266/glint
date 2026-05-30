//! JSONL session logs under `%LOCALAPPDATA%\Glint\logs\` (Windows) or `~/.glint/logs/`.
//!
//! **On by default** for local testing. Set `GLINT_LOG=0` to disable.
//! Writes are queued to a background thread so hook handlers stay fast.

use crate::session::{self, App, RawEvent, Session, Status};
use crate::state::SessionRouting;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

static SENDER: OnceLock<mpsc::Sender<String>> = OnceLock::new();
static RUN_ID: OnceLock<String> = OnceLock::new();

const MAX_LOG_STR: usize = 800;

/// Logging is enabled unless explicitly disabled with `GLINT_LOG=0` or `false`.
pub fn enabled() -> bool {
    match std::env::var("GLINT_LOG").ok().as_deref() {
        None | Some("") => true,
        Some(v) if v == "0" || v.eq_ignore_ascii_case("false") => false,
        Some(_) => true,
    }
}

/// Directory where JSONL files are written (created on first log).
pub fn log_dir() -> PathBuf {
    if let Some(base) = dirs::data_local_dir() {
        base.join("Glint").join("logs")
    } else if let Some(home) = dirs::home_dir() {
        home.join(".glint").join("logs")
    } else {
        PathBuf::from(".glint/logs")
    }
}

fn run_log_path(dir: &PathBuf) -> PathBuf {
    dir.join(format!("glint-{}.jsonl", run_id()))
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn run_id() -> &'static str {
    RUN_ID.get_or_init(|| {
        let ms = wall_ms();
        format!("{ms}")
    })
}

fn enqueue(line: String) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(line);
    }
}

fn base_fields() -> Value {
    json!({
        "run_id": run_id(),
        "wall_ms": wall_ms(),
    })
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

fn normalize_event_name(raw: &str, p: &Value) -> String {
    if raw.eq_ignore_ascii_case("unknown") {
        p.get("hook_event_name")
            .and_then(|v| v.as_str())
            .unwrap_or(raw)
            .to_string()
    } else {
        raw.to_string()
    }
}

/// What the AI agent actually invoked (from hook payload).
fn extract_ai_tool(p: &Value) -> Value {
    let tool = p
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let input = p.get("tool_input");
    match tool {
        "Shell" | "Bash" | "PowerShell" => {
            let cmd = input
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            json!({
                "tool": tool,
                "command": trunc(cmd, MAX_LOG_STR),
                "cwd": input.and_then(|v| v.get("cwd")).and_then(|v| v.as_str()),
            })
        }
        "Read" | "Write" | "StrReplace" | "Delete" => {
            json!({
                "tool": tool,
                "path": input
                    .and_then(|v| v.get("file_path").or_else(|| v.get("path")))
                    .and_then(|v| v.as_str())
                    .map(|s| trunc(s, MAX_LOG_STR)),
            })
        }
        "Task" => json!({
            "tool": "Task",
            "description": input.and_then(|v| v.get("description")).and_then(|v| v.as_str()),
            "subagent_type": input.and_then(|v| v.get("subagent_type")).and_then(|v| v.as_str()),
            "run_in_background": input
                .and_then(|v| v.get("run_in_background"))
                .and_then(|v| v.as_bool()),
            "prompt_preview": input
                .and_then(|v| v.get("prompt"))
                .and_then(|v| v.as_str())
                .map(|s| trunc(s, 200)),
        }),
        "Grep" | "Glob" | "SemanticSearch" => json!({
            "tool": tool,
            "pattern": input.and_then(|v| v.get("pattern")).and_then(|v| v.as_str()),
            "path": input
                .and_then(|v| v.get("file_path").or_else(|| v.get("path")))
                .and_then(|v| v.as_str())
                .map(|s| trunc(s, MAX_LOG_STR)),
            "glob": input.and_then(|v| v.get("glob")).and_then(|v| v.as_str()),
        }),
        other if !other.is_empty() => json!({ "tool": other }),
        _ => Value::Null,
    }
}

fn extract_post_tool_result(p: &Value) -> Value {
    let tool = p
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let response = session::tool_response_str_pub(p);
    if response.is_empty() {
        return Value::Null;
    }
    let mut out = json!({
        "tool": tool,
        "response_preview": trunc(&response, MAX_LOG_STR),
    });
    if tool == "Shell" || tool == "Bash" || tool == "PowerShell" {
        if let Ok(v) = serde_json::from_str::<Value>(&response) {
            if let Some(code) = v.get("exitCode").and_then(|c| c.as_i64()) {
                out["exit_code"] = json!(code);
            }
        }
    }
    out
}

/// Start the background writer and emit a `run_start` line.
pub fn init() {
    if !enabled() {
        return;
    }
    let _ = run_id();
    let dir = log_dir();
    if fs::create_dir_all(&dir).is_err() {
        tracing::warn!("Glint logs: could not create log dir {:?}", dir);
        return;
    }
    let path = run_log_path(&dir);
    let log_file = path.display().to_string();
    let (tx, rx) = mpsc::channel();
    if SENDER.set(tx).is_err() {
        return;
    }

    thread::spawn(move || {
        let mut file = match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Glint logs: could not open {:?}: {e}", path);
                return;
            }
        };
        while let Ok(line) = rx.recv() {
            if writeln!(file, "{line}").is_err() {
                break;
            }
        }
    });

    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut merged = base_fields();
    if let Value::Object(ref mut m) = merged {
        m.extend(json!({
            "type": "run_start",
            "version": env!("CARGO_PKG_VERSION"),
            "exe": exe,
            "log_dir": dir.display().to_string(),
            "log_file": log_file,
        })
        .as_object()
        .cloned()
        .unwrap_or_default());
    }
    enqueue(merged.to_string());
    tracing::info!("Glint session logs: {log_file}");
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityLine {
    summary: String,
    at_ms: u64,
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionLine {
    id: String,
    app: String,
    project: String,
    status: String,
    current_action: String,
    acknowledged_done: bool,
    recent_activity: Vec<ActivityLine>,
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Idle => "idle",
        Status::Working => "working",
        Status::Done => "done",
        Status::Errored => "errored",
    }
}

fn activity_kind_str(k: session::ActivityKind) -> &'static str {
    match k {
        session::ActivityKind::Normal => "normal",
        session::ActivityKind::Success => "success",
        session::ActivityKind::Failure => "failure",
    }
}

fn session_line(s: &Session) -> SessionLine {
    let recent_activity = s
        .recent_activity
        .iter()
        .take(5)
        .map(|a| ActivityLine {
            summary: a.summary.clone(),
            at_ms: a.at_ms,
            kind: activity_kind_str(a.kind).to_string(),
        })
        .collect();
    SessionLine {
        id: s.id.clone(),
        app: match s.app {
            App::Codex => "codex",
            App::Cursor => "cursor",
            App::Claude => "claude",
        }
        .to_string(),
        project: s.project.clone(),
        status: status_str(s.status).to_string(),
        current_action: s.current_action.clone(),
        acknowledged_done: s.acknowledged_done,
        recent_activity,
    }
}

/// Log a hook event and the session state after `apply`.
pub fn log_hook_event(
    raw: &RawEvent,
    routing: &SessionRouting,
    touched_id: &Option<String>,
    map: &HashMap<String, Session>,
) {
    if !enabled() {
        return;
    }
    let payload = session::coerce_payload_pub(&raw.payload);
    let conversation_id = session::raw_conversation_id_pub(&payload);
    let rollup_parent = routing
        .child_to_parent
        .get(&conversation_id)
        .cloned()
        .or_else(|| {
            payload
                .get("parent_conversation_id")
                .and_then(|v| v.as_str())
                .filter(|p| !p.is_empty() && *p != conversation_id)
                .map(|s| s.to_string())
        });
    let session_id = touched_id.clone().unwrap_or_else(|| {
        routing.resolve_parent(&conversation_id, &payload)
    });
    let session = map.get(&session_id);
    let (status, current_action, recent_top) = session
        .map(|s| {
            (
                status_str(s.status).to_string(),
                s.current_action.clone(),
                s.recent_activity
                    .first()
                    .map(|a| a.summary.clone())
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_default();

    let event_norm = normalize_event_name(&raw.event, &payload);
    let ai_label = if event_norm.eq_ignore_ascii_case("preToolUse") {
        Some(session::describe_pre_tool_pub(&payload))
    } else {
        None
    };
    let ai_tool = if event_norm.eq_ignore_ascii_case("preToolUse") {
        extract_ai_tool(&payload)
    } else if event_norm.eq_ignore_ascii_case("postToolUse") {
        extract_post_tool_result(&payload)
    } else {
        Value::Null
    };

    let pill_matches_label = ai_label
        .as_ref()
        .map(|label| label == &current_action)
        .unwrap_or(true);

    let line = json!({
        "type": "hook_event",
        "hook_ts": raw.ts,
        "event": raw.event,
        "event_norm": event_norm,
        "conversation_id": conversation_id,
        "session_id": session_id,
        "rollup_parent": rollup_parent,
        "ui_shown": {
            "pill": current_action,
            "status": status,
            "hover_top_activity": recent_top,
        },
        "ai": {
            "label_from_hook": ai_label,
            "tool": ai_tool,
        },
        "pill_matches_ai_label": pill_matches_label,
        "payload": payload,
    });
    let mut merged = base_fields();
    if let Value::Object(ref mut m) = merged {
        if let Value::Object(extra) = line {
            m.extend(extra);
        }
    }
    enqueue(merged.to_string());
}

/// Log the session snapshot pushed to the UI.
pub fn log_snapshot(sessions: &[Session]) {
    if !enabled() {
        return;
    }
    let lines: Vec<SessionLine> = sessions.iter().map(session_line).collect();
    let line = json!({
        "type": "snapshot",
        "session_count": lines.len(),
        "sessions": lines,
    });
    let mut merged = base_fields();
    if let Value::Object(ref mut m) = merged {
        if let Value::Object(extra) = line {
            m.extend(extra);
        }
    }
    enqueue(merged.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_ends_with_glint_logs() {
        let d = log_dir();
        let s = d.to_string_lossy();
        assert!(s.contains("Glint") || s.contains(".glint"));
        assert!(s.ends_with("logs") || s.ends_with("logs\\") || s.ends_with("logs/"));
    }

    #[test]
    fn enabled_defaults_on_opt_out_with_zero() {
        let prev = std::env::var("GLINT_LOG").ok();
        std::env::remove_var("GLINT_LOG");
        assert!(enabled());
        std::env::set_var("GLINT_LOG", "0");
        assert!(!enabled());
        std::env::set_var("GLINT_LOG", "false");
        assert!(!enabled());
        match prev {
            Some(v) => std::env::set_var("GLINT_LOG", v),
            None => std::env::remove_var("GLINT_LOG"),
        }
    }

    #[test]
    fn extract_ai_tool_shell() {
        let p = json!({
            "tool_name": "Shell",
            "tool_input": { "command": "git status", "cwd": "/proj" }
        });
        let v = extract_ai_tool(&p);
        assert_eq!(v["tool"], "Shell");
        assert_eq!(v["command"], "git status");
    }

    #[test]
    fn extract_ai_tool_powershell() {
        let p = json!({
            "tool_name": "PowerShell",
            "tool_input": { "command": "Get-ChildItem -Recurse", "cwd": "C:\\proj" }
        });
        let v = extract_ai_tool(&p);
        assert_eq!(v["tool"], "PowerShell");
        assert_eq!(v["command"], "Get-ChildItem -Recurse");
        assert_eq!(v["cwd"], "C:\\proj");
    }
}
