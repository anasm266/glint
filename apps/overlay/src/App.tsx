import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CompactView from "./components/CompactView";
import { useSessions } from "./store/sessions";
import type { SessionDTO, SettingsDTO } from "./types";

export default function App() {
  const setSessions = useSessions((s) => s.setSessions);
  const setCorner = useSessions((s) => s.setCorner);
  const setCodexConnected = useSessions((s) => s.setCodexConnected);
  const setCursorConnected = useSessions((s) => s.setCursorConnected);
  const setClaudeConnected = useSessions((s) => s.setClaudeConnected);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    invoke<SessionDTO[]>("get_sessions").then(setSessions).catch(() => {});
    invoke<SettingsDTO>("get_settings")
      .then((st) => {
        setCorner(st.corner);
        setCodexConnected(st.codexConnected);
        setCursorConnected(st.cursorConnected);
        setClaudeConnected(st.claudeConnected);
      })
      .catch(() => {});

    listen<SessionDTO[]>("sessions:update", (e) => {
      setSessions(e.payload);
    }).then((u) => {
      unlisten = u;
    });

    return () => {
      unlisten?.();
    };
  }, [setSessions, setCorner, setCodexConnected, setCursorConnected, setClaudeConnected]);

  return <CompactView />;
}
