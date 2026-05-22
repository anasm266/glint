# overlay-app

A translucent always-on-top Windows overlay that shows live state of your AI coding agents. v0.1 covers **Codex Desktop** and **Cursor** (user-level hooks); Claude Code is planned.

## Status

v0.1 — Milestone A. Codex Desktop only. **Compact pill** plus an **expanded hover detail** when you point at the pill or fleet dots. Dogfood build.

Near-term polish is tracked in [ROADMAP.md](ROADMAP.md). Hover panel layout and activity feed behavior are summarized in [HOVER_PANEL.md](HOVER_PANEL.md).

## What it does today

### Compact pill (380×60 collapsed, corner from Settings)

- **Fleet bar** — One dot per agent session (Codex or Cursor; up to eight visible, then `+N` overflow). Dots reflect status: idle, working, done, errored (working dots use a subtle pulse).
- **Primary line** — App label (`Codex`), project folder name, and the current action (`Editing …`, `Running: …`, or a **Done** summary with diff totals when the run has finished).
- **Done queue** — If more than one session is **Done** and not yet acknowledged, the primary line is prefixed with `✓ N done ·` before the usual Done summary for the frontmost completion.
- **Status badge** — Elapsed time while working; **just now** / **ago** after Done.
- **Hooks disconnected hint** — If neither Codex nor Cursor hooks are installed, the pill shows **Connect in Settings to get started** (click opens Settings). When sessions exist but hooks are still off, a small **hollow amber dot** appears after the fleet; click it to open Settings.

**Primary selection** — Auto-priority: errored → unacknowledged Done → most recently active. **Single-click a fleet dot** to temporarily switch the primary line to that session (clears when the overlay loses focus). Clicking the pill body clears that temporary selection.

The pill row is draggable (`data-tauri-drag-region`). **Tray:** Show, Settings…, Quit. **Single instance** — launching again focuses the existing overlay.

### Hover detail card

Opens when the pointer is over the pill or fleet bar (expands the window to ~380×300). The card is **interactive** — buttons do not steal drag from the pill.

- **Context row** — `You asked: …` from the last prompt submit (truncated; full text in tooltip), or **New session**. Right side: `project · Codex|Cursor` and model name when known.
- **While working** — Bold `currentAction` from the latest `PreToolUse` (patch targets, bash labels, MCP tools). Below that, a short **activity feed** (up to eight recent lines) built from hook events:
  - Per-turn buffer: parallel tool calls in one turn flush as `Parallel: A · B · C` (with `+N more` when capped).
  - Bash commands are classified (git, npm/cargo, rg, gh, PowerShell `Get-Content` ranges, heredoc/pipe-to-node, etc.) with dedupe within a turn.
  - `PostToolUse` adds lines for test results (green/red), ripgrep matches, git log/blame, `gh` PR/issue JSON, node `CASE:` probes, and commit hashes when the last bash was a commit/push-style command.
  - Repeated identical summaries in the same turn bump a `×N` counter on the newest line.
- **When Done** — Prefer a one- or two-line **assistant summary** from `Stop` when present; otherwise diff totals and a scrollable per-file list (`basename` + `+adds / −dels`), or **No files changed** / **Pushed {hash}** when only a commit was recorded.
- **Action strip** — Elapsed label (`Running …` / `Finished …`; hover title shows absolute start/finish time). **Open Codex** / **Open Cursor** (by session app) and **Dismiss** (acknowledges Done without focusing) as appropriate.

Acknowledged Done sessions are removed from the fleet after about **seven seconds** (scheduled in the frontend when `acknowledgedDone` flips true).

### Settings

- **Connect Codex** — Installs or removes managed hook entries in `~/.codex/config.toml` (with backup). Restart Codex after toggling.
- **Connect Cursor** — Installs or removes managed hook entries in `~/.cursor/hooks.json` (with backup). Restart Cursor after toggling.
- **Position** — Corners of the primary monitor (top-left / top-right / bottom-left / bottom-right).
- **Opacity** — Slider updates in-app state (full visual wiring is still on the roadmap).

When the overlay regains focus, it refreshes **corner** and **hook installed** state from disk so changes from Settings stay in sync.

## Architecture

Two binaries:

- `overlay-app.exe` — the Tauri app: Rust core (HTTP listener + session store) + React WebView UI + system tray
- `overlay-hook.exe` — a tiny stdin-to-HTTP relay that Codex and Cursor hooks point at

