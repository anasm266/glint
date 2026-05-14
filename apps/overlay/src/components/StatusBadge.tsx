import { useEffect, useState } from "react";
import type { SessionDTO } from "../types";

export default function StatusBadge({ session }: { session?: SessionDTO }) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  if (!session) {
    return <span className="text-label text-white/35 tnum shrink-0">—</span>;
  }

  // For done state we show "just now" briefly, then elapsed time since done.
  if (session.status === "done") {
    const ago = Math.max(0, now - session.lastEventAtMs);
    return (
      <span className="text-label text-emerald-300/85 tnum shrink-0">
        {ago < 5000 ? "just now" : humanAgo(ago)}
      </span>
    );
  }

  const elapsed = Math.max(0, now - session.startedAtMs);
  return (
    <span className="text-label text-white/55 tnum shrink-0">
      {humanElapsed(elapsed)}
    </span>
  );
}

function humanElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  if (m < 60) return `${m}m ${rem.toString().padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${(m % 60).toString().padStart(2, "0")}m`;
}

function humanAgo(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  return `${h}h ago`;
}
