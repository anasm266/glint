//! overlay-app library crate.
//!
//! The bin (`main.rs`) is a thin shim that calls [`run`]. All Tauri
//! configuration lives here.

mod commands;
mod hook_install;
mod http_server;
mod session;
mod state;
mod tray;
mod win;

use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub use state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,overlay_app_lib=debug")),
        )
        .with_target(false)
        .with_level(true)
        .init();

    let state = Arc::new(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_sessions,
            commands::remove_session,
            commands::focus_session,
            commands::install_codex_hooks,
            commands::remove_codex_hooks,
            commands::is_codex_connected,
            commands::get_settings,
            commands::set_position,
            commands::set_opacity,
            commands::open_settings,
            commands::quit_app,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            if let Some(window) = app.get_webview_window("main") {
                // Do NOT apply native acrylic to the main window. Acrylic fills
                // the entire window rectangle including the transparent corners
                // outside the pill, producing a visible grey rectangle around it.
                // Instead, `transparent: true` makes those corners truly
                // invisible; the pill gets its translucency from CSS alone.
                reposition_main(&window, state.settings().corner);
                let _ = window.set_always_on_top(true);
                let _ = window.set_skip_taskbar(true);
            }

            tray::install(&handle)?;

            // Local HTTP listener for hook events.
            http_server::spawn(handle.clone(), state.clone());

            // Push initial empty snapshot so the frontend has something to bind to.
            state.emit_snapshot(&handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub(crate) fn reposition_main(window: &tauri::WebviewWindow, corner: state::Corner) {
    let Ok(monitor) = window.current_monitor() else { return };
    let Some(monitor) = monitor else { return };
    let size = monitor.size();
    let scale = monitor.scale_factor();
    let inset = (24.0 * scale) as i32;
    let win_w = (380.0 * scale) as i32;
    let win_h = (52.0 * scale) as i32;
    let pos = monitor.position();
    let x = match corner {
        state::Corner::Tl | state::Corner::Bl => pos.x + inset,
        state::Corner::Tr | state::Corner::Br => pos.x + size.width as i32 - win_w - inset,
    };
    let y = match corner {
        state::Corner::Tl | state::Corner::Tr => pos.y + inset,
        state::Corner::Bl | state::Corner::Br => pos.y + size.height as i32 - win_h - inset,
    };
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

pub(crate) fn open_or_focus_settings(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let _ = WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App("settings.html".into()),
    )
    .title("overlay-app · settings")
    .inner_size(440.0, 480.0)
    .resizable(false)
    .decorations(true)
    .transparent(false)
    .center()
    .build();
}
