# Hover Panel — Feature Spec

This document covers the four additions planned for the hover panel that appears when a user mouses over a fleet-bar dot.

---

## Current state

The panel already shows:

- `project · App` header
- `currentAction` (e.g. "Editing src/auth.ts" / "Thinking…" / "Done · +89 / −38 across 6 files")
- Elapsed label ("Running 4m 20s" / "Finished 2m ago")

The panel is `pointer-events-none` — it cannot be clicked.

---

## Feature 1 — File list for Done state

### Problem

When a session finishes, the primary line and hover panel both show a single summary string: `Done · +89 / −38 across 6 files`. A developer can't tell what changed without switching to Codex. The data is already there — `filesEdited: Array<[string, DiffStat]>` is fully populated on `Stop`.

### Desired behaviour

Replace the single summary line in the Done hover panel with a scrollable file list, one row per file:

```
src/auth.ts              +42 / −18
src/middleware.ts        +31 / −12
tests/auth.test.ts       +16 / −8
```

- File paths are right-truncated with `…` if too long (align the stats column to the right).
- List is scrollable when it exceeds ~4 rows (the panel height is fixed by the window expand).
- A compact total summary (`+89 / −38 · 6 files`) sits below the list as a footer in muted text.
- If `filesEdited` is empty at Done (pure reasoning run, no patches), fall back to "Done — no files changed".

### Data source

`session.filesEdited: Array<[string, DiffStat]>` — already sorted alphabetically by `flatten_files()` in Rust on `Stop`. `DiffStat` has `adds: number` and `dels: number`.

### Files changed

- `apps/overlay/src/components/HoverPanel.tsx` — replace `doneAction()` string with a list component for Done state.

---

## Feature 2 — "X files touched so far" during Working

### Problem

During a working session the panel only shows `currentAction` and elapsed time. `filesEdited` already accumulates live (updated on every `PostToolUse` for `apply_patch` calls), so there is real progress information available that goes unused.

### Desired behaviour

Below `currentAction`, show a muted live counter when at least one file has been touched:

```
3 files touched so far
```

- Only shown when `status === "working"` and `filesEdited.length > 0`.
- Updates automatically as hook events arrive and the store re-renders.
- No list — just the count. The full list is the Done-state feature above.

### Data source

`session.filesEdited.length` — already live during working.

### Files changed

- `apps/overlay/src/components/HoverPanel.tsx` — add the counter line to the working branch.

---

## Feature 3 — Toggleable time display

### Problem

The current elapsed label in the hover panel (`Running 4m 20s`) tells you duration but not *when* the session started. For multi-session workflows where you want to correlate agent activity with your own actions ("was this running before or after I made that change?"), a clock time is more useful. Both modes are useful depending on context.

### Desired behaviour

The time line in the hover panel is a toggle button. Clicking it switches between two modes:

| Mode | Working | Done |
|------|---------|------|
| **Elapsed** (default) | `Running 4m 20s` | `Finished 2m ago` |
| **Clock** | `Started 9:42 AM` | `Finished at 9:46 AM` |