```
Codex / Cursor session
     |
     | spawns per hook event, JSON on stdin
     v
overlay-hook.exe  -- POST http://127.0.0.1:47611/event (200ms fire-and-forget) -->  overlay-app.exe
     |
     | stdout always `{}` on exit 0
     v
   agent continues
```

Hook events are normalized in `apps/overlay/src-tauri/src/session.rs` (`Session::apply`): status, `currentAction`, file diffs, `recentActivity`, `doneSummary`, `lastPrompt`, model, and commit hash hints. The UI polls/streams snapshots via Tauri commands.

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

Open **Settings…** from the tray and toggle **Connect Codex** and/or **Connect Cursor**. Codex entries go into `~/.codex/config.toml` (backup `config.toml.overlay-backup`). Cursor entries go into `~/.cursor/hooks.json` (backup `hooks.json.overlay-backup`). Restart each app after toggling. Disconnecting removes only entries we added (`overlay_managed`).

## Build (release)

```powershell
cd apps\overlay
npm run app:build
```

The unpacked `overlay-app.exe` and an `.msi` end up under `apps/overlay\src-tauri\target\release\bundle\`. The release `overlay-hook.exe` is at `target\release\overlay-hook.exe` (workspace root). For "live with it for a day" testing, run the unpacked overlay-app.exe directly; the hook installer will resolve the hook binary path correctly via `current_exe()`.

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

The `overlay_managed = true` marker is what disconnect uses to find and remove only our entries; user-authored hook entries are preserved.

## Cursor hook config written by "Connect Cursor"

User-level `~/.cursor/hooks.json` — six events: `sessionStart`, `stop`, `preToolUse`, `postToolUse`, `beforeSubmitPrompt`, `afterFileEdit`. Each entry is `{ "command": "<abs path>/overlay-hook.exe", "overlay_managed": true }`. The hook reads `hook_event_name` from stdin JSON (camelCase); `session.rs` normalizes to the same internal event names as Codex.

### Payload differences (Cursor vs Codex)

| Area | Codex | Cursor |
|------|-------|--------|
| Session id | `session_id` | `conversation_id` (same value on `sessionStart`) |
| Project root | `cwd` | `cwd` or `workspace_roots[0]` |
| Shell tool | `Bash` | `Shell` |
| File edit tool | `apply_patch` | `Write` + `afterFileEdit` |
| Tool output field | `tool_response` | `tool_response` or `tool_output` |
| Turn id | `turn_id` | `generation_id` |
| Stop | `last_assistant_message` | `status`: `completed` / `aborted` / `error` |

## Acceptance criteria for v0.1

1. `cargo tauri dev` launches a translucent always-on-top window in the chosen corner of the primary monitor with the Win11 acrylic surface.
2. Tray icon shows up; `Quit` exits cleanly; `Settings…` opens settings window.
3. Toggling `Connect Codex` writes the five hook entries into `~/.codex/config.toml`, preserves user content, and disconnect removes them cleanly.
4. With Codex Desktop running and a fresh prompt, a session row appears within about a second of the first hook firing.
5. As Codex works, the primary line updates (`Editing X`, `Running: Y`) and elapsed time ticks; the hover card shows activity lines when tools run.
6. On `Stop`, the row shows Done styling with diff scope when available; the hover card lists touched files or an assistant summary.
7. **Open Codex** / **Dismiss** in the hover card focus Codex or acknowledge Done; acknowledged sessions drop off the fleet after the delay.
8. Killing `overlay-app.exe` mid-session does not interfere with Codex (hook calls time out in 200ms, agent continues).
9. Single-instance lock prevents a second copy from launching.

## Out of scope for v0.1 (still)

- **Claude Code** hook integration.
- **Project-level** `.cursor/hooks.json` (user-level only today).
- **Persisting** corner and opacity to disk across restarts (`tauri-plugin-store` is present but not fully wired).
- **Stall detection**, cost meter, sound alerts, installer, auto-update.
- **Settings window chrome** refresh (borderless dark window) per ROADMAP.

## Hooks reload

Codex reads `~/.codex/config.toml` at startup; Cursor watches `~/.cursor/hooks.json` on save but a full restart is safest after first connect. After connecting or disconnecting in Settings, restart the relevant app.
