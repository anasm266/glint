import { useSessions } from "../store/sessions";

const DELAY_MS = 120;

let timer: ReturnType<typeof setTimeout> | null = null;

/** Cancel a pending hover collapse (call on pill/panel enter). */
export function cancelScheduledHoverLeave(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
}

/** Debounced collapse when leaving the pill row or hover panel. */
export function scheduleHoverLeaveClear(): void {
  cancelScheduledHoverLeave();
  timer = setTimeout(() => {
    timer = null;
    useSessions.getState().setPillPanelHovered(false);
  }, DELAY_MS);
}
