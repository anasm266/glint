# Hover panel — current behavior

Reference for the detail card in `HoverPanel.tsx`, fed by `session.rs` hook mapping. See [README.md](README.md) for pill-level UX.

## When it opens

Pointer over the **pill row** or **fleet bar** expands the main window (~380×300) and shows the card for the **primary** session (respecting temporary dot selection). Leaving the pill/card collapses after a short debounce.

## Layout

```
┌──────────────────────────────────────────────┐
│ You asked: refactor the auth middleware…     │  context (or "New session")
│                        my-app · Codex · gpt-4│
│                                              │
│ Running: npm test                            │  currentAction (working)
│ Parallel: git status · rg foo · npm test     │  activity feed (optional)
│ 12 tests passed                              │  success/failure tint
│                                              │
│ Running 4m 20s     [Open Codex / Open Cursor] │  action strip
└──────────────────────────────────────────────┘
```

**Done** — assistant `doneSummary` when present; else `+N / −M across K files` plus scrollable basenames; commit short hash inline when parsed. **Errored** — `currentAction` or fallback copy.

## Activity feed (working)

- Up to **8** entries, newest first; fade mask at bottom.
- **Normal** / **success** (emerald) / **failure** (rose) kinds from `PostToolUse` parsers.
- **Parallel** lines are italic/muted (`Parallel: …`).
- Duplicate summary in the same turn: `×N` on the head entry.

### Rust sources (`session.rs`)

| Source | Examples |
|--------|----------|
| `PreToolUse` per turn | Buffered actions; flush on turn change / `PostToolUse` / `Stop` as one line or `Parallel: A · B · C` |
| Bash classifier | `git status`, `Running: jest`, `Searching: foo.ts`, `Reading: README.md`, `Get-Content` ranges, heredoc → node |
| `PostToolUse` Bash output | Test pass/fail counts, `rg` match summary, `git log` / `git blame`, `gh` PR/issue JSON, `CASE:` node probes |
| Commit guard | `lastCommitHash` only after commit/push-style bash; shown on Done when no file diffs |

## Actions

| Control | Effect |
|---------|--------|
| Open Codex / Open Cursor | `open_codex` or `open_cursor` — focus agent window by `session.app` (`parent_pid` chain + process name walk) |
| Dismiss | `acknowledge_done` — marks Done acknowledged; fleet removal ~7s later |
| Time label | Elapsed only in UI; `title` tooltip has absolute start/finish |

## Not in the panel

- No elapsed/clock toggle (removed; tooltip carries absolute times).
- Fleet dots are **status-colored only** (not app-colored); app name is on the compact primary line.
- Fleet dot click = **temporary** primary only (no focus); use **Open Codex** / **Open Cursor** to focus.
- Cursor: `afterFileEdit` updates file diff totals; `Stop` uses `status` (`completed` / `aborted` / `error`), not `last_assistant_message`. `subagentStart` / `subagentStop` keep the parent **working** (activity line on stop; `N subagents running` while parallel workers run). `sessionEnd` marks **Done** when the composer closes even if `stop` was delayed. Hook stdin may include a UTF-8 BOM (stripped in `overlay-hook.exe`).
