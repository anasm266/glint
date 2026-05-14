export type App = "codex" | "cursor" | "claude";

export type Status = "idle" | "working" | "done" | "errored";

export interface DiffStat {
  adds: number;
  dels: number;
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
  filesEdited: Array<[string, DiffStat]>;
}

export type Corner = "tl" | "tr" | "bl" | "br";

export interface SettingsDTO {
  corner: Corner;
  opacity: number;
  codexConnected: boolean;
}
