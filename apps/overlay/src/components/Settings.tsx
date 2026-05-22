import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import clsx from "clsx";
import type { Corner, SettingsDTO } from "../types";

export default function Settings() {
  const [s, setS] = useState<SettingsDTO | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<SettingsDTO>("get_settings").then(setS).catch((e) => setError(String(e)));
  }, []);

  if (!s) {
    return (
      <div className="surface m-3 p-5 text-label text-white/55">Loading…</div>
    );
  }

  const setCodexConnected = async (next: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (next) {
        await invoke("install_codex_hooks");
      } else {
        await invoke("remove_codex_hooks");
      }
      const fresh = await invoke<SettingsDTO>("get_settings");
      setS(fresh);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const setCursorConnected = async (next: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (next) {
        await invoke("install_cursor_hooks");
      } else {
        await invoke("remove_cursor_hooks");
      }
      const fresh = await invoke<SettingsDTO>("get_settings");
      setS(fresh);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const setCorner = async (corner: Corner) => {
    await invoke("set_position", { corner });
    setS({ ...s, corner });
  };

  const setOpacity = async (opacity: number) => {
    await invoke("set_opacity", { opacity });
    setS({ ...s, opacity });
  };

  return (
    <div className="surface m-3 p-5 flex flex-col gap-5 text-white/85">
      <header className="flex items-baseline justify-between">
        <h1 className="text-[15px] font-medium">overlay-app · settings</h1>
        <span className="text-[11px] text-white/40 tnum">v0.1</span>
      </header>

      <section className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-value flex items-center gap-2">
              Connect Codex
              {s.codexConnected ? (
                <span className="text-[10px] text-emerald-300/85 px-1.5 py-0.5 rounded bg-emerald-400/10 border border-emerald-400/20">
                  connected
                </span>
              ) : null}
            </div>
            <div className="text-[11px] text-white/45 mt-0.5">
              Writes hook entries into ~/.codex/config.toml. Restart Codex Desktop after enabling.
            </div>
          </div>
          <Switch
            value={s.codexConnected}
            disabled={busy}
            onChange={setCodexConnected}
          />
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <div className="text-value flex items-center gap-2">
              Connect Cursor
              {s.cursorConnected ? (
                <span className="text-[10px] text-emerald-300/85 px-1.5 py-0.5 rounded bg-emerald-400/10 border border-emerald-400/20">
                  connected
                </span>
              ) : null}
            </div>
            <div className="text-[11px] text-white/45 mt-0.5">
              Writes hook entries into ~/.cursor/hooks.json. Restart Cursor after enabling.
            </div>
          </div>
          <Switch
            value={s.cursorConnected}
            disabled={busy}
            onChange={setCursorConnected}
          />
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <div className="text-value">Position</div>
        <div className="grid grid-cols-2 gap-1.5 w-44">
          {(["tl", "tr", "bl", "br"] as Corner[]).map((c) => (
            <button
              key={c}
              onClick={() => setCorner(c)}
              className={clsx(
                "h-9 rounded-md text-[11px] border border-white/10 transition-colors duration-220 ease-out",
                s.corner === c
                  ? "bg-white/10 text-white"
                  : "bg-white/[0.03] text-white/55 hover:bg-white/[0.06]"
              )}
            >
              {cornerLabel(c)}
            </button>
          ))}
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <div className="flex items-baseline justify-between">
          <div className="text-value">Opacity</div>
          <div className="text-[11px] text-white/40 tnum">
            {Math.round(s.opacity * 100)}%
          </div>
        </div>
        <input
          type="range"
          min={60}
          max={100}
          value={Math.round(s.opacity * 100)}
          onChange={(e) => setOpacity(Number(e.target.value) / 100)}
          className="w-full accent-white/60"
        />
      </section>

      <section className="border-t border-white/[0.06] pt-4 flex items-center justify-between">
        <span className="text-[11px] text-white/35">
          {error ? <span className="text-rose-300">{error}</span> : null}
        </span>
        <button
          onClick={() => invoke("quit_app")}
          className="text-[12px] px-3 h-8 rounded-md bg-white/[0.05] hover:bg-white/[0.09] border border-white/10 text-white/75 transition-colors duration-220 ease-out"
        >
          Quit
        </button>
      </section>
    </div>
  );
}

function Switch({
  value,
  disabled,
  onChange,
}: {
  value: boolean;
  disabled?: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      role="switch"
      aria-checked={value}
      disabled={disabled}
      onClick={() => onChange(!value)}
      className={clsx(
        "relative h-6 w-10 rounded-full transition-colors duration-220 ease-out border border-white/10",
        value ? "bg-emerald-400/70" : "bg-white/[0.08]",
        disabled && "opacity-50 cursor-not-allowed"
      )}
    >
      <span
        className={clsx(
          "absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-all duration-220 ease-out",
          value ? "left-[1.25rem]" : "left-0.5"
        )}
      />
    </button>
  );
}

function cornerLabel(c: Corner): string {
  return { tl: "Top left", tr: "Top right", bl: "Bottom left", br: "Bottom right" }[c];
}
