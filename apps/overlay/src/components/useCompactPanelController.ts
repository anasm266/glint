import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { HOVER_PANEL_EXIT_MS, waitFrames, waitMs } from "../lib/hoverPanelMotion";
import {
  collapseWindowForPanel,
  defaultPanelSideFromCorner,
  ensureWindowCollapsed,
  ensureWindowCollapsedForMeasure,
  expandWindowForPanel,
  getPanelSideForWindow,
} from "../lib/panelPlacement";
import type { Corner, PanelSide } from "../types";

const HOVER_LEAVE_DELAY_MS = 120;

type PanelPhase = "collapsed" | "opening" | "open" | "closing" | "dragging";

interface ControllerArgs {
  corner: Corner;
  hasPrimary: boolean;
}

export interface CompactPanelController {
  panelSide: PanelSide;
  phase: PanelPhase;
  open: () => void;
  scheduleClose: () => void;
  cancelClose: () => void;
  startDrag: () => void;
}

export function useCompactPanelController({
  corner,
  hasPrimary,
}: ControllerArgs): CompactPanelController {
  const [panelSide, setPanelSide] = useState<PanelSide>(() =>
    defaultPanelSideFromCorner(corner)
  );
  const [phase, setPhase] = useState<PanelPhase>("collapsed");
  const [pointerInside, setPointerInside] = useState(false);

  const cornerRef = useRef(corner);
  const hasPrimaryRef = useRef(hasPrimary);
  const panelSideRef = useRef(panelSide);
  const phaseRef = useRef(phase);
  const pointerInsideRef = useRef(pointerInside);
  const draggingRef = useRef(false);
  const closeAfterOpenRef = useRef(false);
  const leaveTimerRef = useRef<ReturnType<typeof window.setTimeout> | null>(
    null
  );
  const operationRef = useRef(0);
  const nativeQueueRef = useRef<Promise<void>>(Promise.resolve());

  const setPanelSideNow = useCallback((next: PanelSide) => {
    panelSideRef.current = next;
    setPanelSide(next);
  }, []);

  const setPhaseNow = useCallback((next: PanelPhase) => {
    phaseRef.current = next;
    setPhase(next);
  }, []);

  const runNative = useCallback(<T,>(op: () => Promise<T>): Promise<T> => {
    const run = nativeQueueRef.current.catch(() => {}).then(op);
    nativeQueueRef.current = run.then(
      () => undefined,
      () => undefined
    );
    return run;
  }, []);

  const cancelClose = useCallback(() => {
    pointerInsideRef.current = true;
    setPointerInside(true);
    if (leaveTimerRef.current !== null) {
      window.clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
  }, []);

  const finishCollapsed = useCallback(() => {
    flushSync(() => {
      setPanelSideNow(defaultPanelSideFromCorner(cornerRef.current));
      setPhaseNow("collapsed");
    });
  }, [setPanelSideNow, setPhaseNow]);

  const close = useCallback(
    async (skipAnimation = false) => {
      if (leaveTimerRef.current !== null) {
        window.clearTimeout(leaveTimerRef.current);
        leaveTimerRef.current = null;
      }

      const currentPhase = phaseRef.current;
      if (currentPhase === "collapsed" || currentPhase === "closing") return;
      if (currentPhase === "dragging") return;
      if (currentPhase === "opening") {
        closeAfterOpenRef.current = true;
        return;
      }

      const token = ++operationRef.current;
      const side = panelSideRef.current;
      setPhaseNow("closing");

      try {
        if (!skipAnimation) {
          await waitMs(HOVER_PANEL_EXIT_MS);
        }
        if (operationRef.current !== token) return;
        await runNative(() => collapseWindowForPanel(getCurrentWindow(), side));
      } catch {
        /* Keep UI state recoverable even if the native resize fails. */
      } finally {
        if (operationRef.current === token) {
          finishCollapsed();
        }
      }
    },
    [finishCollapsed, runNative, setPhaseNow]
  );

  const open = useCallback(() => {
    if (draggingRef.current) return;
    cancelClose();
    if (!hasPrimaryRef.current) return;
    if (phaseRef.current !== "collapsed") return;

    const token = ++operationRef.current;
    const win = getCurrentWindow();
    closeAfterOpenRef.current = false;
    setPhaseNow("opening");

    void (async () => {
      try {
        await runNative(() =>
          ensureWindowCollapsedForMeasure(win, panelSideRef.current)
        );
        if (operationRef.current !== token) return;

        const side = await getPanelSideForWindow(
          win,
          defaultPanelSideFromCorner(cornerRef.current)
        );
        if (operationRef.current !== token) return;

        flushSync(() => {
          setPanelSideNow(side);
          setPhaseNow("opening");
        });

        await runNative(() => expandWindowForPanel(win, side));
        if (operationRef.current !== token) return;

        await waitFrames(2);
        if (operationRef.current !== token) return;

        flushSync(() => {
          setPhaseNow("open");
        });

        if (
          closeAfterOpenRef.current ||
          !pointerInsideRef.current ||
          !hasPrimaryRef.current
        ) {
          void close();
        }
      } catch {
        if (operationRef.current !== token) return;
        try {
          await runNative(() =>
            ensureWindowCollapsed(getCurrentWindow(), panelSideRef.current)
          );
        } catch {
          /* ignore */
        }
        finishCollapsed();
      }
    })();
  }, [
    cancelClose,
    close,
    finishCollapsed,
    runNative,
    setPanelSideNow,
    setPhaseNow,
  ]);

  const scheduleClose = useCallback(() => {
    pointerInsideRef.current = false;
    setPointerInside(false);
    if (leaveTimerRef.current !== null) {
      window.clearTimeout(leaveTimerRef.current);
    }
    leaveTimerRef.current = window.setTimeout(() => {
      leaveTimerRef.current = null;
      if (!pointerInsideRef.current) {
        void close();
      }
    }, HOVER_LEAVE_DELAY_MS);
  }, [close]);

  const startDrag = useCallback(() => {
    if (draggingRef.current) return;

    pointerInsideRef.current = false;
    setPointerInside(false);
    if (leaveTimerRef.current !== null) {
      window.clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }

    const token = ++operationRef.current;
    const win = getCurrentWindow();
    const side = panelSideRef.current;
    draggingRef.current = true;
    closeAfterOpenRef.current = false;
    setPhaseNow("dragging");

    void (async () => {
      try {
        await runNative(() => collapseWindowForPanel(win, side));
        if (operationRef.current !== token) return;

        flushSync(() => {
          setPanelSideNow(defaultPanelSideFromCorner(cornerRef.current));
        });

        if (operationRef.current !== token) return;
        await win.startDragging();
      } catch {
        /* ignore */
      } finally {
        if (operationRef.current === token) {
          finishCollapsed();
        }
        draggingRef.current = false;
      }
    })();
  }, [finishCollapsed, runNative, setPanelSideNow, setPhaseNow]);

  useEffect(() => {
    cornerRef.current = corner;
    if (phaseRef.current === "collapsed") {
      setPanelSideNow(defaultPanelSideFromCorner(corner));
    }
  }, [corner, setPanelSideNow]);

  useEffect(() => {
    hasPrimaryRef.current = hasPrimary;
    if (!hasPrimary) {
      pointerInsideRef.current = false;
      setPointerInside(false);
      void close(true);
    }
  }, [close, hasPrimary]);

  useEffect(() => {
    if (hasPrimary && pointerInside && phase === "collapsed") {
      open();
    }
  }, [hasPrimary, open, phase, pointerInside]);

  useEffect(() => {
    const onPointerEnd = () => {
      draggingRef.current = false;
    };
    window.addEventListener("pointerup", onPointerEnd);
    window.addEventListener("pointercancel", onPointerEnd);
    return () => {
      window.removeEventListener("pointerup", onPointerEnd);
      window.removeEventListener("pointercancel", onPointerEnd);
    };
  }, []);

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current !== null) {
        window.clearTimeout(leaveTimerRef.current);
      }
    };
  }, []);

  return {
    panelSide,
    phase,
    open,
    scheduleClose,
    cancelClose,
    startDrag,
  };
}
