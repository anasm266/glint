/** Exit duration for hover panel shell — keep in sync with HoverPanel motion config. */
export const HOVER_PANEL_EXIT_MS = 260;

export function waitFrames(count = 2): Promise<void> {
  return new Promise((resolve) => {
    const step = (remaining: number) => {
      if (remaining <= 1) {
        resolve();
        return;
      }
      requestAnimationFrame(() => step(remaining - 1));
    };
    requestAnimationFrame(() => step(count));
  });
}

export function waitMs(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
