# Hover panel

Notes on the detail card in `HoverPanel.tsx`, fed by the hook mapping in
`session.rs`. See the README for the pill-level UX.

Hovering the pill row or fleet bar expands the main window (~380x300) and
shows the card for the primary session (or the temporarily selected dot).
Leaving the pill or card collapses it after a short debounce.

The card shows, top to bottom: the last user prompt (or "New session"), the
project / app / model line, the current action while working, a short activity
feed, and an action strip with elapsed time plus Open and Dismiss buttons.
For Done sessions it shows the assistant's summary when one exists, otherwise
a `+N / -M across K files` diff line with scrollable basenames, and the commit
short hash when one was parsed. Errored sessions fall back to the last action.

## Activity feed

Up to 8 entries, newest first, with a fade mask at the bottom. Entries are
tinted emerald or rose when a `PostToolUse` parser recognizes success or
failure output (test pass/fail counts, `rg` match summaries, `git log`, `gh`
JSON, and so on). Tool calls that run in parallel within a turn are flushed
as one italic `Parallel: A · B · C` line. A repeated summary in the same turn
collapses to an `xN` counter on the head entry.

The bash classifier in `session.rs` turns raw commands into readable labels:
`Running: jest`, `Searching: foo.ts`, `Reading: README.md`, `Get-Content`
ranges, heredoc-to-node probes, and plain git commands.

## Actions

- **Open Codex / Open Cursor / Open Claude** focuses the agent window for the
  session's app by walking the `parent_pid` chain and process names.
- **Dismiss** acknowledges a Done session; the dot is removed ~7s later.
- The elapsed-time label carries absolute start/finish times in its tooltip.

## Per-app quirks

Cursor's `afterFileEdit` updates diff totals, `Stop` carries a `status` rather
than an assistant message, and subagent start/stop events keep the parent
marked as working (`N subagents running`). `sessionEnd` marks Done even when
`stop` is delayed. Cursor hook stdin can include a UTF-8 BOM, which
`overlay-hook.exe` strips. Claude Code sessions are identified by a
`transcript_path` under `~/.claude/projects/` so they aren't confused with
Codex sessions.
