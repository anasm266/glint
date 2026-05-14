# overlay-app

A translucent always-on-top Windows overlay that shows live state of your AI coding agents. v0.1 covers **Codex Desktop**; Cursor and Claude Code are planned.

## Status

v0.1 — Milestone A. Codex Desktop only. **Compact pill** plus an **expanded hover detail** when you point at a session dot. Dogfood build.

Near-term polish is tracked in [ROADMAP.md](ROADMAP.md). Hover-specific behavior is also summarized in [HOVER_PANEL.md](HOVER_PANEL.md).

## What it does today

### Compact pill (380×52, corner from Settings)

- **Fleet bar** — One dot per Codex session (up to eight visible, then `+N` overflow). Dots reflect status: idle, working, done, errored (working dots use a subtle pulse).
- **Primary line** — App label (`Codex`), project folder name, and the current action (`Editing …`, `Running: …`, or a **Done** summary with diff totals when the run has finished).
- **Done queue** — If more than one session is **Done** and not yet acknowledged, the primary line is prefixed with `✓ N done ·` before the usual Done summary for the frontmost completion.
- **Status badge** — Elapsed time while working; **just now** / **ago** after Done.
- **Hooks disconnected hint** — If Codex hooks are not installed, the pill shows **Connect Codex to get started** (click opens Settings). When sessions exist but hooks are still off, a small **hollow amber dot** appears after the fleet; click it to open Settings.

Click the **pill body** (not on a dot) to focus the Codex window for whichever session the **primary line** is showing, and to clear a temporary dot selection. The window is draggable (`data-tauri-drag-region`). **Tray:** Show, Settings…, Quit. **Single instance** — launching again focuses the existing overlay.

### Fleet bar interactions

- **Single-click a dot** — Temporarily treats that session as primary, focuses its Codex window, and runs the same acknowledge flow as the pill click **only when that session is Done** (so in-progress runs are not removed after focus).
- **Double-click a dot** — Pins that session as primary until you double-click again to unpin or the session disappears. Pinned and temp-selected dots show distinct rings.
- **Hover a dot** — Expands the native window vertically and opens a **detail card** below or above the pill (depending on corner). Moving the pointer into the card keeps it open; leaving the dot or card collapses after a short delay.

### Hover detail card

- **Last prompt** — When Codex sends `UserPromptSubmit`, the user message is stored and shown (up to two lines, full text in a tooltip).
- **While working** — Current action line plus **N files touched so far** once patch activity has produced file entries.
- **When Done** — Scrollable list of **files** with per-file `+adds / −dels`, plus a compact totals footer. If nothing was patched, it says **Done — no files changed**.
- **Time row** — Click to toggle between **elapsed** (e.g. `Running 4m 20s`, `Finished 2m ago`) and **clock** (e.g. `Started 9:42 AM`, `Finished at 9:46 AM`, with yesterday / date when needed).

After you focus a **Done** session (dot or pill), it is marked acknowledged; **about seven seconds later** the session is removed from the overlay so the fleet stays tidy.

### Settings

- **Connect Codex** — Installs or removes managed hook entries in `~/.codex/config.toml` (with backup). Restart Codex after toggling.
- **Position** — Corners of the primary monitor (top-left / top-right / bottom-left / bottom-right).
- **Opacity** — Slider updates in-app state (full visual wiring is still on the roadmap).

When the overlay regains focus, it refreshes **corner** and **hook installed** state from disk so changes from Settings stay in sync.

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

`app:dev` first builds `overlay-hook.exe` in release mode, then launches `cargo tauri dev`, which starts Vite for hot-reload UI and the Rust app. The overlay window appears in the chosen corner of the primary monitor; the tray icon shows up next to the clock.

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

1. `cargo tauri dev` launches a translucent always-on-top window in the chosen corner of the primary monitor with the Win11 acrylic surface.
2. Tray icon shows up; `Quit` exits cleanly; `Settings…` opens settings window.
3. Toggling `Connect Codex` writes the five hook entries into `~/.codex/config.toml`, preserves user content, and disconnect removes them cleanly.
4. With Codex Desktop running and a fresh prompt, a session row appears within about a second of the first hook firing.
5. As Codex works, the primary line updates (`Editing X`, `Running: Y`) and elapsed time ticks.
6. On `Stop`, the row shows Done styling with diff scope when available; the hover card lists touched files.
7. Clicking the pill or a fleet dot focuses the Codex Desktop window for that session; Done sessions are acknowledged on focus and drop off the fleet after the delay.
8. Killing `overlay-app.exe` mid-session does not interfere with Codex (hook calls time out in 200ms, agent continues).
9. Single-instance lock prevents a second copy from launching.

## Out of scope for v0.1 (still)

- **Cursor / Claude Code** hook integrations.
- **Persisting** corner and opacity to disk across restarts (`tauri-plugin-store` is present but not fully wired).
- **Stall detection**, cost meter, sound alerts, installer, auto-update.
- **Settings window chrome** refresh (borderless dark window) per ROADMAP.

## Hooks reload

Codex only reads `~/.codex/config.toml` when the app starts. After connecting or disconnecting in Settings, quit Codex Desktop completely and open it again.
