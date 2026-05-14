import clsx from "clsx";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import type { Corner, SessionDTO } from "../types";
import {
  cancelScheduledHoverLeave,
  scheduleHoverLeaveClear,
} from "../lib/hoverLeaveDebounce";
import { useSessions } from "../store/sessions";

const appLabel: Record<SessionDTO["app"], string> = {
  codex: "Codex",
  cursor: "Cursor",
  claude: "Claude",
};

const easeOut = [0.16, 1, 0.3, 1] as const;

const listContainerVariants = {
  hidden: {},
  show: {
    transition: {
      staggerChildren: 0.03,
      delayChildren: 0.04,
    },
  },
};

const listItemVariants = {
  hidden: { opacity: 0, x: -5 },
  show: {
    opacity: 1,
    x: 0,
    transition: { duration: 0.16, ease: easeOut },
  },
};

export default function HoverPanel({
  session,
  corner,
}: {
  session: SessionDTO | null;
  corner: Corner;
}) {
  const bottom = corner === "bl" || corner === "br";
  const [now, setNow] = useState(() => Date.now());
  const [clockMode, setClockMode] = useState(false);
  const setPillPanelHovered = useSessions((s) => s.setPillPanelHovered);

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  useEffect(() => {
    setClockMode(false);
  }, [session?.id]);

  return (
    <div
      className={clsx(
        "flex min-h-0 flex-1 flex-col px-1.5 pb-1.5 pt-1",
        session ? "pointer-events-auto" : "pointer-events-none"
      )}
      aria-hidden={!session}
    >
      <AnimatePresence initial={false}>
        {session ? (
          <motion.div
            key={session.id}
            initial={{ opacity: 0, y: bottom ? 10 : -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: bottom ? 10 : -10 }}
            transition={{ duration: 0.22, ease: easeOut }}
            className={clsx(
              "surface flex flex-col gap-2.5 overflow-hidden rounded-surface px-3 py-3",
              bottom && "mt-auto"
            )}
            onMouseEnter={() => {
              cancelScheduledHoverLeave();
              setPillPanelHovered(true);
            }}
            onMouseLeave={() => {
              scheduleHoverLeaveClear();
            }}
          >
            <div className="flex items-center justify-between gap-2">
              <div className="text-[11px] text-white/50">
                <span className="font-medium text-white/80">
                  {session.project || "—"}
                </span>
                <span className="text-white/30"> · </span>
                <span>{appLabel[session.app]}</span>
              </div>
              <div
                className={clsx(
                  "h-1.5 w-1.5 shrink-0 rounded-full",
                  session.status === "working" && "bg-dot-work animate-breathe",
                  session.status === "done" && "bg-dot-done",
                  session.status === "errored" && "bg-dot-err",
                  session.status === "idle" && "bg-dot-idle"
                )}
              />
            </div>

            <div className="flex flex-wrap items-center justify-end gap-2">
              <button
                type="button"
                className="rounded px-2 py-0.5 text-[11px] text-white/50 transition-colors duration-150 ease-out hover:bg-white/[0.06] hover:text-white/80"
                title="Bring the Codex app window to the front"
                onClick={(e) => {
                  e.stopPropagation();
                  invoke("open_codex", { id: session.id }).catch(() => {});
                }}
              >
                Open Codex
              </button>
              {session.status === "done" && !session.acknowledgedDone ? (
                <button
                  type="button"
                  className="rounded px-2 py-0.5 text-[11px] text-emerald-400/70 transition-colors duration-150 ease-out hover:bg-emerald-500/10 hover:text-emerald-300/90"
                  title="Acknowledge without switching apps"
                  onClick={(e) => {
                    e.stopPropagation();
                    invoke("acknowledge_done", { id: session.id }).catch(
                      () => {}
                    );
                  }}
                >
                  Dismiss
                </button>
              ) : null}
            </div>

            <AnimatePresence initial={false}>
              {session.lastPrompt ? (
                <motion.div
                  key="prompt"
                  initial={{ opacity: 0, height: 0, marginTop: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.2, ease: easeOut }}
                  className="overflow-hidden"
                >
                  <p
                    className="line-clamp-2 text-[11px] leading-snug text-white/55 italic"
                    title={session.lastPrompt}
                  >
                    {session.lastPrompt}
                  </p>
                </motion.div>
              ) : null}
            </AnimatePresence>

            <AnimatePresence mode="popLayout" initial={false}>
              <motion.div
                key={`${session.id}:${session.status}`}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.2, ease: easeOut }}
                className="min-h-0"
              >
                {session.status === "done" ? (
                  <DoneBody session={session} />
                ) : (
                  <WorkingBody session={session} />
                )}
              </motion.div>
            </AnimatePresence>

            <button
              type="button"
              className="mt-0.5 border-t border-white/[0.06] pt-2.5 text-left text-[11px] tnum text-white/38 transition-colors duration-150 ease-out hover:text-white/60"
              title={clockMode ? "Show elapsed time" : "Show clock time"}
              onClick={(e) => {
                e.stopPropagation();
                setClockMode((m) => !m);
              }}
            >
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={clockMode ? "clock" : "elapsed"}
                  initial={{ opacity: 0, y: clockMode ? -5 : 5 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: clockMode ? 5 : -5 }}
                  transition={{ duration: 0.18, ease: easeOut }}
                  className="block"
                >
                  {clockMode ? clockLabel(session) : elapsedLabel(session, now)}
                </motion.span>
              </AnimatePresence>
            </button>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function WorkingBody({ session }: { session: SessionDTO }) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="text-[12px] leading-snug text-white/85">
        {session.currentAction}
      </div>
      <AnimatePresence initial={false}>
        {session.filesEdited.length > 0 ? (
          <motion.div
            key="touched"
            initial={{ opacity: 0, y: 3 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 3 }}
            transition={{ duration: 0.18, ease: easeOut }}
            className="text-[11px] text-white/40"
          >
            {session.filesEdited.length} file
            {session.filesEdited.length === 1 ? "" : "s"} touched so far
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function DoneBody({ session }: { session: SessionDTO }) {
  if (session.filesEdited.length === 0) {
    return (
      <div className="text-[12px] leading-snug text-white/50">
        Done — no files changed
      </div>
    );
  }

  const { adds, dels } = fileDiffTotals(session.filesEdited);
  const n = session.filesEdited.length;

  return (
    <div className="flex min-h-0 flex-col gap-1.5">
      <motion.ul
        variants={listContainerVariants}
        initial="hidden"
        animate="show"
        className="scrollbar-none flex max-h-[76px] flex-col gap-0.5 overflow-y-auto"
      >
        {session.filesEdited.map(([path, diff]) => (
          <motion.li
            key={path}
            variants={listItemVariants}
            className="flex items-baseline justify-between gap-2 text-[11px]"
            title={path}
          >
            <span className="min-w-0 truncate text-white/70">
              {fileBasename(path)}
            </span>
            <span className="shrink-0 tnum text-white/40">
              +{diff.adds} / −{diff.dels}
            </span>
          </motion.li>
        ))}
      </motion.ul>
      <div className="border-t border-white/[0.06] pt-1.5 text-[10px] tnum text-white/32">
        +{adds} / −{dels} · {n} file{n === 1 ? "" : "s"}
      </div>
    </div>
  );
}

function fileBasename(p: string): string {
  const norm = p.replace(/\\/g, "/");
  const i = norm.lastIndexOf("/");
  return i >= 0 ? norm.slice(i + 1) : norm;
}

function fileDiffTotals(files: SessionDTO["filesEdited"]): {
  adds: number;
  dels: number;
} {
  let adds = 0;
  let dels = 0;
  for (const [, d] of files) {
    adds += d.adds;
    dels += d.dels;
  }
  return { adds, dels };
}

function elapsedLabel(s: SessionDTO, now: number): string {
  if (s.status === "done") {
    const ago = Math.max(0, now - s.lastEventAtMs);
    return ago < 5000 ? "Finished just now" : `Finished ${humanAgo(ago)}`;
  }
  const elapsed = Math.max(0, now - s.startedAtMs);
  return `Running ${humanElapsed(elapsed)}`;
}

function clockLabel(s: SessionDTO): string {
  if (s.status === "done") {
    return formatClockLine(new Date(s.lastEventAtMs), "finished");
  }
  return formatClockLine(new Date(s.startedAtMs), "started");
}

function formatClockLine(d: Date, kind: "started" | "finished"): string {
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const yest = new Date(now);
  yest.setDate(yest.getDate() - 1);
  const isYesterday = d.toDateString() === yest.toDateString();
  const time = d.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

  if (kind === "finished") {
    if (sameDay) return `Finished at ${time}`;
    if (isYesterday) return `Finished yesterday ${time}`;
    const datePart = d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
    return `Finished ${datePart} ${time}`;
  }

  if (sameDay) return `Started ${time}`;
  if (isYesterday) return `Started yesterday ${time}`;
  const datePart = d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
  return `Started ${datePart} ${time}`;
}

function humanElapsed(ms: number): string {
  const sec = Math.floor(ms / 1000);
  if (sec < 60) return `${sec}s`;
  const m = Math.floor(sec / 60);
  const rem = sec % 60;
  if (m < 60) return `${m}m ${rem.toString().padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${(m % 60).toString().padStart(2, "0")}m`;
}

function humanAgo(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  return `${h}h ago`;
}
