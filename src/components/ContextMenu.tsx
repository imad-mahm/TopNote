import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useQueueDock } from "../lib/hooks";
import { api, type Settings } from "../lib/ipc";
import { control, loadLyrics, nudgeTrackOffset, updateSettings, useApp, usePlayback } from "../store/app";
import { useBarFit, useVolume, VolumeSlider } from "./ControlBar";
import { Heart, Repeat, Shuffle, Volume } from "./Icons";

const fmt = (v: number) => `${v > 0 ? "+" : ""}${v} ms`;

export function ContextMenu({ x, y, onClose }: { x: number; y: number; onClose: () => void }) {
  const s = useApp((st) => st.settings);
  const clickThrough = useApp((st) => st.clickThrough);
  const p = usePlayback();
  const fit = useBarFit();
  const queue = useQueueDock();
  const vol = useVolume();
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
  }, [x, y]);

  useEffect(() => {
    const close = (e: Event) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("pointerdown", close, true);
    window.addEventListener("keydown", esc);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("pointerdown", close, true);
      window.removeEventListener("keydown", esc);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  if (!s) return null;
  const act = (fn: () => void) => () => {
    fn();
    onClose();
  };
  const trackId = p?.trackId ?? null;
  const isTrack = p?.kind === "track" && !!trackId;
  const strip = window.innerHeight < 120;

  // Controls the bar had no room for.
  const showPair = !!trackId && !fit.pair;
  const showLike = isTrack && !fit.like;
  const showVol = !!trackId && vol.available && !fit.volume;

  // Without a track, the timing stepper adjusts the global offset instead.
  const songOffset = trackId ? s.trackOffsets?.[trackId] ?? 0 : s.offsetMs;
  const nudge = (d: number | null) =>
    trackId ? nudgeTrackOffset(d) : updateSettings({ offsetMs: d == null ? 0 : Math.max(-5000, Math.min(5000, s.offsetMs + d)) });

  const mode = s.overlay ? "overlay" : s.view;
  const setMode = (m: "artwork" | "lyrics" | "overlay") =>
    updateSettings(m === "overlay" ? { overlay: true } : { overlay: false, view: m });

  return (
    <div ref={ref} className="menu" style={{ left: pos.x, top: pos.y }} role="menu">
      {(showPair || showLike) && (
        <div className="menu-quick">
          {showPair && (
            <button className={`ic sm ${p!.shuffle ? "on" : ""}`} title="Shuffle" onClick={() => control("shuffle")}>
              <Shuffle width={16} height={16} />
            </button>
          )}
          {showPair && (
            <button className={`ic sm ${p!.repeat !== "off" ? "on" : ""}`} title={`Repeat: ${p!.repeat}`} onClick={() => control("repeat")}>
              <Repeat one={p!.repeat === "track"} width={16} height={16} />
            </button>
          )}
          {showLike && (
            <button className={`ic sm ${p!.liked ? "on" : ""}`} title={p!.liked ? "Remove from Liked" : "Save to Liked"} onClick={() => control(p!.liked ? "unlike" : "like")}>
              <Heart filled={p!.liked} width={16} height={16} />
            </button>
          )}
        </div>
      )}
      {showVol && (
        <div className="menu-row">
          <button className="ic sm" title={vol.v > 0 ? "Mute" : "Unmute"} onClick={vol.toggleMute}>
            <Volume level={vol.v === 0 ? 0 : vol.v < 50 ? 1 : 2} width={16} height={16} />
          </button>
          <div className="menu-slider">
            <VolumeSlider vol={vol} />
          </div>
        </div>
      )}
      {(showPair || showLike || showVol) && <hr />}

      <div className="seg" role="radiogroup" aria-label="View">
        {(["artwork", "lyrics", "overlay"] as const).map((m) => (
          <button key={m} role="radio" aria-checked={mode === m} className={mode === m ? "on" : ""} onClick={act(() => setMode(m))}>
            {m === "artwork" ? "Artwork" : m === "lyrics" ? "Lyrics" : "Overlay"}
          </button>
        ))}
      </div>
      {s.overlay && (
        <Row label="Background">
          <input type="range" min={0} max={1} step={0.05} value={s.overlayBgOpacity} onChange={(e) => updateSettings({ overlayBgOpacity: Number(e.target.value) })} />
        </Row>
      )}
      {!strip && trackId && <Toggle label="Up next queue" on={queue.open} onClick={act(queue.toggle)} />}

      <div className="menu-label">Lyrics</div>
      <Row label={trackId ? "Song timing" : "Lyric timing"}>
        <div className="menu-stepper">
          <button onClick={() => nudge(-50)} title="Earlier">−</button>
          <span className="mono" onDoubleClick={() => nudge(null)} title="Double-click to reset">{fmt(songOffset)}</span>
          <button onClick={() => nudge(50)} title="Later">+</button>
        </div>
      </Row>
      <Row label="Translation">
        <select value={s.translationMode} onChange={(e) => updateSettings({ translationMode: e.target.value as Settings["translationMode"] })}>
          <option value="off">Off</option>
          <option value="translate">Translate</option>
          <option value="romanize">Romanize</option>
          <option value="both">Both</option>
        </select>
      </Row>
      {trackId && s.manualSearch && !strip && (
        <button role="menuitem" onClick={act(() => useApp.setState({ panel: "search" }))}>Search lyrics…</button>
      )}
      {trackId && <button role="menuitem" onClick={act(() => loadLyrics(true))}>Refetch lyrics</button>}

      <div className="menu-label">Window</div>
      <Row label="Opacity">
        <input type="range" min={0.3} max={1} step={0.05} value={s.opacity} onChange={(e) => updateSettings({ opacity: Number(e.target.value) })} />
      </Row>
      <Toggle label="Click-through" hint={s.shortcuts.clickThrough} on={clickThrough} onClick={act(() => api.setClickThrough(!clickThrough))} />

      <hr />
      <div className="menu-foot">
        <button onClick={act(() => api.openSettings())}>Settings</button>
        <button onClick={act(() => api.hideMain())}>Hide</button>
        <button className="danger" onClick={act(() => api.quit())}>Quit</button>
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="menu-row">
      <span>{label}</span>
      {children}
    </div>
  );
}

function Toggle({ label, hint, on, onClick }: { label: string; hint?: string; on: boolean; onClick: () => void }) {
  return (
    <button role="menuitemcheckbox" aria-checked={on} onClick={onClick}>
      <span className="grow">{label}</span>
      {hint && <kbd>{hint.replace(/\b\w/g, (c) => c.toUpperCase())}</kbd>}
      <span className={`switch ${on ? "on" : ""}`} aria-hidden />
    </button>
  );
}
