import clsx from "clsx";
import { motion, AnimatePresence } from "framer-motion";
import type { SessionDTO, Status } from "../types";

const dotColor: Record<Status, string> = {
  idle: "bg-dot-idle",
  working: "bg-dot-work",
  done: "bg-dot-done",
  errored: "bg-dot-err",
};

export default function FleetBar({
  sessions,
  primaryId,
}: {
  sessions: SessionDTO[];
  primaryId?: string;
}) {
  const visible = sessions.slice(0, 8);
  const overflow = Math.max(0, sessions.length - visible.length);

  return (
    <div className="flex items-center gap-1 shrink-0">
      <AnimatePresence initial={false}>
        {visible.map((s) => (
          <motion.span
            key={s.id}
            layout
            initial={{ scale: 0.4, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.4, opacity: 0 }}
            transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
            className={clsx(
              "block h-1.5 w-1.5 rounded-full",
              dotColor[s.status],
              s.status === "working" && "animate-breathe",
              s.id === primaryId && "ring-1 ring-white/30 ring-offset-0"
            )}
          />
        ))}
      </AnimatePresence>
      {overflow > 0 && (
        <span className="ml-1 text-[10px] leading-none text-white/45 tnum">
          +{overflow}
        </span>
      )}
    </div>
  );
}
