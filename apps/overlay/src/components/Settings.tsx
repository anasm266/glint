import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import clsx from "clsx";
import type { Corner, SettingsDTO } from "../types";

type ConnectionId = "codex" | "cursor" | "claude";

const connectionMeta: Record<
  ConnectionId,
  {
    label: string;
    target: string;
    detail: string;
    installCommand: string;
    removeCommand: string;
  }
> = {
  codex: {
    label: "Codex",
    target: "~/.codex/config.toml",
    detail: "Restart Codex Desktop after enabling.",
    installCommand: "install_codex_hooks",
    removeCommand: "remove_codex_hooks",
  },
  cursor: {
    label: "Cursor",
    target: "~/.cursor/hooks.json",
    detail: "Restart Cursor after enabling.",
    installCommand: "install_cursor_hooks",
    removeCommand: "remove_cursor_hooks",
  },
  claude: {
    label: "Claude",
    target: "~/.claude/settings.json",
    detail: "Restart Claude Code after enabling.",
    installCommand: "install_claude_hooks",
    removeCommand: "remove_claude_hooks",
  },
};

export default function Settings() {
  const [settings, setSettings] = useState<SettingsDTO | null>(null);
  const [busyConnection, setBusyConnection] = useState<ConnectionId | null>(
    null
  );
  const [savingControl, setSavingControl] = useState<"position" | "opacity" | null>(
    null
  );
  const [error, setError] = useState<string | null>(null);

  const refreshSettings = async () => {
    const fresh = await invoke<SettingsDTO>("get_settings");
    setSettings(fresh);
  };

  useEffect(() => {
    refreshSettings().catch((e) => setError(errorMessage(e)));
  }, []);

  const connectedCount = useMemo(() => {
    if (!settings) return 0;
    return [
      settings.codexConnected,
      settings.cursorConnected,
      settings.claudeConnected,
    ].filter(Boolean).length;
  }, [settings]);

  if (!settings) {
    return (
      <main className="flex h-full w-full items-center justify-center bg-[#101116] text-white/70">
        <div className="flex flex-col items-center gap-3">
          <div className="h-5 w-5 animate-spin rounded-full border border-white/10 border-t-white/70" />
          <div className="text-[12px]">Loading settings</div>
          {error ? <div className="max-w-80 text-center text-[11px] text-rose-300">{error}</div> : null}
        </div>
      </main>
    );
  }

  const connections = [
    {
      id: "codex" as const,
      connected: settings.codexConnected,
    },
    {
      id: "cursor" as const,
      connected: settings.cursorConnected,
    },
    {
      id: "claude" as const,
      connected: settings.claudeConnected,
    },
  ];

  const setConnection = async (id: ConnectionId, next: boolean) => {
    const meta = connectionMeta[id];
    setBusyConnection(id);
    setError(null);
    try {
      await invoke(next ? meta.installCommand : meta.removeCommand);
      await refreshSettings();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusyConnection(null);
    }
  };

  const setCorner = async (corner: Corner) => {
    setSavingControl("position");
    setError(null);
    try {
      await invoke("set_position", { corner });
      setSettings((current) => (current ? { ...current, corner } : current));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setSavingControl(null);
    }
  };

  const setOpacity = async (opacity: number) => {
    setSavingControl("opacity");
    setError(null);
    setSettings((current) => (current ? { ...current, opacity } : current));
    try {
      await invoke("set_opacity", { opacity });
    } catch (e) {
      setError(errorMessage(e));
      await refreshSettings().catch(() => {});
    } finally {
      setSavingControl(null);
    }
  };

  return (
    <main className="h-full w-full overflow-hidden bg-[#101116] text-white/88">
      <div className="flex h-full flex-col">
        <header className="border-b border-white/[0.08] px-5 pb-4 pt-5">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h1 className="text-[18px] font-semibold leading-none text-white">
                Glint
              </h1>
              <p className="mt-1.5 text-[12px] text-white/45">
                Connections and overlay controls
              </p>
            </div>
            <div className="rounded-md border border-white/[0.08] bg-white/[0.04] px-2.5 py-1 text-[11px] text-white/55">
              {connectedCount}/3 connected
            </div>
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <Section
            title="Connections"
            description="Enable hooks for the tools Glint should watch."
          >
            <div className="divide-y divide-white/[0.07] border-y border-white/[0.07]">
              {connections.map(({ id, connected }) => {
                const meta = connectionMeta[id];
                const busy = busyConnection === id;
                return (
                  <ConnectionRow
                    key={id}
                    label={meta.label}
                    target={meta.target}
                    detail={meta.detail}
                    connected={connected}
                    disabled={busyConnection !== null && !busy}
                    busy={busy}
                    onChange={(next) => setConnection(id, next)}
                  />
                );
              })}
            </div>
          </Section>

          <Section
            title="Overlay"
            description="Choose where the pill lives and how strongly it shows."
          >
            <div className="flex flex-col gap-4 border-y border-white/[0.07] py-4">
              <div className="flex items-start justify-between gap-5">
                <div className="min-w-0 pt-1">
                  <div className="text-[13px] font-medium text-white/88">
                    Position
                  </div>
                  <div className="mt-1 text-[11px] text-white/40">
                    Current: {cornerLabel(settings.corner)}
                  </div>
                </div>
                <div className="grid w-52 grid-cols-2 gap-1.5">
                  {(["tl", "tr", "bl", "br"] as Corner[]).map((corner) => (
                    <button
                      key={corner}
                      type="button"
                      disabled={savingControl === "position"}
                      onClick={() => setCorner(corner)}
                      className={clsx(
                        "h-9 rounded-md border px-2 text-[11px] font-medium transition-colors duration-220 ease-out",
                        settings.corner === corner
                          ? "border-white/18 bg-white/14 text-white"
                          : "border-white/[0.08] bg-white/[0.035] text-white/55 hover:bg-white/[0.07] hover:text-white/78",
                        savingControl === "position" &&
                          "cursor-wait opacity-60"
                      )}
                    >
                      {cornerShortLabel(corner)}
                    </button>
                  ))}
                </div>
              </div>

              <div className="flex items-center justify-between gap-5">
                <div className="min-w-0">
                  <div className="text-[13px] font-medium text-white/88">
                    Opacity
                  </div>
                  <div className="mt-1 text-[11px] text-white/40">
                    {savingControl === "opacity" ? "Saving..." : "Overlay surface strength"}
                  </div>
                </div>
                <div className="flex w-52 items-center gap-3">
                  <input
                    type="range"
                    min={60}
                    max={100}
                    value={Math.round(settings.opacity * 100)}
                    onChange={(e) =>
                      setOpacity(Number(e.currentTarget.value) / 100)
                    }
                    className="min-w-0 flex-1 accent-white/70"
                  />
                  <div className="w-10 text-right text-[11px] text-white/50 tnum">
                    {Math.round(settings.opacity * 100)}%
                  </div>
                </div>
              </div>
            </div>
          </Section>
        </div>

        <footer className="flex min-h-14 items-center justify-between gap-4 border-t border-white/[0.08] px-5">
          <div className="min-w-0 flex-1 truncate text-[11px] text-rose-300/90">
            {error}
          </div>
          <button
            type="button"
            onClick={() => invoke("quit_app")}
            className="h-8 rounded-md border border-white/[0.08] bg-white/[0.04] px-3 text-[12px] text-white/65 transition-colors duration-220 ease-out hover:bg-white/[0.08] hover:text-white/90"
          >
            Quit
          </button>
        </footer>
      </div>
    </main>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="mb-6 last:mb-0">
      <div className="mb-3">
        <h2 className="text-[12px] font-semibold uppercase text-white/38">
          {title}
        </h2>
        <p className="mt-1 text-[11px] text-white/35">{description}</p>
      </div>
      {children}
    </section>
  );
}

