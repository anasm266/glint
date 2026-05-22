# Roadmap

Near-term polish for the Codex-only build, before Cursor support. Items are ordered roughly by impact; **last preference** means nice-to-have once the rest feels solid.

---

## 1. Compact strip: fleet bar + primary line

**Shipped in v0.1 dogfood:** fleet dots (8 + overflow), temp select on dot click, auto primary priority (error → unacked Done → recent), done queue prefix on primary line, hover card with activity feed + Done file list + assistant summary, ack via **Dismiss** and ~7s removal.

**Still open** (see subsections below):

### Fleet bar (left)

- One **small dot per active session**, colored by state so you can read "3 working, 1 done, 1 errored" at a glance.
- Size the strip so **about 5–8** sessions fit comfortably in the dot row.
- If there are more sessions than fit, collapse the overflow to **`+N`** count of hidden sessions.

### Fleet bar dots — interactive

- **Single click** a dot → temporary primary selection (ring); clears on overlay blur. Does **not** focus Codex (use hover **Open Codex**).
- **Hover** pill or bar → expanded panel for the current primary session.

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

- **Dismiss** in the hover card (or equivalent command) sets `acknowledgedDone`; Done tint stays until ack.
- Optional later: also ack when user focuses Codex from outside the overlay.

### Fleet bar: expire finished sessions after ack

**Shipped:** ~7s timer after ack, then `remove_session`; dot exit animation via Framer Motion.

**Polish still open:**

- Staggered exit when multiple Done sessions ack in a row.
- Tune delay (currently 7s fixed in `sessions.ts`).

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
