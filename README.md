# overlay-app

A translucent always-on-top Windows overlay that shows live state of your AI coding agents. v0.1 covers Codex Desktop; Cursor and Claude Code are planned.

## Status

v0.1 — Milestone A. Codex Desktop only. Compact view only. Dogfood build.

## What it shows

A 380x52 pill in the top-right of your primary monitor:

- A **fleet bar** of dots, one per active Codex session, colored by status (idle, working, done, errored)
- A **primary line** showing app, project, and current action (`Editing src/auth.ts`, `Running: npm test`, `Done · +89 / -38 across 6 files`, …)
- A **status badge** with tabular numeral elapsed time

Click the overlay to focus the Codex window that owns the primary session. The window itself is draggable (left-click and hold anywhere). Tray icon gives `Show`, `Settings…`, `Quit`.

## Architecture

Two binaries:

- `overlay-app.exe` — the Tauri app: Rust core (HTTP listener + session store) + React WebView UI + system tray
- `overlay-hook.exe` — a tiny stdin-to-HTTP relay that Codex hooks point at

```
Codex session
     |
     | spawns per hook event, JSON on stdin
     v
overlay-hook.exe  -- POST http://127.0.0.1:47611/event (200ms fire-and-forget) -->  overlay-app.exe
```

If `overlay-app.exe` is closed or crashed, the hook POST times out in 200ms and the Codex session is never blocked.

## Prerequisites

- Windows 10 or 11
- [Rust](https://rustup.rs/) stable, with the `x86_64-pc-windows-msvc` target
- **Microsoft C++ Build Tools** — install from [the Visual Studio Build Tools page](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and select the "Desktop development with C++" workload. This provides `link.exe`, which Rust uses to link Tauri. WebView2 is shipped with Windows 11 and recent Windows 10.
- Node 20+ and npm 10+

## Build / run (dev)

```powershell
cd apps\overlay
npm install
npm run app:dev
```

`app:dev` first builds `overlay-hook.exe` in release mode, then launches `cargo tauri dev`, which starts Vite for hot-reload UI and the Rust app. The overlay window appears top-right of the primary monitor; the tray icon shows up next to the clock.

Open **Settings…** from the tray and toggle **Connect Codex**. This writes the v0.1 hook entries into `~/.codex/config.toml` (a one-time backup of the original is saved alongside as `config.toml.overlay-backup`). Restart Codex Desktop to pick up the new config. Disconnecting removes only the entries we added.

## Build (release)

```powershell
cd apps\overlay
npm run app:build
```

The unpacked `overlay-app.exe` and an `.msi` end up under `apps\overlay\src-tauri\target\release\bundle\`. The release `overlay-hook.exe` is at `target\release\overlay-hook.exe` (workspace root). For "live with it for a day" testing, run the unpacked overlay-app.exe directly; the hook installer will resolve the hook binary path correctly via `current_exe()`.

## Folder layout

```
overlay-app/
  Cargo.toml                # workspace
  apps/
    hook/                   # overlay-hook.exe
      src/main.rs
    overlay/                # Tauri app
      src/                  # React frontend
      src-tauri/            # Rust core + Tauri config
```

## Codex hook config written by "Connect Codex"

```toml
[features]
codex_hooks = true

[[hooks.SessionStart]]
overlay_managed = true

[[hooks.SessionStart.hooks]]
type = "command"
command = '"<abs path>\overlay-hook.exe" SessionStart'

# ... PreToolUse, PostToolUse, UserPromptSubmit, Stop
```

The `overlay_managed = true` marker is what `Disconnect Codex` uses to find and remove only our entries; user-authored hook entries are preserved.

## Acceptance criteria for v0.1

1. `cargo tauri dev` launches a translucent always-on-top window in the top-right of the primary monitor with the Win11 acrylic surface.
2. Tray icon shows up; `Quit` exits cleanly; `Settings…` opens settings window.
3. Toggling `Connect Codex` writes the five hook entries into `~/.codex/config.toml`, preserves user content, and disconnect removes them cleanly.
4. With Codex Desktop running and a fresh prompt, a session row appears within one second of the first hook firing.
5. As Codex works, the primary line updates (`Editing X`, `Running: Y`) and elapsed time ticks.
6. On `Stop`, the row flashes green and shows `Done` with diff scope if available.
7. Clicking the row focuses the Codex Desktop window for that session.
8. Killing `overlay-app.exe` mid-session does not interfere with Codex (hook calls time out in 200ms, agent continues).
9. Single-instance lock prevents a second copy from launching.

## Out of scope for v0.1

Expanded panel (hover / click reveal), Cursor / Claude Code integration, derived states (stall / loop / dangerous command), cost meter, sound alerts, installer, auto-update, persistence.

## Hooks reload

Codex only reads `~/.codex/config.toml` when the app starts. After connecting or disconnecting in Settings, quit Codex Desktop completely and open it again.
