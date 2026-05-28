import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import clsx from "clsx";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import type { PointerEvent } from "react";
import { H_COLLAPSED, H_EXPANDED } from "../lib/panelPlacement";
import { useSessions } from "../store/sessions";
import type { SettingsDTO } from "../types";
import FleetBar from "./FleetBar";
import HoverPanel from "./HoverPanel";
import PrimaryLine from "./PrimaryLine";
import StatusBadge from "./StatusBadge";
import { useCompactPanelController } from "./useCompactPanelController";

const easeOut = [0.16, 1, 0.3, 1] as const;
const MOTION_FAST = { duration: 0.14, ease: easeOut } as const;
const PILL_ROW_HEIGHT = 36;

export default function CompactView() {
  const sessions = useSessions((s) => s.sessions);
  const corner = useSessions((s) => s.corner);
  const codexConnected = useSessions((s) => s.codexConnected);
  const cursorConnected = useSessions((s) => s.cursorConnected);
  const claudeConnected = useSessions((s) => s.claudeConnected);
  const hooksConnected = codexConnected || cursorConnected || claudeConnected;
  const clearTempSelect = useSessions((s) => s.clearTempSelect);
  const setCorner = useSessions((s) => s.setCorner);
  const setCodexConnected = useSessions((s) => s.setCodexConnected);
  const setCursorConnected = useSessions((s) => s.setCursorConnected);
  const setClaudeConnected = useSessions((s) => s.setClaudeConnected);

  const primary = useSessions((s) => s.primary());
  const doneQueueCount = useSessions(
    (s) =>
      s.sessions.filter((x) => x.status === "done" && !x.acknowledgedDone)
        .length
  );

  const panel = useCompactPanelController({
    corner,
    hasPrimary: primary !== undefined,
  });
  const panelSession = panel.phase === "open" ? primary ?? null : null;

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
            setClaudeConnected(settings.claudeConnected);
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
  }, [
    clearTempSelect,
    setClaudeConnected,
    setCodexConnected,
    setCorner,
    setCursorConnected,
  ]);

  const handlePillPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest("button, [role='button'], a")) return;
    panel.startDrag();
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
      onMouseEnter={panel.open}
      onMouseLeave={panel.scheduleClose}
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

  const panelSlotStyle =
    panel.panelSide === "above"
      ? { top: 0, height: H_EXPANDED - H_COLLAPSED }
      : { top: PILL_ROW_HEIGHT, height: H_EXPANDED - PILL_ROW_HEIGHT };
  const pillSlotStyle =
    panel.panelSide === "above"
      ? { bottom: 0, height: H_COLLAPSED }
      : { top: 0, height: H_COLLAPSED };

  return (
    <div className="relative h-full min-h-0 w-full">
      <div className="absolute left-0 right-0" style={panelSlotStyle}>
        <HoverPanel
          session={panelSession}
          panelSide={panel.panelSide}
          onMouseEnter={panel.cancelClose}
          onMouseLeave={panel.scheduleClose}
        />
      </div>
      <div
        className="absolute left-0 right-0 flex flex-col justify-start"
        style={pillSlotStyle}
      >
        {pillRow}
      </div>
    </div>
  );
}