function ConnectionRow({
  label,
  target,
  detail,
  connected,
  busy,
  disabled,
  onChange,
}: {
  label: string;
  target: string;
  detail: string;
  connected: boolean;
  busy: boolean;
  disabled: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <div className="flex min-h-[76px] items-center justify-between gap-4 py-3">
      <div className="flex min-w-0 items-center gap-3">
        <div
          className={clsx(
            "flex h-9 w-9 shrink-0 items-center justify-center rounded-md border text-[13px] font-semibold",
            connected
              ? "border-emerald-300/20 bg-emerald-300/[0.09] text-emerald-200"
              : "border-white/[0.08] bg-white/[0.04] text-white/45"
          )}
        >
          {label.slice(0, 1)}
        </div>
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <div className="truncate text-[13px] font-medium text-white/88">
              {label}
            </div>
            <StatusPill connected={connected} busy={busy} />
          </div>
          <div className="mt-1 truncate text-[11px] text-white/38">
            {target}
          </div>
          <div className="mt-0.5 truncate text-[11px] text-white/30">
            {detail}
          </div>
        </div>
      </div>
      <Switch
        value={connected}
        disabled={disabled || busy}
        busy={busy}
        label={`${connected ? "Disable" : "Enable"} ${label}`}
        onChange={onChange}
      />
    </div>
  );
}

function StatusPill({
  connected,
  busy,
}: {
  connected: boolean;
  busy: boolean;
}) {
  return (
    <span
      className={clsx(
        "shrink-0 rounded border px-1.5 py-0.5 text-[10px] leading-none",
        busy
          ? "border-blue-300/20 bg-blue-300/[0.08] text-blue-200/80"
          : connected
            ? "border-emerald-300/18 bg-emerald-300/[0.08] text-emerald-200/85"
            : "border-white/[0.08] bg-white/[0.035] text-white/35"
      )}
    >
      {busy ? "Updating" : connected ? "Connected" : "Off"}
    </span>
  );
}

function Switch({
  value,
  disabled,
  busy,
  label,
  onChange,
}: {
  value: boolean;
  disabled?: boolean;
  busy?: boolean;
  label: string;
  onChange: (value: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-label={label}
      aria-checked={value}
      disabled={disabled}
      onClick={() => onChange(!value)}
      className={clsx(
        "relative h-7 w-12 shrink-0 rounded-full border transition-colors duration-220 ease-out",
        value
          ? "border-emerald-300/25 bg-emerald-400/65"
          : "border-white/[0.10] bg-white/[0.07]",
        disabled && "cursor-not-allowed opacity-55"
      )}
    >
      <span
        className={clsx(
          "absolute top-0.5 h-5 w-5 rounded-full bg-white shadow transition-all duration-220 ease-out",
          value ? "left-[1.55rem]" : "left-0.5",
          busy && "opacity-70"
        )}
      />
    </button>
  );
}

function cornerLabel(corner: Corner): string {
  return {
    tl: "Top left",
    tr: "Top right",
    bl: "Bottom left",
    br: "Bottom right",
  }[corner];
}

function cornerShortLabel(corner: Corner): string {
  return {
    tl: "Top left",
    tr: "Top right",
    bl: "Bottom left",
    br: "Bottom right",
  }[corner];
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
