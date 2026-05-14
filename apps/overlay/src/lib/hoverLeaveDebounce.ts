import { useSessions } from "../store/sessions";

const DELAY_MS = 120;

let timer: ReturnType<typeof setTimeout> | null = null;

/** Cancel a pending hover collapse (call on dot enter or panel enter). */
export function cancelScheduledHoverLeave(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
}

/** Debounced collapse when leaving a dot or the hover panel. */
export function scheduleHoverLeaveClear(): void {
  cancelScheduledHoverLeave();
  timer = setTimeout(() => {
    timer = null;
    useSessions.getState().setHoveredDotId(null);
  }, DELAY_MS);
}
