export type App = "codex" | "cursor" | "claude";

export type Status = "idle" | "working" | "done" | "errored";

export interface DiffStat {
  adds: number;
  dels: number;
}

export type ActivityKind = "normal" | "success" | "failure";

export interface ActivityEntryDTO {
  seq: number;
  atMs: number;
  summary: string;
  count: number;
  kind: ActivityKind;
}

export interface SessionDTO {
  id: string;
  app: App;
  project: string;
  cwd: string;
  status: Status;
  currentAction: string;
  startedAtMs: number;
  lastEventAtMs: number;
  acknowledgedDone: boolean;
  lastPrompt: string;
  filesEdited: Array<[string, DiffStat]>;
  recentActivity: ActivityEntryDTO[];
  model: string;
  lastCommitHash: string | null;
  doneSummary: string | null;
}

export type Corner = "tl" | "tr" | "bl" | "br";

export interface SettingsDTO {
  corner: Corner;
  opacity: number;
  codexConnected: boolean;
}
