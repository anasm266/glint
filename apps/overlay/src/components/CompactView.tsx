import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import clsx from "clsx";
import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import {
  cancelScheduledHoverLeave,
  clearHoverState,
  markHoverOpenGrace,
  scheduleHoverLeaveClear,
  setHoverLeaveGuard,
} from "../lib/hoverLeaveDebounce";
import {
  collapseWindowForPanel,
  defaultPanelSideFromCorner,
  ensureWindowCollapsed,
  expandWindowForPanel,
  getPanelSideForWindow,
  H_COLLAPSED,
} from "../lib/panelPlacement";
import { useSessions } from "../store/sessions";
import type { PanelSide, SettingsDTO } from "../types";
import FleetBar from "./FleetBar";
import HoverPanel from "./HoverPanel";
import PrimaryLine from "./PrimaryLine";
import StatusBadge from "./StatusBadge";

const easeOut = [0.16, 1, 0.3, 1] as const;
const MOTION_FAST = { duration: 0.14, ease: easeOut } as const;

export default function CompactView() {
  const sessions = useSessions((s) => s.sessions);
  const corner = useSessions((s) => s.corner);
  const codexConnected = useSessions((s) => s.codexConnected);
  const cursorConnected = useSessions((s) => s.cursorConnected);
  const hooksConnected = codexConnected || cursorConnected;
  const pillPanelHovered = useSessions((s) => s.pillPanelHovered);
  const setPillPanelHovered = useSessions((s) => s.setPillPanelHovered);
  const clearTempSelect = useSessions((s) => s.clearTempSelect);
  const setCorner = useSessions((s) => s.setCorner);
  const setCodexConnected = useSessions((s) => s.setCodexConnected);
  const setCursorConnected = useSessions((s) => s.setCursorConnected);

  const primary = useSessions((s) => s.primary());
  const doneQueueCount = useSessions(
    (s) =>
      s.sessions.filter((x) => x.status === "done" && !x.acknowledgedDone)
        .length
  );

  const [panelSide, setPanelSide] = useState<PanelSide>(() =>
    defaultPanelSideFromCorner(corner)
  );
  const [panelLayoutReady, setPanelLayoutReady] = useState(false);

  const pillPanelHoveredRef = useRef(pillPanelHovered);
  pillPanelHoveredRef.current = pillPanelHovered;
  const panelLayoutReadyRef = useRef(panelLayoutReady);
  panelLayoutReadyRef.current = panelLayoutReady;
  const openingRef = useRef(false);
  const draggingRef = useRef(false);
  const panelSideRef = useRef(panelSide);
  panelSideRef.current = panelSide;

  const showPanel =
    panelLayoutReady && pillPanelHovered && primary !== undefined;
  const panelSession = showPanel ? primary : null;
  const panelAbove = panelLayoutReady && panelSide === "above";
  const anchorPillToBottom = panelAbove;

  useEffect(() => {
    setHoverLeaveGuard(
      () => openingRef.current || draggingRef.current
    );
    return () => setHoverLeaveGuard(null);
  }, []);

  useEffect(() => {
    if (!pillPanelHoveredRef.current && !panelLayoutReady) {
      setPanelSide(defaultPanelSideFromCorner(corner));
    }
  }, [corner, panelLayoutReady]);

  const endDrag = useCallback(() => {
    draggingRef.current = false;
  }, []);

  useEffect(() => {
    const onPointerUp = () => endDrag();
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
    return () => {
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    };
  }, [endDrag]);

  const collapsePanelWindow = useCallback(async () => {
    const side = panelSideRef.current;
    setPanelLayoutReady(false);
    clearHoverState();
    await collapseWindowForPanel(getCurrentWindow(), side);
    setPanelSide(defaultPanelSideFromCorner(corner));
  }, [corner]);

  const openHoverPanel = useCallback(async () => {
    if (openingRef.current || draggingRef.current) return;
    if (
      pillPanelHoveredRef.current &&
      panelLayoutReadyRef.current
    ) {
      return;
    }
    if (primary === undefined) return;

    cancelScheduledHoverLeave();
    openingRef.current = true;

    try {
      const win = getCurrentWindow();
      await ensureWindowCollapsed(win, panelSideRef.current);
      const fallback = defaultPanelSideFromCorner(corner);
      const side = await getPanelSideForWindow(win, fallback);
      panelSideRef.current = side;
      await expandWindowForPanel(win, side);
      setPillPanelHovered(true);
      flushSync(() => {
        setPanelSide(side);
        setPanelLayoutReady(true);
      });
      cancelScheduledHoverLeave();
      markHoverOpenGrace();
    } catch {
      setPanelLayoutReady(false);
      clearHoverState();
      try {
        await ensureWindowCollapsed(
          getCurrentWindow(),
          panelSideRef.current
        );
      } catch {
        /* ignore */
      }
    } finally {
      openingRef.current = false;
    }
  }, [corner, primary, setPillPanelHovered]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onFocusChanged(async ({ payload: focused }) => {
        if (!focused) {
          clearTempSelect();
        } else {
          try {
            const settings = await invoke<SettingsDTO>("get_settings");
            setCorner(settings.corner);
            setCodexConnected(settings.codexConnected);
            setCursorConnected(settings.cursorConnected);
          } catch {
            /* ignore */
          }
        }
      })
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});
    return () => {
      unlisten?.();
    };
  }, [clearTempSelect, setCorner, setCodexConnected, setCursorConnected]);

  useEffect(() => {
    if (
      openingRef.current ||
      pillPanelHovered ||
      !panelLayoutReady ||
      draggingRef.current
    ) {
      return;
    }

    void collapsePanelWindow().catch(() => {});
  }, [pillPanelHovered, panelLayoutReady, collapsePanelWindow]);

  const handlePillPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, [role='button'], a")) return;

    draggingRef.current = true;
    cancelScheduledHoverLeave();

    const releaseDrag = () => endDrag();

    void (async () => {
      try {
        if (panelLayoutReadyRef.current) {
          clearHoverState();
          setPanelLayoutReady(false);
          await collapseWindowForPanel(
            getCurrentWindow(),
            panelSideRef.current
          );
          setPanelSide(defaultPanelSideFromCorner(corner));
        }
        await getCurrentWindow().startDragging();
      } catch {
        /* ignore */
      } finally {
        releaseDrag();
      }
    })();
  };

  const tint =
    primary?.status === "errored"
      ? "bg-rose-500/[0.06]"
      : primary?.status === "done" && !primary.acknowledgedDone
        ? "bg-emerald-500/[0.05]"
        : "";

  const onPillClick = () => {
    clearTempSelect();
  };

  const pillRow = (
    <div
      className={clsx(
        "surface flex h-9 w-full shrink-0 items-center gap-3 px-3 transition-colors duration-220 ease-out cursor-default select-none",
        tint
      )}
      onMouseEnter={() => {
        void openHoverPanel();
      }}
      onMouseLeave={() => {
        scheduleHoverLeaveClear();
      }}
      onPointerDown={handlePillPointerDown}
      onClick={onPillClick}
    >
      <FleetBar
        sessions={sessions}
        primaryId={primary?.id}
        hooksConnected={hooksConnected}
      />
      <div className="min-w-0 flex-1">
        <AnimatePresence mode="popLayout">
          {primary ? (
            <motion.div
              key={primary.id + ":" + primary.status}
              initial={{ opacity: 0, y: 2 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -2 }}
              transition={MOTION_FAST}
              className="min-w-0"
            >
              <PrimaryLine
                session={primary}
                doneQueueCount={doneQueueCount}
              />
            </motion.div>
          ) : (
            <motion.div
              key={hooksConnected ? "empty" : "disconnected"}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={MOTION_FAST}
            >
              {hooksConnected ? (
                <span className="text-label text-white/35">
                  No active sessions
                </span>
              ) : (
                <button
                  className="flex items-center gap-1.5 text-label text-white/40 hover:text-white/65 transition-colors duration-220 ease-out"
                  onClick={(e) => {
                    e.stopPropagation();
                    invoke("open_settings").catch(() => {});
                  }}
                >
                  Connect in Settings to get started
                </button>
              )}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
      <StatusBadge session={primary} />
    </div>
  );

  /** Fixed slot matching collapsed window height — anchors above-panel without shifting the 36px pill. */
  const pillContent = anchorPillToBottom ? (
    <div
      className="flex w-full shrink-0 flex-col justify-start"
      style={{ height: H_COLLAPSED }}
    >
      {pillRow}
    </div>
  ) : (
    pillRow
  );

  return (
    <div
      className={clsx(
        "flex h-full w-full min-h-0 flex-col",
        anchorPillToBottom ? "justify-end" : "justify-start"
      )}
    >
      {panelAbove && showPanel ? (
        <>
          <HoverPanel session={panelSession} panelSide={panelSide} />
          {pillContent}
        </>
      ) : (
        <>
          {pillContent}
          {showPanel ? (
            <HoverPanel session={panelSession} panelSide={panelSide} />
          ) : null}
        </>
      )}
    </div>
  );
}
