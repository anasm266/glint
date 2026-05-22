import clsx from "clsx";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useState } from "react";
import type { ActivityEntryDTO, Corner, SessionDTO } from "../types";
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

const activityEnter = {
  opacity: 0,
  y: -6,
  height: 0,
};
const activityAnimate = {
  opacity: 1,
  y: 0,
  height: "auto" as const,
};
const activityExit = {
  opacity: 0,
  y: 6,
  height: 0,
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
  const setPillPanelHovered = useSessions((s) => s.setPillPanelHovered);

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

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
            <ContextRow session={session} />

            <AnimatePresence mode="popLayout" initial={false}>
              <motion.div
                key={`${session.id}:${session.status}`}
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.2, ease: easeOut }}
                className="min-h-0"
              >
                {session.status === "working" ? (
                  <WorkingResult session={session} />
                ) : session.status === "done" ? (
                  <DoneResult session={session} />
                ) : session.status === "errored" ? (
                  <ErroredResult session={session} />
                ) : (
                  <WorkingResult session={session} />
                )}
              </motion.div>
            </AnimatePresence>

            <ActionStrip session={session} now={now} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function ContextRow({ session }: { session: SessionDTO }) {
  const prompt = session.lastPrompt?.trim();
  const contextText = prompt
    ? `You asked: ${truncatePrompt(prompt, 72)}`
    : "New session";

  return (
    <div className="flex min-w-0 items-baseline justify-between gap-2">
      <p
        className="min-w-0 flex-1 truncate text-[11px] text-white/40"
        title={prompt || undefined}
      >
        {contextText}
      </p>
      <span className="shrink-0 text-[10px] text-white/28">
        <span className="text-white/45">{session.project || "—"}</span>
        <span className="text-white/20"> · </span>
        {appLabel[session.app]}
      </span>
    </div>
  );
}

function WorkingResult({ session }: { session: SessionDTO }) {
  const activity = session.recentActivity ?? [];

  return (
    <div className="flex flex-col gap-2">
      <p className="text-[14px] font-medium leading-snug text-white/90">
        {session.currentAction}
      </p>
      {activity.length > 0 ? <ActivityFeed entries={activity} /> : null}
    </div>
  );
}

function ActivityFeed({ entries }: { entries: ActivityEntryDTO[] }) {
  return (
    <div
      className="relative max-h-[72px] overflow-hidden"
      style={{
        maskImage:
          "linear-gradient(to bottom, black 0%, black 70%, transparent 100%)",
        WebkitMaskImage:
          "linear-gradient(to bottom, black 0%, black 70%, transparent 100%)",
      }}
    >
      <div className="flex flex-col gap-0.5 overflow-hidden">
        <AnimatePresence mode="popLayout" initial={false}>
          {entries.map((entry, i) => (
            <motion.div
              key={entry.seq}
              layout
              initial={activityEnter}
              animate={activityAnimate}
              exit={activityExit}
              transition={{ duration: 0.18, ease: easeOut }}
              className={clsx(
                "overflow-hidden text-[11px] leading-snug",
                i === 0 ? "text-white/70" : "text-white/38"
              )}
              title={entry.summary}
            >
              {entry.summary}
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

function DoneResult({ session }: { session: SessionDTO }) {
  const files = session.filesEdited;

  if (files.length === 0) {
    return (
      <p className="text-[14px] font-medium leading-snug text-white/90">
        No files changed
      </p>
    );
  }

  const { adds, dels } = fileDiffTotals(files);
  const n = files.length;

  return (
    <div className="flex min-h-0 flex-col gap-2">
      <p className="text-[14px] font-medium leading-snug text-white/90 tnum">
        +{adds} / −{dels} across {n} file{n === 1 ? "" : "s"}
      </p>
      <motion.ul
        variants={listContainerVariants}
        initial="hidden"
        animate="show"
        className="scrollbar-none flex max-h-[88px] flex-col gap-0.5 overflow-y-auto"
      >
        {files.map(([path, diff]) => (
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
    </div>
  );
}

function ErroredResult({ session }: { session: SessionDTO }) {
  return (
    <p className="text-[14px] font-medium leading-snug text-rose-300/80">
      {session.currentAction || "Something went wrong"}
    </p>
  );
}

function ActionStrip({ session, now }: { session: SessionDTO; now: number }) {
  const showDismiss =
    session.status === "done" && !session.acknowledgedDone;
  const hasFiles = session.filesEdited.length > 0;
  const isWorking = session.status === "working";
  const isErrored = session.status === "errored";
  const isDoneEmpty = session.status === "done" && !hasFiles;

  const openCodex = (e: React.MouseEvent) => {
    e.stopPropagation();
    invoke("open_codex", { id: session.id }).catch(() => {});
  };

  const dismiss = (e: React.MouseEvent) => {
    e.stopPropagation();
    invoke("acknowledge_done", { id: session.id }).catch(() => {});
  };

  const primaryBtn =
    "rounded-md bg-white/10 px-3 py-1.5 text-[12px] font-medium text-white/90 transition-colors duration-150 ease-out hover:bg-white/15";
  const ghostBtn =
    "rounded-md px-3 py-1.5 text-[12px] text-white/50 transition-colors duration-150 ease-out hover:bg-white/[0.06] hover:text-white/80";

  return (
    <div className="flex items-center justify-between gap-2 border-t border-white/[0.06] pt-2">
      <span
        className="text-[11px] tnum text-white/38"
        title={absoluteTimeTitle(session)}
      >
        {elapsedLabel(session, now)}
      </span>
      <div className="flex items-center gap-1.5">
        {isWorking ? (
          <button type="button" className={ghostBtn} onClick={openCodex}>
            Open Codex
          </button>
        ) : null}

        {session.status === "done" && hasFiles ? (
          <>
            {showDismiss ? (
              <button type="button" className={ghostBtn} onClick={dismiss}>
                Dismiss
              </button>
            ) : null}
            <button type="button" className={primaryBtn} onClick={openCodex}>
              Open Codex
            </button>
          </>
        ) : null}

        {isDoneEmpty ? (
          <>
            <button type="button" className={ghostBtn} onClick={openCodex}>
              Open Codex
            </button>
            {showDismiss ? (
              <button type="button" className={primaryBtn} onClick={dismiss}>
                Dismiss
              </button>
            ) : null}
          </>
        ) : null}

        {isErrored ? (
          <button type="button" className={primaryBtn} onClick={openCodex}>
            Open Codex
          </button>
        ) : null}
      </div>
    </div>
  );
}

function truncatePrompt(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + "…";
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

function absoluteTimeTitle(s: SessionDTO): string {
  const d =
    s.status === "done"
      ? new Date(s.lastEventAtMs)
      : new Date(s.startedAtMs);
  const kind = s.status === "done" ? "Finished" : "Started";
  return `${kind} ${d.toLocaleString()}`;
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
