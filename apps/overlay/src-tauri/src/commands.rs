use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

use crate::cursor_hook_install;
use crate::hook_install;
use crate::session::{App, Session, Status};
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
pub fn open_codex(
    id: Option<String>,
    state: State<'_, SharedState>,
) -> Result<bool, String> {
    let parent_pid = state.with_sessions(|m| {
        if let Some(ref sid) = id {
            m.get(sid).and_then(|s| s.parent_pid)
        } else {
            let mut best: Option<(u64, u32)> = None;
            for s in m.values() {
                if let Some(pid) = s.parent_pid {
                    let t = s.last_event_at_ms;
                    if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                        best = Some((t, pid));
                    }
                }
            }
            best.map(|(_, p)| p)
        }
    });
    let Some(pid) = parent_pid else {
        return Ok(false);
    };
    let target = win::root_codex_pid(pid).unwrap_or(pid);
    Ok(win::focus_pid(target))
}

#[tauri::command]
pub fn open_cursor(
    id: Option<String>,
    state: State<'_, SharedState>,
) -> Result<bool, String> {
    let parent_pid = state.with_sessions(|m| {
        if let Some(ref sid) = id {
            m.get(sid).and_then(|s| s.parent_pid)
        } else {
            let mut best: Option<(u64, u32)> = None;
            for s in m.values() {
                if s.app != App::Cursor {
                    continue;
                }
                if let Some(pid) = s.parent_pid {
                    let t = s.last_event_at_ms;
                    if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
                        best = Some((t, pid));
                    }
                }
            }
            best.map(|(_, p)| p)
        }
    });
    let Some(pid) = parent_pid else {
        return Ok(false);
    };
    let target = win::root_cursor_pid(pid).unwrap_or(pid);
    Ok(win::focus_cursor(target))
}

#[tauri::command]
pub fn acknowledge_done(
    id: String,
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<(), String> {
    state.with_sessions(|m| {
        if let Some(s) = m.get_mut(&id) {
            if s.status == Status::Done {
                s.acknowledged_done = true;
            }
        }
    });
    state.emit_snapshot(&app);
    Ok(())
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
pub fn install_cursor_hooks(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<(), String> {
    let exe = hook_exe_path(&app).map_err(err)?;
    tracing::info!("install_cursor_hooks: using hook binary at {}", exe.display());
    cursor_hook_install::install(&exe).map_err(|e| {
        tracing::error!("install_cursor_hooks failed: {e:?}");
        err(e)
    })?;
    state.set_settings(|s| s.cursor_connected = true);
    tracing::info!("install_cursor_hooks: ok");
    Ok(())
}

#[tauri::command]
pub fn remove_cursor_hooks(state: State<'_, SharedState>) -> Result<(), String> {
    cursor_hook_install::remove().map_err(|e| {
        tracing::error!("remove_cursor_hooks failed: {e:?}");
        err(e)
    })?;
    state.set_settings(|s| s.cursor_connected = false);
    tracing::info!("remove_cursor_hooks: ok");
    Ok(())
}

#[tauri::command]
pub fn is_cursor_connected() -> bool {
    cursor_hook_install::is_installed()
}

#[tauri::command]
pub fn get_settings(state: State<'_, SharedState>) -> Settings {
    let mut s = state.settings();
    s.codex_connected = hook_install::is_installed();
    s.cursor_connected = cursor_hook_install::is_installed();
    state.set_settings(|x| {
        x.codex_connected = s.codex_connected;
        x.cursor_connected = s.cursor_connected;
    });
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

const PANEL_WIN_W: f64 = 380.0;
const PANEL_H_COLLAPSED: f64 = 60.0;
const PANEL_H_EXPANDED: f64 = 300.0;

#[cfg(windows)]
fn set_main_window_rect(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSENDCHANGING, SWP_NOZORDER,
    };

    let hwnd = window.hwnd().map_err(err)?;
    let ok = unsafe {
        SetWindowPos(
            hwnd.0 as HWND,
            std::ptr::null_mut(),
            x,
            y,
            width as i32,
            height as i32,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING | SWP_NOZORDER,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_main_window_rect(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    window
        .set_size(tauri::PhysicalSize::new(width, height))
        .map_err(err)?;
    window
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(err)
}

fn set_panel_window_height(
    anchor_bottom: bool,
    target_height: f64,
    app: AppHandle,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    let scale = window.scale_factor().map_err(err)?;
    let width = (PANEL_WIN_W * scale) as u32;
    let height = (target_height * scale) as u32;
    let pos = window.outer_position().map_err(err)?;
    let y = if anchor_bottom {
        let size = window.outer_size().map_err(err)?;
        let bottom = pos.y + size.height as i32;
        bottom - height as i32
    } else {
        pos.y
    };

    set_main_window_rect(&window, pos.x, y, width, height)
}

/// Resize the main overlay for the hover panel. Above-panel transitions are
/// bottom-anchored so the pill stays fixed on screen.
#[tauri::command]
pub fn set_panel_window_expanded(expand_up: bool, app: AppHandle) -> Result<(), String> {
    set_panel_window_height(expand_up, PANEL_H_EXPANDED, app)
}

/// Collapse the main overlay after the hover panel closes.
#[tauri::command]
pub fn set_panel_window_collapsed(collapse_from_above: bool, app: AppHandle) -> Result<(), String> {
    set_panel_window_height(collapse_from_above, PANEL_H_COLLAPSED, app)
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
