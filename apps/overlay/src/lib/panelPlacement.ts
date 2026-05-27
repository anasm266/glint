import { invoke } from "@tauri-apps/api/core";
import { currentMonitor, type Window } from "@tauri-apps/api/window";
import type { Corner, PanelSide } from "../types";

export const H_COLLAPSED = 60;
export const H_EXPANDED = 300;
export const SHADOW_BUFFER = 16;
export const EDGE_MARGIN = 24;

const WIN_W = 380;
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

/** Pick panel side from free space above vs below the window within the work area. */
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
  return spaceAbove >= spaceBelow ? "above" : "below";
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

    const panelNeed =
      PANEL_DY_LOGICAL * scale + SHADOW_BUFFER + EDGE_MARGIN;

    const windowRect: PhysicalRect = {
      x: pos.x,
      y: pos.y,
      width: size.width,
      height: size.height,
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

/** If a prior collapse failed, normalize to collapsed before measuring or expanding. */
export async function ensureWindowCollapsed(
  win: Window,
  side: PanelSide
): Promise<void> {
  const [size, scale] = await Promise.all([win.outerSize(), win.scaleFactor()]);
  if (size.height > EXPANDED_HEIGHT_MIN * scale) {
    await collapseWindowForPanel(win, side);
  }
}
