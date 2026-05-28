import clsx from "clsx";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import type { SessionDTO, Status } from "../types";
import { useSessions } from "../store/sessions";

const dotColor: Record<Status, string> = {
  idle: "bg-dot-idle",
  working: "bg-dot-work",
  done: "bg-dot-done",
  errored: "bg-dot-err",
};

const easeOut = [0.16, 1, 0.3, 1] as const;
const MOTION_FAST = { duration: 0.14, ease: easeOut } as const;

export default function FleetBar({
  sessions,
  primaryId,
  hooksConnected,
}: {
  sessions: SessionDTO[];
  primaryId?: string;
  /** True when Codex or Cursor hooks are installed. */
  hooksConnected?: boolean;
}) {
  const tempSelectedId = useSessions((s) => s.tempSelectedId);
  const tempSelect = useSessions((s) => s.tempSelect);

  const visible = sessions.slice(0, 8);
  const overflow = Math.max(0, sessions.length - visible.length);

  const ringFor = (id: string) => {
    if (tempSelectedId === id) {
      return "ring-2 ring-white/55 ring-offset-0";
    }
    if (!tempSelectedId && id === primaryId) {
      return "ring-1 ring-white/30 ring-offset-0";
    }
    return "";
  };

  return (
    <div className="flex items-center gap-1 shrink-0">
      <AnimatePresence initial={false}>
        {visible.map((s) => (
          <motion.span
            key={s.id}
            layout
            role="button"
            tabIndex={0}
            initial={{ scale: 0.5, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.5, opacity: 0 }}
            transition={MOTION_FAST}
            className={clsx(
              "block h-1.5 w-1.5 rounded-full cursor-default",
              dotColor[s.status],
              s.status === "working" && "animate-breathe",
              ringFor(s.id)
            )}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              tempSelect(s.id);
            }}
          />
        ))}
      </AnimatePresence>
      {overflow > 0 && (
        <span className="ml-1 text-[10px] leading-none text-white/45 tnum">
          +{overflow}
        </span>
      )}
      <AnimatePresence initial={false}>
        {!hooksConnected && (
          <motion.span
            key="disconnected"
            layout
            initial={{ scale: 0.5, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.5, opacity: 0 }}
            transition={MOTION_FAST}
            title="Hooks not installed — click to open Settings"
            className={clsx(
              "block h-1.5 w-1.5 shrink-0 rounded-full",
              "ring-1 ring-amber-400/55",
              sessions.length > 0 && "ml-0.5"
            )}
            onMouseDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              invoke("open_settings").catch(() => {});
            }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}
