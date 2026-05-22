import clsx from "clsx";
import type { SessionDTO } from "../types";

const appLabel: Record<SessionDTO["app"], string> = {
  codex: "Codex",
  cursor: "Cursor",
  claude: "Claude",
};

export default function PrimaryLine({
  session,
  doneQueueCount = 0,
}: {
  session: SessionDTO;
  doneQueueCount?: number;
}) {
  const action =
    session.status === "done"
      ? doneAction(session)
      : truncateAction(session.currentAction);

  const showQueue = doneQueueCount > 1;

  return (
    <div className="flex min-w-0 items-baseline gap-2">
      <span className="shrink-0 text-label text-white/55">
        {appLabel[session.app]}
        {session.project ? <span className="text-white/35"> · </span> : null}
        {session.project ? (
          <span className="text-white/75">{session.project}</span>
        ) : null}
      </span>
      <span className="flex min-w-0 items-baseline gap-1.5 truncate text-value text-white/92">
        {showQueue ? (
          <span className="shrink-0 text-dot-done">✓ {doneQueueCount} done · </span>
        ) : null}
        <span className={clsx("truncate", showQueue && "min-w-0")}>{action}</span>
      </span>
    </div>
  );
}

function truncateAction(action: string): string {
  const t = action.trim();
  if (!t) return "Thinking…";
  if (t.length <= 40) return t;
  return `${t.slice(0, 39)}…`;
}

function doneAction(s: SessionDTO): string {
  const totalAdds = s.filesEdited.reduce((acc, [, d]) => acc + d.adds, 0);
  const totalDels = s.filesEdited.reduce((acc, [, d]) => acc + d.dels, 0);
  const fileCount = s.filesEdited.length;
  if (fileCount === 0) return "Done";
  return `Done · +${totalAdds} / −${totalDels} across ${fileCount} file${
    fileCount === 1 ? "" : "s"
  }`;
}
