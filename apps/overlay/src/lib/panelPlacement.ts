import {
  currentMonitor,
  LogicalSize,
  PhysicalPosition,
  type Window,
} from "@tauri-apps/api/window";
import type { Corner, PanelSide } from "../types";

export const H_COLLAPSED = 60;
export const H_EXPANDED = 300;
export const SHADOW_BUFFER = 16;
export const EDGE_MARGIN = 24;

const WIN_W = 380;
const PANEL_DY_LOGICAL = H_EXPANDED - H_COLLAPSED;

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
      (H_EXPANDED - H_COLLAPSED) * scale + SHADOW_BUFFER + EDGE_MARGIN;

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

async function panelDyPhysical(win: Window): Promise<number> {
  const scale = await win.scaleFactor();
  return Math.round(PANEL_DY_LOGICAL * scale);
}

/** Grow the native window toward `side` before showing the hover panel (keeps pill anchored). */
export async function expandWindowForPanel(
  win: Window,
  side: PanelSide
): Promise<void> {
  if (side === "above") {
    const [pos, dy] = await Promise.all([win.outerPosition(), panelDyPhysical(win)]);
    await win.setPosition(new PhysicalPosition(pos.x, pos.y - dy));
  }
  await win.setSize(new LogicalSize(WIN_W, H_EXPANDED));
}

/** Shrink the native window after the hover panel closes. */
export async function collapseWindowForPanel(
  win: Window,
  side: PanelSide
): Promise<void> {
  await win.setSize(new LogicalSize(WIN_W, H_COLLAPSED));
  if (side === "above") {
    const [pos, dy] = await Promise.all([win.outerPosition(), panelDyPhysical(win)]);
    await win.setPosition(new PhysicalPosition(pos.x, pos.y + dy));
  }
}
