# Roadmap

Polish list for the v0.1 dogfood build. Codex, Cursor, and Claude Code hooks
are all shipped; what's left is mostly UX detail.

## Fleet bar polish

- Staggered dot exit when several Done sessions get acked in a row.
- Tune the post-ack removal delay (7s in `sessions.ts` right now).
- Maybe ack Done automatically when the user focuses the agent outside the
  overlay. Today it takes Dismiss or a hover Open.
- Undecided: whether dots should encode app identity (Codex vs Cursor vs
  Claude) with color/shape, or stay status-only with the app name on the
  primary line. Needs more days of running mixed fleets before committing.

## Window behavior

- Persist the chosen corner (and opacity) across restarts —
  `tauri-plugin-store` is already a dependency, just needs wiring.
- The settings opacity slider is stored but not applied; hook it up to a CSS
  variable on the overlay surface and persist it.
- Borderless dark settings window. The native title bar is light and clashes
  with everything else. If borderless is too painful on Windows, at least
  force a dark title bar.

## Stall detection

If a session is "working" but no hook events arrive for ~3 minutes, tint the
dot amber with something like "Idle 3m". Big edits and long thinking pauses
look identical to a hang from the outside, so keep the copy neutral and let
the color do the work. Reset on any event.

## Eventually

- Real app icon.
- Project-level `.cursor/hooks.json` support (user-level only today).
- Installer, auto-update, code signing.
