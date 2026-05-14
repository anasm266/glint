import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CompactView from "./components/CompactView";
import { useSessions } from "./store/sessions";
import type { SessionDTO } from "./types";

export default function App() {
  const setSessions = useSessions((s) => s.setSessions);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    invoke<SessionDTO[]>("get_sessions").then(setSessions).catch(() => {});

    listen<SessionDTO[]>("sessions:update", (e) => {
      setSessions(e.payload);
    }).then((u) => {
      unlisten = u;
    });

    return () => {
      unlisten?.();
    };
  }, [setSessions]);

  return <CompactView />;
}
