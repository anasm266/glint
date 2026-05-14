import { invoke } from "@tauri-apps/api/core";
import clsx from "clsx";
import { motion, AnimatePresence } from "framer-motion";
import { useSessions } from "../store/sessions";
import FleetBar from "./FleetBar";
import PrimaryLine from "./PrimaryLine";
import StatusBadge from "./StatusBadge";

export default function CompactView() {
  const sessions = useSessions((s) => s.sessions);
  const primary = useSessions((s) => s.primary());

  const tint =
    primary?.status === "errored"
      ? "bg-rose-500/[0.06]"
      : primary?.status === "done" && !primary.acknowledgedDone
        ? "bg-emerald-500/[0.05]"
        : "";

  const onClick = () => {
    if (!primary) return;
    invoke("focus_session", { id: primary.id }).catch(() => {});
  };

  return (
    <div
      data-tauri-drag-region
      className={clsx(
        "surface flex h-9 w-full items-center gap-3 px-3 transition-colors duration-220 ease-out cursor-default select-none",
        tint
      )}
      onClick={onClick}
      onDoubleClick={onClick}
    >
      <FleetBar sessions={sessions} primaryId={primary?.id} />
      <div className="flex-1 min-w-0">
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
              <PrimaryLine session={primary} />
            </motion.div>
          ) : (
            <motion.div
              key="empty"
              initial={{ opacity: 0 }}
              animate={{ opacity: 0.55 }}
              exit={{ opacity: 0 }}
              className="text-label text-white/50"
            >
              No active sessions
            </motion.div>
          )}
        </AnimatePresence>
      </div>
      <StatusBadge session={primary} />
    </div>
  );
}
