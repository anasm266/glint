# Roadmap

Near-term polish for the Codex-only build, before Cursor support. Items are ordered roughly by impact; **last preference** means nice-to-have once the rest feels solid.

---

## 1. Compact strip: fleet bar + primary line (full target)

This section records the **intended** UX for the always-visible pill. Today's build only implements a subset; the rest is polish / v0.2.

### Fleet bar (left)

- One **small dot per active session**, colored by state so you can read "3 working, 1 done, 1 errored" at a glance.
- Size the strip so **about 5–8** sessions fit comfortably in the dot row.
- If there are more sessions than fit, collapse the overflow to **`●●● +4`** (a few dots + count of hidden ones).

### Fleet bar dots — interactive

Dots are not just indicators; they are the primary navigation control for multi-session workflows.

- **Single click** a dot → switches the primary line to show that session's info and focuses its Codex window. Selection is **temporary**: auto-priority resumes once that session ends or the user clicks outside the overlay.
- **Double click** a dot → **pins** that session as primary until it ends or the user double-clicks it again to unpin. A subtle ring around the pinned dot shows which session is locked.
- **Hover** a dot → the expanded panel opens anchored to that specific session, not the auto-priority primary.

### Primary line (right)

When **nothing is pinned**, the primary line auto-selects whichever session is most interesting right now using this priority (first match wins):

1. Any session in an **attention** state (error / stall / loop / dangerous command when those exist).
2. Any session that is **Done but not yet acknowledged**.
3. **Most recently active** session (by last hook / activity).

When a dot is **pinned** (double-clicked), the primary line is locked to that session regardless of priority until unpinned or the session ends.

### Done queue

- If **multiple** sessions finish before any are acknowledged, the primary line shows the most recent completion and a count, e.g. **`✓ 3 done`**.
- **Single-clicking** a done dot focuses that session's Codex window and marks it acknowledged, surfacing the next unacked done session in the primary line (queue advances).

### Ack-to-dismiss

- A session's "Done, pending review" state is cleared when the user **focuses its Codex window** — via single-clicking its dot, or via the OS (manually switching to Codex).
- Until acknowledged, Done stays visually distinct (green dot / tint) so nothing gets silently missed.

### Fleet bar: expire finished sessions after focus

After Done + acknowledge, the dot should leave the fleet bar cleanly.

- Remove acknowledged dots with a **smooth staggered animation** (~300–500ms each), bar shrinks when empty.
- A short **5–10 second delay** after the acknowledging click before removal prevents accidental disappearance on stray clicks (tunable).
- If some sessions are still working, finished dots can disappear on ack independently — no need to wait for the full fleet to be idle.

**Technical notes**

- Today `acknowledged_done` is set on focus in `focus_session`; the UI still lists the session. Need a **third state**: "done, acknowledged, pending removal" — remove from `SessionStore` after the animation delay.
- Framer Motion already used in fleet bar — reuse for exit transitions.

---

## 2. Persist window position (corner)

**Problem**
Corner choice (TL / TR / BL / BR) resets when the app restarts.

**Desired behavior**

- Save corner (and optionally last pixel position if we ever allow free-drag snap) on change.
- Restore on startup before showing the main window.

**Technical notes**

- `tauri-plugin-store` is already a dependency — persist e.g. `{ "corner": "tr", "mainOpacity": 0.85 }` under app config dir.
- Apply saved corner in `setup` when positioning the main window.

---

## 3. Wire opacity slider to the overlay

**Problem**
Settings exposes an opacity slider; the value is stored in memory but does not affect the pill or window.

**Desired behavior**

- Slider range stays meaningful (e.g. 60–100%).
- Changing the slider updates **live**:
  - The **CSS** alpha on `.surface` (and any tint overlays), and/or
  - Optional: WebView background opacity if needed for edge cases.
- Persist with the same store as position so restarts keep the choice.

**Technical notes**

- Pass opacity from Rust to the webview via `window.emit` or a small Tauri command `get_chrome_settings` on load + `set_opacity` already partially exists — extend frontend to read and apply CSS variable `--surface-alpha` on the root.

---

## 4. Stall detection

**Problem**
In full-auto Codex mode, long gaps with no hook events can mean "still thinking / big edit" or "stuck." The compact row should surface ambiguity without crying wolf.

**Desired behavior**

- If **no hook events** for **3 minutes** while status is still "working," treat as stale / possible stall:
  - Amber pulse on that session's dot (or primary line tint).
  - Copy along the lines of **"Idle 3m"** or **"No activity for 3m"** — keep it neutral; color does the "look here" work.
- Reset the stall timer on any event.

**Technical notes**

- Derived state in Rust: a 1 Hz tick or event-driven `last_event_at` comparison; no change to Codex config.
- Configurable threshold later (default 3m).

---

## 5. Settings window chrome (preferred: borderless dark)

**Problem**
Settings uses native decorated window + dark web content → light system title bar clashes with the rest of the UI.

**Desired behavior** (preferred)

- **Borderless** settings window aligned with the main overlay: dark surface, rounded corners, `data-tauri-drag-region` on header, custom close button, same translucency language as the pill.
- Optional: traffic-light style close only to keep scope small.

**Alternative** (if borderless is painful on Windows)

- Force dark title bar via platform APIs / Tauri theme hints where supported, and match background to title bar color so the split is less jarring.

---

## 6. App icon — **last preference**

Replace the placeholder generated icon with a deliberate mark once the product name and visual direction are final. Low priority vs behavior and readability.

---

## Out of scope for this document

- Cursor / Claude hook integrations (separate milestone after Codex polish).
- Installer / auto-update / code signing (shipping milestone).
