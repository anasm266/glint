import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import type { Corner, SessionDTO } from "../types";

/** Background cleanup after ack if remove_session fails (user dismiss is immediate). */
const REMOVAL_DELAY_MS = 500;

const removalTimers = new Map<string, ReturnType<typeof setTimeout>>();
/** Optimistic dismiss — hide until backend snapshot confirms removal. */
const dismissedIds = new Set<string>();

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

function filterSessions(sessions: SessionDTO[]): SessionDTO[] {
  return sessions.filter((s) => !dismissedIds.has(s.id));
}

interface SessionsState {
  sessions: SessionDTO[];
  corner: Corner;
  codexConnected: boolean;
  cursorConnected: boolean;
  claudeConnected: boolean;
  tempSelectedId: string | null;
  setSessions: (s: SessionDTO[]) => void;
  setCorner: (c: Corner) => void;
  setCodexConnected: (v: boolean) => void;
  setCursorConnected: (v: boolean) => void;
  setClaudeConnected: (v: boolean) => void;
  tempSelect: (id: string) => void;
  clearTempSelect: () => void;
  /** Remove a session from the overlay (instant UI, then sync backend). */
  untrackSession: (id: string) => void;
  primary: () => SessionDTO | undefined;
  doneQueueCount: () => number;
}

export const useSessions = create<SessionsState>((set, get) => ({
  sessions: [],
  corner: "tr",
  codexConnected: false,
  cursorConnected: false,
  claudeConnected: false,
  tempSelectedId: null,

  setCorner: (corner) => set({ corner }),
  setCodexConnected: (codexConnected) => set({ codexConnected }),
  setCursorConnected: (cursorConnected) => set({ cursorConnected }),
  setClaudeConnected: (claudeConnected) => set({ claudeConnected }),

  setSessions: (next) => {
    const prev = get().sessions;
    const prevTemp = get().tempSelectedId;
    const nextIds = new Set(next.map((x) => x.id));

    for (const id of [...dismissedIds]) {
      if (!nextIds.has(id)) dismissedIds.delete(id);
    }

    const filtered = filterSessions(next);

    for (const s of filtered) {
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

    for (const id of [...removalTimers.keys()]) {
      if (!nextIds.has(id)) {
        clearRemovalTimer(id);
      }
    }

    let tempSelectedId = prevTemp;
    if (tempSelectedId && !filtered.some((x) => x.id === tempSelectedId)) {
      tempSelectedId = null;
    }

    set({ sessions: filtered, tempSelectedId });
  },

  tempSelect: (id) => set({ tempSelectedId: id }),

  clearTempSelect: () => set({ tempSelectedId: null }),

  untrackSession: (id) => {
    dismissedIds.add(id);
    clearRemovalTimer(id);
    const { sessions, tempSelectedId } = get();
    const removed = sessions.find((s) => s.id === id);
    const remaining = sessions.filter((s) => s.id !== id);
    set({
      sessions: remaining,
      tempSelectedId: tempSelectedId === id ? null : tempSelectedId,
    });
    if (
      removed?.status === "done" &&
      !removed.acknowledgedDone
    ) {
      invoke("acknowledge_done", { id }).catch(() => {});
    }
    invoke("remove_session", { id }).catch(() => {});
  },

  primary: () => {
    const { sessions, tempSelectedId } = get();
    const visible = filterSessions(sessions);

    if (visible.length === 0) return undefined;

    const byId = (id: string | null | undefined) =>
      id ? visible.find((x) => x.id === id) : undefined;
    if (tempSelectedId) {
      const t = byId(tempSelectedId);
      if (t) return t;
    }

    const erroreds = visible.filter((x) => x.status === "errored");
    if (erroreds.length) {
      return [...erroreds].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    const dones = visible.filter(
      (x) => x.status === "done" && !x.acknowledgedDone
    );
    if (dones.length) {
      return [...dones].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
    }
    const active = visible.filter(
      (x) => !(x.status === "done" && x.acknowledgedDone)
    );
    if (active.length === 0) return undefined;
    return [...active].sort((a, b) => b.lastEventAtMs - a.lastEventAtMs)[0];
  },

  doneQueueCount: () =>
    filterSessions(get().sessions).filter(
      (x) => x.status === "done" && !x.acknowledgedDone
    ).length,
}));
