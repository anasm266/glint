use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use crate::hook_install;
use crate::session::{Session, Status};
use crate::state::{Corner, Settings, SharedState};
use crate::win;

fn err<T: std::fmt::Display>(e: T) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_sessions(state: State<'_, SharedState>) -> Vec<Session> {
    state.snapshot()
}

#[tauri::command]
pub fn remove_session(id: String, state: State<'_, SharedState>, app: AppHandle) {
    state.remove_session(&id);
    state.emit_snapshot(&app);
}

#[tauri::command]
pub fn focus_session(
    id: String,
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<bool, String> {
    let parent_pid = state.with_sessions(|m| {
        m.get_mut(&id).and_then(|s| {
            // Only "done pending review" is acknowledged on focus. Working
            // sessions must keep acknowledged_done false so the UI does not
            // treat a focus click as dismissing a completed run.
            if s.status == Status::Done {
                s.acknowledged_done = true;
            }
            s.parent_pid
        })
    });
    state.emit_snapshot(&app);
    let Some(pid) = parent_pid else {
        return Ok(false);
    };
    let target = win::root_codex_pid(pid).unwrap_or(pid);
    Ok(win::focus_pid(target))
}

#[tauri::command]
pub fn install_codex_hooks(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<(), String> {
    let exe = hook_exe_path(&app).map_err(err)?;
    tracing::info!("install_codex_hooks: using hook binary at {}", exe.display());
    hook_install::install(&exe).map_err(|e| {
        tracing::error!("install_codex_hooks failed: {e:?}");
        err(e)
    })?;
    state.set_settings(|s| s.codex_connected = true);
    tracing::info!("install_codex_hooks: ok");
    Ok(())
}

#[tauri::command]
pub fn remove_codex_hooks(state: State<'_, SharedState>) -> Result<(), String> {
    hook_install::remove().map_err(|e| {
        tracing::error!("remove_codex_hooks failed: {e:?}");
        err(e)
    })?;
    state.set_settings(|s| s.codex_connected = false);
    tracing::info!("remove_codex_hooks: ok");
    Ok(())
}

#[tauri::command]
pub fn is_codex_connected() -> bool {
    hook_install::is_installed()
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedState>) -> Settings {
    let mut s = state.settings();
    s.codex_connected = hook_install::is_installed();
    state.set_settings(|x| x.codex_connected = s.codex_connected);
    s
}

#[tauri::command]
pub fn set_position(
    corner: Corner,
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<(), String> {
    state.set_settings(|s| s.corner = corner);
    if let Some(window) = app.get_webview_window("main") {
        crate::reposition_main(&window, corner);
    }
    Ok(())
}

#[tauri::command]
pub fn set_opacity(
    opacity: f32,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let clamped = opacity.clamp(0.4, 1.0);
    state.set_settings(|s| s.opacity = clamped);
    // Re-applying acrylic with a different alpha would require recreating the
    // window; for now we just persist the setting. The visual surface alpha
    // comes from `.surface` CSS, which can read the live setting later.
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    crate::open_or_focus_settings(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Resolve the absolute path to `overlay-hook.exe`. We try several places
/// in order of preference, so the same code works for `cargo tauri dev`,
/// `cargo tauri build`, and an installed copy.
fn hook_exe_path(_app: &AppHandle) -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("no parent dir"))?
        .to_path_buf();

    // 1. Sibling of overlay-app.exe (release bundle, installed copy).
    let here = dir.join("overlay-hook.exe");
    if here.exists() {
        return Ok(here);
    }

    // 2. Workspace sibling directories. In a workspace build, the hook lives
    //    in `target/{profile}/overlay-hook.exe` alongside the overlay.
    //    `tauri dev` puts overlay-app.exe in `target/debug/`, but the hook
    //    might have been built with `--release` and live in `target/release/`.
    if let Some(target) = dir.parent() {
        for profile in ["debug", "release"] {
            let candidate = target.join(profile).join("overlay-hook.exe");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(anyhow::anyhow!(
        "overlay-hook.exe not found near {}. Build it with `cargo build -p overlay-hook --release` first.",
        dir.display()
    ))
}
