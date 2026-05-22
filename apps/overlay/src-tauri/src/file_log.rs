//! Optional JSONL session logs under `%LOCALAPPDATA%\Glint\logs\` (Windows) or `~/.glint/logs/`.
//!
//! Enable with `GLINT_LOG=1` (any non-empty value). Writes are queued to a background
//! thread so hook handlers stay fast.

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

pub fn enabled() -> bool {
    std::env::var("GLINT_LOG")
        .ok()
        .is_some_and(|v| !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false"))
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

/// Start the background writer and emit a `run_start` line. No-op when logging is disabled.
pub fn init() {
    if !enabled() {
        return;
    }
    let _ = run_id();
    let dir = log_dir();
    if fs::create_dir_all(&dir).is_err() {
        tracing::warn!("GLINT_LOG: could not create log dir {:?}", dir);
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
                tracing::warn!("GLINT_LOG: could not open {:?}: {e}", path);
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
    tracing::info!("GLINT_LOG=1: session logs at {log_file}");
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
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Idle => "idle",
        Status::Working => "working",
        Status::Done => "done",
        Status::Errored => "errored",
    }
}

fn session_line(s: &Session) -> SessionLine {
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
    let (status, current_action) = map
        .get(&session_id)
        .map(|s| (status_str(s.status).to_string(), s.current_action.clone()))
        .unwrap_or_default();

    let line = json!({
        "type": "hook_event",
        "hook_ts": raw.ts,
        "event": raw.event,
        "conversation_id": conversation_id,
        "session_id": session_id,
        "rollup_parent": rollup_parent,
        "status": status,
        "current_action": current_action,
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
    fn enabled_respects_env() {
        let prev = std::env::var("GLINT_LOG").ok();
        std::env::set_var("GLINT_LOG", "1");
        assert!(enabled());
        std::env::set_var("GLINT_LOG", "0");
        assert!(!enabled());
        match prev {
            Some(v) => std::env::set_var("GLINT_LOG", v),
            None => std::env::remove_var("GLINT_LOG"),
        }
    }
}
