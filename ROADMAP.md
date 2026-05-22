# Roadmap

Near-term polish for the v0.1 dogfood build (Codex + Cursor hooks shipped). Items are ordered roughly by impact; **last preference** means nice-to-have once the rest feels solid.

---

## 1. Compact strip: fleet bar + primary line

**Shipped in v0.1 dogfood:**

- Fleet dots (8 + `+N` overflow), **status-colored** (idle / working / done / errored; working pulse)
- Primary line shows **app label** (`Codex` / `Cursor`) + project + action
- Auto primary priority: errored → unacknowledged Done → most recently active
- **Temporary** primary on single dot click (ring); clears on overlay blur; pill click clears temp
- Done queue prefix `✓ N done ·` on primary when multiple unacked Done
- Hover card: activity feed, Done file list / assistant summary, **Open Codex|Cursor**, **Dismiss**, ~7s removal after ack
- Dual **Connect** toggles in Settings (`~/.codex/config.toml`, `~/.cursor/hooks.json`)

**Polish still open:**

- Staggered dot exit when multiple Done sessions ack in a row
- Tune post-ack removal delay (currently 7s in `sessions.ts`)
- Optional: ack Done when user focuses agent from outside the overlay (today: **Dismiss** or hover **Open**)

---

## Explore / future: fleet bar dot encoding (UX)

**Not committed design** — for later dogfooding with mixed Codex + Cursor sessions.

Today fleet dots encode **status only**; app identity lives on the **primary line** (`Codex` / `Cursor`). Options to explore:

- **App color** — tint dots by agent (Cursor vs Codex) instead of or in addition to status
- **Status-only** — keep current model; rely on primary line for app
- **Hybrid** — e.g. app hue + status pulse, shape per app, or fixed slot order (Codex left, Cursor right)

Tradeoff: mixed-fleet glanceability vs reading state at a dot row. User will explore when running both agents daily.

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
In full-auto agent mode, long gaps with no hook events can mean "still thinking / big edit" or "stuck." The compact row should surface ambiguity without crying wolf.

**Desired behavior**

- If **no hook events** for **3 minutes** while status is still "working," treat as stale / possible stall:
  - Amber pulse on that session's dot (or primary line tint).
  - Copy along the lines of **"Idle 3m"** or **"No activity for 3m"** — keep it neutral; color does the "look here" work.
- Reset the stall timer on any event.

**Technical notes**

- Derived state in Rust: a 1 Hz tick or event-driven `last_event_at` comparison; no change to agent hook config.
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

- **Claude Code** hook integration (planned; same hook relay pattern).
- **Project-level** `.cursor/hooks.json` (user-level only today).
- Installer / auto-update / code signing (shipping milestone).
