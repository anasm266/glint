import { invoke } from "@tauri-apps/api/core";
import { currentMonitor, type Window } from "@tauri-apps/api/window";
import type { Corner, PanelSide } from "../types";

export const H_COLLAPSED = 60;
export const H_EXPANDED = 300;
export const SHADOW_BUFFER = 16;
export const EDGE_MARGIN = 12;

const PANEL_DY_LOGICAL = H_EXPANDED - H_COLLAPSED;
/** Collapsed vs expanded height threshold (logical px, with slack for DPI rounding). */
const EXPANDED_HEIGHT_MIN = H_COLLAPSED + (H_EXPANDED - H_COLLAPSED) * 0.5;

export interface PhysicalRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WorkAreaRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function defaultPanelSideFromCorner(corner: Corner): PanelSide {
  return corner === "bl" || corner === "br" ? "above" : "below";
}

/** Physical pixels needed below/above the collapsed pill to expand the panel. */
export function panelNeedPhysical(scale: number): number {
  return Math.round(
    PANEL_DY_LOGICAL * scale + (SHADOW_BUFFER + EDGE_MARGIN) * scale
  );
}

/** Pick panel side from free space above vs below the pill within the work area. */
export function resolvePanelSide(
  windowRect: PhysicalRect,
  workArea: WorkAreaRect,
  panelNeed: number
): PanelSide {
  const winBottom = windowRect.y + windowRect.height;
  const winTop = windowRect.y;
  const workAreaBottom = workArea.y + workArea.height;
  const workAreaTop = workArea.y;

  const spaceBelow = workAreaBottom - winBottom;
  const spaceAbove = winTop - workAreaTop;

  if (spaceBelow >= panelNeed) return "below";
  if (spaceAbove >= panelNeed) return "above";
  // Prefer below when both are tight — matches default top-corner UX.
  return spaceBelow >= spaceAbove ? "below" : "above";
}

export async function getPanelSideForWindow(
  win: Window,
  fallback: PanelSide
): Promise<PanelSide> {
  try {
    const [pos, size, monitor, scale] = await Promise.all([
      win.outerPosition(),
      win.outerSize(),
      currentMonitor(),
      win.scaleFactor(),
    ]);

    if (!monitor) return fallback;

    const panelNeed = panelNeedPhysical(scale);
    const collapsedH = Math.round(H_COLLAPSED * scale);
    const expandedThreshold = EXPANDED_HEIGHT_MIN * scale;

    // If still expanded, measure as collapsed (pill sits in the 60px slot).
    const measureHeight =
      size.height > expandedThreshold ? collapsedH : size.height;

    const windowRect: PhysicalRect = {
      x: pos.x,
      y: pos.y,
      width: size.width,
      height: measureHeight,
    };

    const workArea: WorkAreaRect = {
      x: monitor.workArea.position.x,
      y: monitor.workArea.position.y,
      width: monitor.workArea.size.width,
      height: monitor.workArea.size.height,
    };

    return resolvePanelSide(windowRect, workArea, panelNeed);
  } catch {
    return fallback;
  }
}

/** Grow the native window toward `side` before showing the hover panel (keeps pill anchored). */
export async function expandWindowForPanel(
  _win: Window,
  side: PanelSide
): Promise<void> {
  await invoke("set_panel_window_expanded", { expandUp: side === "above" });
}

/** Shrink the native window after the hover panel closes. */
export async function collapseWindowForPanel(
  _win: Window,
  side: PanelSide
): Promise<void> {
  await invoke("set_panel_window_collapsed", {
    collapseFromAbove: side === "above",
  });
}

/**
 * Force the native window back to collapsed height before placement measure.
 * Tries the last-known expand direction first so a stale above-panel expand
 * does not top-anchor collapse (which jumps the pill and skews placement).
 */
export async function ensureWindowCollapsedForMeasure(
  win: Window,
  preferredSide?: PanelSide
): Promise<void> {
  const scale = await win.scaleFactor();
  const threshold = EXPANDED_HEIGHT_MIN * scale;

  let size = await win.outerSize();
  if (size.height <= threshold) return;

  const firstFromAbove = (preferredSide ?? "above") === "above";
  await invoke("set_panel_window_collapsed", {
    collapseFromAbove: firstFromAbove,
  });
  size = await win.outerSize();
  if (size.height <= threshold) return;

  await invoke("set_panel_window_collapsed", {
    collapseFromAbove: !firstFromAbove,
  });
}

/** If a prior collapse failed, normalize to collapsed before expanding. */
export async function ensureWindowCollapsed(
  win: Window,
  side: PanelSide
): Promise<void> {
  const [size, scale] = await Promise.all([win.outerSize(), win.scaleFactor()]);
  if (size.height > EXPANDED_HEIGHT_MIN * scale) {
    await collapseWindowForPanel(win, side);
  }
}
