import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import clsx from "clsx";
import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelScheduledHoverLeave,
  scheduleHoverLeaveClear,
} from "../lib/hoverLeaveDebounce";
import {
  collapseWindowForPanel,
  defaultPanelSideFromCorner,
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
  /** Native window is expanded and it is safe to render the hover panel layout. */
  const [panelLayoutReady, setPanelLayoutReady] = useState(false);

  const pillPanelHoveredRef = useRef(pillPanelHovered);
  pillPanelHoveredRef.current = pillPanelHovered;
  const openingRef = useRef(false);

  /** Keep pill at window bottom while expanding upward (before panel mounts). */
  const [pillAnchorEnd, setPillAnchorEnd] = useState(false);

  const showPanel =
    panelLayoutReady && pillPanelHovered && primary !== undefined;
  const panelSession = showPanel ? primary : null;
  const panelAbove = panelSide === "above";
  const anchorPillToBottom =
    panelAbove && (pillAnchorEnd || panelLayoutReady || pillPanelHovered);

  useEffect(() => {
    if (!pillPanelHoveredRef.current && !panelLayoutReady) {
      setPanelSide(defaultPanelSideFromCorner(corner));
    }
  }, [corner, panelLayoutReady]);

  const openHoverPanel = useCallback(async () => {
    if (openingRef.current || pillPanelHoveredRef.current) return;
    if (primary === undefined) return;

    cancelScheduledHoverLeave();
    openingRef.current = true;

    try {
      const win = getCurrentWindow();
      const fallback = defaultPanelSideFromCorner(corner);
      const side = await getPanelSideForWindow(win, fallback);
      setPillAnchorEnd(side === "above");
      setPanelSide(side);
      await expandWindowForPanel(win, side);
      setPanelLayoutReady(true);
      setPillPanelHovered(true);
    } catch {
      setPanelLayoutReady(false);
      setPillAnchorEnd(false);
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
    if (pillPanelHovered || !panelLayoutReady) return;

    const side = panelSide;
    setPanelLayoutReady(false);

    const run = async () => {
      await collapseWindowForPanel(getCurrentWindow(), side);
      setPanelSide(defaultPanelSideFromCorner(corner));
      setPillAnchorEnd(false);
    };

    void run().catch(() => {});
  }, [pillPanelHovered, panelLayoutReady, panelSide, corner]);

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
      data-tauri-drag-region
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
