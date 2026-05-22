import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { Corner, SessionDTO } from "../types";

const REMOVAL_DELAY_MS = 7000;

const removalTimers = new Map<string, ReturnType<typeof setTimeout>>();

function clearRemovalTimer(id: string) {
  const t = removalTimers.get(id);
  if (t !== undefined) {
    clearTimeout(t);
    removalTimers.delete(id);
  }
}

function scheduleRemovalAfterAck(id: string) {
  if (removalTimers.has(id)) return;
  const t = setTimeout(() => {
    removalTimers.delete(id);
    invoke("remove_session", { id }).catch(() => {});
  }, REMOVAL_DELAY_MS);
  removalTimers.set(id, t);
}

interface SessionsState {
  sessions: SessionDTO[];
  corner: Corner;
  codexConnected: boolean;
  cursorConnected: boolean;
  tempSelectedId: string | null;
  /** True while pointer is over the pill row or hover panel (debounced leave). */
  pillPanelHovered: boolean;
  setSessions: (s: SessionDTO[]) => void;
  setCorner: (c: Corner) => void;
  setCodexConnected: (v: boolean) => void;
  setCursorConnected: (v: boolean) => void;
  tempSelect: (id: string) => void;
  clearTempSelect: () => void;
  setPillPanelHovered: (v: boolean) => void;
  primary: () => SessionDTO | undefined;
  doneQueueCount: () => number;
}

export const useSessions = create<SessionsState>((set, get) => ({
  sessions: [],
  corner: "tr",
  codexConnected: false,
  cursorConnected: false,
  tempSelectedId: null,
  pillPanelHovered: false,

  setCorner: (corner) => set({ corner }),
  setCodexConnected: (codexConnected) => set({ codexConnected }),
  setCursorConnected: (cursorConnected) => set({ cursorConnected }),

  setSessions: (next) => {
    const prev = get().sessions;
    const prevTemp = get().tempSelectedId;

    for (const s of next) {
      const was = prev.find((x) => x.id === s.id);
      if (
        s.status === "done" &&
        s.acknowledgedDone &&
        was &&
        !was.acknowledgedDone
      ) {
        scheduleRemovalAfterAck(s.id);
      }
    }

    const nextIds = new Set(next.map((x) => x.id));
    for (const id of [...removalTimers.keys()]) {
      if (!nextIds.has(id)) {
        clearRemovalTimer(id);
      }
    }

    let tempSelectedId = prevTemp;
    if (tempSelectedId && !nextIds.has(tempSelectedId)) tempSelectedId = null;

    set({ sessions: next, tempSelectedId });
  },

  tempSelect: (id) => set({ tempSelectedId: id }),

  clearTempSelect: () => set({ tempSelectedId: null }),

  setPillPanelHovered: (pillPanelHovered) => set({ pillPanelHovered }),

  primary: () => {
    const { sessions, tempSelectedId } = get();
    if (sessions.length === 0) return undefined;

    const byId = (id: string | null | undefined) =>
      id ? sessions.find((x) => x.id === id) : undefined;

    if (tempSelectedId) {
      const t = byId(tempSelectedId);
      if (t) return t;
    }

    const erroreds = sessions.filter((x) => x.status === "errored");
    if (erroreds.length) {
      return [...erroreds].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    const dones = sessions.filter(
      (x) => x.status === "done" && !x.acknowledgedDone
    );
    if (dones.length) {
      return [...dones].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    return [...sessions].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
  },

  doneQueueCount: () =>
    get().sessions.filter((x) => x.status === "done" && !x.acknowledgedDone)
      .length,
}));
