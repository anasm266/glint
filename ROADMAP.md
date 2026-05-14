# Roadmap

Near-term polish for the Codex-only build, before Cursor support. Items are ordered roughly by impact; **last preference** means nice-to-have once the rest feels solid.

---

## 1. Fleet bar: expire finished sessions after focus

**Problem**  
When a session reaches **Done** and you acknowledge it (click the overlay to focus Codex), the corresponding dot stays in the fleet bar until you restart the app. With several parallel agents, the bar fills with stale dots.

**Desired behavior**

- Up to **5** dots remain the cap (already the UI target).
- When **all visible sessions are Done**, clicking the overlay (on any “done” state / primary line) should:
  1. Focus the Codex window (existing behavior).
  2. Mark that session as fully dismissed for the fleet bar.
  3. **Remove its dot** with a short, smooth animation (about **5–10 seconds** after the click — not instant, so accidental clicks do not flash away; tune in implementation).
- If some sessions are still **working**, dots for finished ones can either stay until everything is done, or use a simpler rule: dismiss only the primary session’s dot after focus — product decision when implementing; default suggestion: dismiss **all Done + acknowledged** sessions after a successful focus, each dot animating out over ~300–500ms staggered, with the bar shrinking when empty.

**Technical notes**

- Today `acknowledged_done` is set on focus in `focus_session`; the UI still lists the session. Need a **third state** or filter: “done, acknowledged, and removed from fleet” after animation completes, or remove from `SessionStore` after delay.
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
In full-auto Codex mode, long gaps with no hook events can mean “still thinking / big edit” or “stuck.” The compact row should surface ambiguity without crying wolf.

**Desired behavior**

- If **no hook events** for **3 minutes** while status is still “working,” treat as **stale / possible stall**:
  - Amber pulse on that session’s dot (or primary line tint).
  - Copy along the lines of **“Idle 3m”** or **“No activity for 3m”** — keep it neutral; color does the “look here” work.
- Reset the stall timer on any event.

**Technical notes**

- Derived state in Rust: a 1 Hz tick or event-driven `last_event_at` comparison; no change to Codex config.
- Configurable threshold later (default 3m).

---

## 5. Settings window chrome (preferred: borderless dark)

**Problem**  
Settings uses native **decorated** window + dark web content → light system title bar clashes with the rest of the UI.

**Desired behavior** (preferred)

- **Borderless** settings window aligned with the main overlay: dark surface, rounded corners, `data-tauri-drag-region` on header, custom close button, same translucency language as the pill.
- Optional: **traffic-light** style close only to keep scope small.

**Alternative** (if borderless is painful on Windows)

- Force dark title bar via platform APIs / Tauri theme hints where supported, and match background to title bar color so the split is less jarring.

---

## 6. App icon — **last preference**

Replace the placeholder generated icon with a deliberate mark once the product name and visual direction are final. Low priority vs behavior and readability.

---

## Out of scope for this document

- Cursor / Claude hook integrations (separate milestone after Codex polish).
- Installer / auto-update / code signing (shipping milestone).