- Toggle state is **per-session** (persisted in a `Map<id, mode>` in local component state — not in the Zustand store, since it's purely presentational).
- Default is **elapsed**.
- Visual treatment: the toggle is `cursor-pointer`, underline on hover, no border or button chrome — keeps the panel clean.
- The pill's `StatusBadge` is **not** affected — it stays elapsed-only.

### Data source

- `session.startedAtMs` — start clock time for working.
- `session.lastEventAtMs` — finish clock time for done (the `Stop` event timestamp).

### Time formatting

```
// Clock mode
9:42 AM                  (same-day, 12-hour, no seconds)
Yesterday 9:42 AM        (if startedAtMs is a different calendar day)

// Elapsed mode (existing behaviour, unchanged)
Running 4m 20s
Finished 2m ago
```

### Files changed

- `apps/overlay/src/components/HoverPanel.tsx` — replace the static elapsed label with a toggle button; add `clockMode` local state (Map or per-panel boolean).

---

## Feature 4 — Last prompt

### Problem

When you have 3 sessions running in parallel you often forget what you asked each one to do. The hover panel is the natural place to surface this without cluttering the pill.

### Desired behaviour

A prompt line appears at the top of the panel, above the action line:

```
↳ "refactor the auth middleware to use the new JWT helper"
```

- Shown for both Working and Done states (the prompt that started the current run).
- Truncated to 2 lines max with CSS `line-clamp-2`; full text on native `title` tooltip.
- If no prompt has been captured yet (e.g. `SessionStart` fired but `UserPromptSubmit` hasn't), the line is omitted.
- The `↳` prefix is muted (`text-white/35`); the prompt text is `text-white/70`.

### Required Rust changes

**1. Add field to `Session` in `session.rs`:**

```rust
pub last_prompt: String,
```

Initialise to `String::new()` in `Session::new`.

**2. Capture in `apply()` in `session.rs` for the `UserPromptSubmit` branch:**

The payload field name needs to be confirmed from a real hook log. Likely candidates based on Codex hook conventions: `"input"`, `"prompt"`, or `"user_message"`. Log one `UserPromptSubmit` event to disk or stdout to confirm, then:

```rust
"UserPromptSubmit" => {
    entry.status = Status::Working;
    entry.current_action = "Thinking…".to_string();
    entry.acknowledged_done = false;
    entry.started_at_ms = raw.ts;
    // Capture prompt — field name TBC from a logged sample:
    if let Some(prompt) = p.get("input").and_then(|v| v.as_str()) {
        entry.last_prompt = prompt.to_string();
    }
}
```

**3. `last_prompt` is already serialized** because `Session` derives `Serialize` with `rename_all = "camelCase"` — it will appear as `lastPrompt` in the JSON sent to the frontend automatically.

**4. Add to `SessionDTO` in `types.ts`:**

```typescript
lastPrompt: string;
```

**5. Confirm payload field name** — run `cargo tauri dev`, trigger a Codex session, and add a temporary `tracing::info!("{:?}", raw.payload)` log line in the `UserPromptSubmit` branch to print the full payload to the console. Check the field name, then remove the log line.

### Files changed

- `apps/overlay/src-tauri/src/session.rs` — add `last_prompt` field, capture in `UserPromptSubmit`
- `apps/overlay/src/types.ts` — add `lastPrompt: string`
- `apps/overlay/src/components/HoverPanel.tsx` — render the prompt line

---

## Implementation order

1. Feature 2 (files-touched counter during working) — 5 lines, lowest risk, do first.
2. Feature 1 (file list for Done) — pure frontend, no dependencies.
3. Feature 3 (time toggle) — self-contained UI state, no data changes.
4. Feature 4 (last prompt) — blocked on confirming the `UserPromptSubmit` payload field name.

---

## Panel layout (target, all features present)

```
┌─────────────────────────────────────┐
│ ↳ "refactor the auth middleware…"   │  ← Feature 4 (lastPrompt), omitted if empty
│                                     │
│ project · Codex                     │  ← existing header
│ Editing src/auth.ts                 │  ← existing currentAction
│ 3 files touched so far              │  ← Feature 2 (working only)
│                                     │
│ Started 9:42 AM ⇄                   │  ← Feature 3 (toggleable, click to switch)
└─────────────────────────────────────┘

Done state:
┌─────────────────────────────────────┐
│ ↳ "refactor the auth middleware…"   │
│                                     │
│ project · Codex                     │
│ src/auth.ts               +42 / −18 │  ← Feature 1 (file list)
│ src/middleware.ts          +31 / −12 │
│ tests/auth.test.ts         +16 / −8  │
│ ─────────────────────────────────── │
│ +89 / −38 · 6 files                 │  ← total footer
│                                     │
│ Finished at 9:46 AM ⇄               │  ← Feature 3
└─────────────────────────────────────┘
```
