export type App = "codex" | "cursor" | "claude";

export type Status = "idle" | "working" | "done" | "errored";

export interface DiffStat {
  adds: number;
  dels: number;
}

export interface ActivityEntryDTO {
  seq: number;
  atMs: number;
  summary: string;
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
}

export type Corner = "tl" | "tr" | "bl" | "br";

export interface SettingsDTO {
  corner: Corner;
  opacity: number;
  codexConnected: boolean;
}
