import { create } from "zustand";
import type { SessionDTO, Status } from "../types";

interface SessionsState {
  sessions: SessionDTO[];
  setSessions: (s: SessionDTO[]) => void;
  primary: () => SessionDTO | undefined;
}

const statusRank: Record<Status, number> = {
  errored: 0,
  working: 2,
  done: 1,
  idle: 3,
};

export const useSessions = create<SessionsState>((set, get) => ({
  sessions: [],
  setSessions: (s) => set({ sessions: s }),
  primary: () => {
    const s = get().sessions;
    if (s.length === 0) return undefined;
    // Priority: any errored > any unacked done > most recently active.
    const erroreds = s.filter((x) => x.status === "errored");
    if (erroreds.length) {
      return [...erroreds].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    const dones = s.filter((x) => x.status === "done" && !x.acknowledgedDone);
    if (dones.length) {
      return [...dones].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    return [...s].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
  },
}));
