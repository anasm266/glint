import { useSessions } from "../store/sessions";

const DELAY_MS = 120;

let timer: ReturnType<typeof setTimeout> | null = null;
let leaveBlocked: (() => boolean) | null = null;
let openGraceUntilMs = 0;

/** Register a guard — when true, hover-leave collapse is ignored. */
export function setHoverLeaveGuard(guard: (() => boolean) | null): void {
  leaveBlocked = guard;
}

/** Ignore hover-leave briefly after open (window reposition can fire spurious leave). */
export function markHoverOpenGrace(ms = 250): void {
  openGraceUntilMs = Date.now() + ms;
}

function leaveBlockedNow(): boolean {
  if (Date.now() < openGraceUntilMs) return true;
  return leaveBlocked?.() ?? false;
}

/** Cancel a pending hover collapse (call on pill/panel enter). */
export function cancelScheduledHoverLeave(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
}

/** Debounced collapse when leaving the pill row or hover panel. */
export function scheduleHoverLeaveClear(): void {
  if (leaveBlockedNow()) return;
  cancelScheduledHoverLeave();
  timer = setTimeout(() => {
    timer = null;
    if (leaveBlockedNow()) return;
    useSessions.getState().setPillPanelHovered(false);
  }, DELAY_MS);
}

/** Force-clear hover state (e.g. after a failed open or drag). */
export function clearHoverState(): void {
  cancelScheduledHoverLeave();
  useSessions.getState().setPillPanelHovered(false);
}
