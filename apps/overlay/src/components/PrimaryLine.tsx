import type { SessionDTO } from "../types";

const appLabel: Record<SessionDTO["app"], string> = {
  codex: "Codex",
  cursor: "Cursor",
  claude: "Claude",
};

export default function PrimaryLine({ session }: { session: SessionDTO }) {
  const action =
    session.status === "done" ? doneAction(session) : session.currentAction;

  return (
    <div className="flex items-baseline gap-2 min-w-0">
      <span className="shrink-0 text-label text-white/55">
        {appLabel[session.app]}
        {session.project ? <span className="text-white/35"> · </span> : null}
        {session.project ? (
          <span className="text-white/75">{session.project}</span>
        ) : null}
      </span>
      <span className="truncate text-value text-white/92">{action}</span>
    </div>
  );
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
