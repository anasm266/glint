import { invoke } from "@tauri-apps/api/core";
import {
  getCurrentWindow,
  LogicalSize,
  PhysicalPosition,
} from "@tauri-apps/api/window";
import clsx from "clsx";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import {
  cancelScheduledHoverLeave,
  scheduleHoverLeaveClear,
} from "../lib/hoverLeaveDebounce";
import { useSessions } from "../store/sessions";
import type { SettingsDTO } from "../types";
import FleetBar from "./FleetBar";
import HoverPanel from "./HoverPanel";
import PrimaryLine from "./PrimaryLine";
import StatusBadge from "./StatusBadge";

const WIN_W = 380;
// 8px extra below the pill gives the box-shadow room to fade before hitting
// the WebView boundary, eliminating the rectangular halo on light backgrounds.
const H_COLLAPSED = 60;
/** Total window height when hover panel is open — must fit pill + card + shadow buffer. */
const H_EXPANDED = 300;

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

  const panelSession =
    pillPanelHovered && primary !== undefined ? primary : null;

  const bottom = corner === "bl" || corner === "br";

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
    const expanded = pillPanelHovered && primary !== undefined;
    const bottomCorner = corner === "bl" || corner === "br";
    const dyLogical = H_EXPANDED - H_COLLAPSED;

    const run = async () => {
      const win = getCurrentWindow();
      if (expanded) {
        if (bottomCorner) {
          const pos = await win.outerPosition();
          const scale = await win.scaleFactor();
          const dy = Math.round(dyLogical * scale);
          await win.setPosition(new PhysicalPosition(pos.x, pos.y - dy));
        }
        await win.setSize(new LogicalSize(WIN_W, H_EXPANDED));
      } else {
        await win.setSize(new LogicalSize(WIN_W, H_COLLAPSED));
        if (bottomCorner) {
          const pos = await win.outerPosition();
          const scale = await win.scaleFactor();
          const dy = Math.round(dyLogical * scale);
          await win.setPosition(new PhysicalPosition(pos.x, pos.y + dy));
        }
      }
    };

    void run().catch(() => {});
  }, [pillPanelHovered, primary?.id, corner]);

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
        cancelScheduledHoverLeave();
        setPillPanelHovered(true);
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
              transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
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
              transition={{ duration: 0.22 }}
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

  return (
    <div
      className={clsx(
        "flex h-full w-full min-h-0 flex-col",
        bottom ? "justify-end" : "justify-start"
      )}
    >
      {bottom ? (
        <>
          <HoverPanel session={panelSession} corner={corner} />
          {pillRow}
        </>
      ) : (
        <>
          {pillRow}
          <HoverPanel session={panelSession} corner={corner} />
        </>
      )}
    </div>
  );
}
