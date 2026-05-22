// overlay-hook.exe
//
// Fired by Codex (and later Cursor / Claude Code) for every hook event.
// Reads JSON from stdin, POSTs an envelope to the overlay app, and exits.
//
// Hard constraints:
//   - Must NEVER block the agent. 200ms hard timeout on the HTTP request,
//     any error is swallowed silently.
//   - Codex only required JSON on stdout for `Stop`; Cursor parses stdout for
//     every hook. We always emit `{}` (no decision, continue normally).

use std::io::{Read, Write};
use std::time::Duration;

const OVERLAY_URL: &str = "http://127.0.0.1:47611/event";
const HTTP_TIMEOUT: Duration = Duration::from_millis(200);

fn main() {
    // Read stdin first — we always need it, and it contains hook_event_name.
    let mut raw_stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw_stdin);
    let stdin = strip_hook_stdin(&raw_stdin);

    // Event name: prefer argv[1] (legacy), fall back to hook_event_name in the
    // JSON payload. Using the JSON field means the command in config.toml can be
    // the bare exe path without any arguments, sidestepping Windows quoting bugs.
    let event = std::env::args()
        .nth(1)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            parse_hook_json(stdin)
                .ok()
                .and_then(|v| v.get("hook_event_name").and_then(|n| n.as_str()).map(str::to_string))
        })
        .unwrap_or_else(|| "unknown".to_string());

    let envelope = serde_json::json!({
        "event": event,
        "payload": parse_or_string(stdin),
        "ts": now_ms(),
        "hook_pid": std::process::id(),
        "parent_pid": parent_pid().unwrap_or(0),
    });

    let _ = post_envelope(&envelope);

    let _ = std::io::stdout().write_all(b"{}");
}

fn post_envelope(body: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .timeout_connect(HTTP_TIMEOUT)
        .timeout_read(HTTP_TIMEOUT)
        .timeout_write(HTTP_TIMEOUT)
        .build();
    agent.post(OVERLAY_URL).send_json(body.clone())?;
    Ok(())
}

/// Cursor on Windows may prefix hook stdin with a UTF-8 BOM; serde_json rejects it.
fn strip_hook_stdin(s: &str) -> &str {
    s.trim().trim_start_matches('\u{FEFF}')
}

fn parse_hook_json(s: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(strip_hook_stdin(s))
}

fn parse_or_string(s: &str) -> serde_json::Value {
    let s = strip_hook_stdin(s);
    if s.is_empty() {
        return serde_json::Value::Null;
    }
    match parse_hook_json(s) {
        Ok(v) => v,
        Err(_) => serde_json::Value::String(s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utf8_bom_before_parse() {
        let raw = "\u{FEFF}{\"hook_event_name\":\"preToolUse\",\"conversation_id\":\"x\"}";
        let v = parse_hook_json(raw).expect("parse");
        assert_eq!(
            v.get("hook_event_name").and_then(|x| x.as_str()),
            Some("preToolUse")
        );
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(windows)]
fn parent_pid() -> Option<u32> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    unsafe {
        let me = GetCurrentProcessId();
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == me {
                    found = Some(entry.th32ParentProcessID);
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(not(windows))]
fn parent_pid() -> Option<u32> {
    None
}
