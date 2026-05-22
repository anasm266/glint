# overlay-app

A translucent always-on-top Windows overlay that shows live state of your AI coding agents. v0.1 covers **Codex Desktop** and **Cursor** (user-level hooks); Claude Code is planned.

## Status

v0.1 — Milestone A. **Codex Desktop** and **Cursor** (user-level hooks). **Compact pill** plus **expanded hover detail** on pill or fleet bar. Dogfood build.

Near-term polish is tracked in [ROADMAP.md](ROADMAP.md). Hover panel layout and activity feed behavior are summarized in [HOVER_PANEL.md](HOVER_PANEL.md).

## What it does today

### Compact pill (380×60 collapsed, corner from Settings)

- **Fleet bar** — One dot per agent session (Codex or Cursor; up to eight visible, then `+N` overflow). Dots reflect status: idle, working, done, errored (working dots use a subtle pulse).
- **Primary line** — App label (`Codex` / `Cursor`), project folder name, and the current action (`Editing …`, `Running: …`, or a **Done** summary with diff totals when the run has finished).
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

Hook events are normalized in `apps/overlay/src-tauri/src/session.rs` (`Session::apply`): status, `currentAction`, file diffs, `recentActivity`, `doneSummary`, `lastPrompt`, model, and commit hash hints. Cursor-specific handling includes UTF-8 BOM strip in `overlay-hook.exe`, `Shell` / `Set-Location` command labeling, `tool_output` JSON unwrap, `conversation_id` / `generation_id`, and `afterFileEdit` for diffs. Activity lines use per-turn buffering, parallel flush (`Parallel: A · B · C`), and bash/output classifiers (see [HOVER_PANEL.md](HOVER_PANEL.md)). The UI polls session snapshots via Tauri commands.

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

## Run standalone (no dev server)

You do **not** need `npm run app:dev` for day-to-day use. Build once, then double-click the release binary (or pin it to the taskbar).

**Prerequisites:** same as [Prerequisites](#prerequisites) (Rust MSVC target, Node for the one-time frontend build, WebView2 on Windows).

```powershell
cd apps\overlay
npm install
npm run app:build
```

**Where the `.exe` lands**

| Artifact | Path (from repo root `overlay-app/`) |
|----------|--------------------------------------|
| Overlay app (run this) | `apps\overlay\src-tauri\target\release\overlay-app.exe` |
| Hook relay (installed by Settings) | `target\release\overlay-hook.exe` |
| Installer bundles (optional) | `apps\overlay\src-tauri\target\release\bundle\` (`.msi` / NSIS) |

**First run**

1. Double-click `overlay-app.exe` (or run it from PowerShell). A tray icon appears; the pill shows in the chosen corner.
2. Tray → **Settings…** → **Connect Codex** / **Connect Cursor** (writes managed hooks; restart each agent app).
3. Start a Cursor or Codex session — hooks POST to `http://127.0.0.1:47611/event` with the same 200 ms timeout as in dev.

The release build embeds the Vite UI; no `localhost:5173` process is required. Rebuild with `npm run app:build` after code changes.

## Debug logs (`GLINT_LOG`)

Optional JSONL logs for comparing runs (raw hook input + overlay state after each event).

**Enable** — set before starting the overlay (User or System environment variable, or one-shot in PowerShell):

```powershell
$env:GLINT_LOG = "1"
& "C:\path\to\overlay-app\apps\overlay\src-tauri\target\release\overlay-app.exe"
```

Any non-empty value works except `0` / `false`. When enabled, startup logs the exact file path in the console (`tracing` info line).

**Log directory**

- Windows: `%LOCALAPPDATA%\Glint\logs\` (e.g. `C:\Users\<you>\AppData\Local\Glint\logs\`)
- macOS/Linux fallback: `~/.glint/logs/`

**Per-run file:** `glint-<run_id>.jsonl` where `run_id` is the process start time in ms (one file per overlay launch).

**Line types (JSONL)**

| `type` | When | Contents |
|--------|------|----------|
| `run_start` | App launch | version, exe path, log file path |
| `hook_event` | Each `POST /event` | `event`, `conversation_id`, `session_id`, `rollup_parent`, `status`, `current_action`, full `payload` |
| `snapshot` | After state changes | `sessions[]` with id, app, project, status, `currentAction`, `acknowledgedDone` (what the UI received) |

Logging uses a background thread with append-only writes so hook handling stays within the 200 ms budget.

## Build (release)

Same as [Run standalone](#run-standalone-no-dev-server): `npm run app:build` from `apps\overlay`.

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

User-level `~/.cursor/hooks.json` — nine events: `sessionStart`, `sessionEnd`, `stop`, `preToolUse`, `postToolUse`, `beforeSubmitPrompt`, `afterFileEdit`, `subagentStart`, `subagentStop`. Each entry is `{ "command": "<abs path>/overlay-hook.exe", "overlay_managed": true }`. The hook strips a leading UTF-8 BOM on Windows stdin, reads `hook_event_name` from JSON (camelCase), and `session.rs` normalizes to the same internal event names as Codex. **Multitask / background subagents:** `subagentStop` does not mark Done (parent may still be working); `sessionEnd` and `stop` do.

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
3. Toggling **Connect Codex** / **Connect Cursor** writes managed hook entries (with backup); disconnect removes only `overlay_managed` entries.
4. With Codex or Cursor running and a fresh prompt, a session dot appears within about a second of the first hook firing.
5. As the agent works, the primary line updates (`Editing X`, `Running: Y`) and elapsed time ticks; the hover card shows activity lines when tools run.
6. On `Stop`, the row shows Done styling with diff scope when available; the hover card lists touched files or an assistant summary (Codex) or status-based Done copy (Cursor).
7. **Open Codex** / **Open Cursor** / **Dismiss** in the hover card focus the agent or acknowledge Done; acknowledged sessions drop off the fleet after the delay.
8. Killing `overlay-app.exe` mid-session does not block the agent (hook POST times out in 200ms).
9. Single-instance lock prevents a second copy from launching.

## Out of scope for v0.1 (still)

- **Claude Code** hook integration.
- **Project-level** `.cursor/hooks.json` (user-level only today).
- **Persisting** corner and opacity to disk across restarts (`tauri-plugin-store` is present but not fully wired).
- **Stall detection**, cost meter, sound alerts, installer, auto-update.
- **Settings window chrome** refresh (borderless dark window) per ROADMAP.

## Hooks reload

Codex reads `~/.codex/config.toml` at startup; Cursor watches `~/.cursor/hooks.json` on save but a full restart is safest after first connect. After connecting or disconnecting in Settings, restart the relevant app.
