import { truncateProject } from "../lib/truncate";
import type { SessionDTO } from "../types";

const appLabel: Record<SessionDTO["app"], string> = {
  codex: "Codex",
  cursor: "Cursor",
  claude: "Claude",
};

const PROJECT_MAX_CHARS = 16;

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
  const project = session.project?.trim() ?? "";
  const projectShort = project ? truncateProject(project, PROJECT_MAX_CHARS) : "";

  return (
    <div className="flex min-w-0 items-baseline gap-1 overflow-hidden">
      <span className="shrink-0 text-label text-white/55">{appLabel[session.app]}</span>
      {project ? (
        <>
          <span className="shrink-0 text-label text-white/30">·</span>
          <span
            className="min-w-0 max-w-[5.25rem] shrink truncate text-label text-white/75"
            title={project}
          >
            {projectShort}
          </span>
        </>
      ) : null}
      <span className="flex min-w-0 flex-1 items-baseline gap-1 overflow-hidden text-value text-white/92">
        {showQueue ? (
          <span className="shrink-0 text-dot-done">✓ {doneQueueCount} done · </span>
        ) : null}
        <span className="min-w-0 truncate">{action}</span>
      </span>
    </div>
  );
}

function truncateAction(action: string): string {
  const t = action.trim();
  if (!t) return "Thinking…";
  if (t.length <= 36) return t;
  return `${t.slice(0, 35)}…`;
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
