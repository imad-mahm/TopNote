import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api, type Settings } from "../lib/ipc";
import { loadLyrics, nudgeTrackOffset, updateSettings, useApp } from "../store/app";

const fmt = (v: number) => `${v > 0 ? "+" : ""}${v}ms`;

export function ContextMenu({ x, y, onClose }: { x: number; y: number; onClose: () => void }) {
  const s = useApp((st) => st.settings);
  const clickThrough = useApp((st) => st.clickThrough);
  const trackId = useApp((st) => st.playback?.trackId ?? null);
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
  const globalOffset = (d: number) => updateSettings({ offsetMs: Math.max(-5000, Math.min(5000, s.offsetMs + d)) });
  const songOffset = trackId ? s.trackOffsets?.[trackId] ?? 0 : 0;

  return (
    <div ref={ref} className="menu" style={{ left: pos.x, top: pos.y }} role="menu">
      {!s.overlay && (
        <button role="menuitem" onClick={act(() => updateSettings({ view: s.view === "lyrics" ? "artwork" : "lyrics" }))}>
          {s.view === "lyrics" ? "Show artwork" : "Show lyrics"}
        </button>
      )}
      <button role="menuitemcheckbox" aria-checked={s.overlay} onClick={act(() => updateSettings({ overlay: !s.overlay }))}>
        <Check on={s.overlay} /> Lyrics overlay
      </button>
      {s.overlay && (
        <div className="menu-row">
          <span>Overlay background</span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={s.overlayBgOpacity}
            onChange={(e) => updateSettings({ overlayBgOpacity: Number(e.target.value) })}
          />
        </div>
      )}
      <button role="menuitemcheckbox" aria-checked={s.showQueue} onClick={act(() => updateSettings({ showQueue: !s.showQueue }))}>
        <Check on={s.showQueue} /> Show queue
      </button>
      <hr />
      <button role="menuitem" onClick={act(() => loadLyrics(true))}>Refetch lyrics</button>
      {s.manualSearch && trackId && (
        <button role="menuitem" onClick={act(() => useApp.setState({ panel: "search" }))}>Search lyrics…</button>
      )}
      <div className="menu-row">
        <span>Translation</span>
        <select
          value={s.translationMode}
          onChange={(e) => updateSettings({ translationMode: e.target.value as Settings["translationMode"] })}
        >
          <option value="off">Off</option>
          <option value="translate">Translate</option>
          <option value="romanize">Romanize</option>
          <option value="both">Both</option>
        </select>
      </div>
      {trackId && (
        <div className="menu-row">
          <span title="Remembered for this song">This song</span>
          <div className="menu-stepper">
            <button onClick={() => nudgeTrackOffset(-50)} title="Earlier">−</button>
            <span className="mono" onDoubleClick={() => nudgeTrackOffset(null)} title="Double-click to reset">{fmt(songOffset)}</span>
            <button onClick={() => nudgeTrackOffset(50)} title="Later">+</button>
          </div>
        </div>
      )}
      <div className="menu-row">
        <span>All songs</span>
        <div className="menu-stepper">
          <button onClick={() => globalOffset(-50)} title="Earlier">−</button>
          <span className="mono">{fmt(s.offsetMs)}</span>
          <button onClick={() => globalOffset(50)} title="Later">+</button>
        </div>
      </div>
      <div className="menu-row">
        <span>Opacity</span>
        <input
          type="range"
          min={0.3}
          max={1}
          step={0.05}
          value={s.opacity}
          onChange={(e) => updateSettings({ opacity: Number(e.target.value) })}
        />
      </div>
      <button role="menuitem" onClick={act(() => api.setClickThrough(!clickThrough))}>
        Click-through mode <kbd>{s.shortcuts.clickThrough}</kbd>
      </button>
      <hr />
      <button role="menuitem" onClick={act(() => api.openSettings())}>Settings…</button>
      <button role="menuitem" onClick={act(() => api.hideMain())}>Hide to tray</button>
      <button role="menuitem" className="danger" onClick={act(() => api.quit())}>Quit</button>
    </div>
  );
}

function Check({ on }: { on: boolean }) {
  return <span className={`check ${on ? "on" : ""}`} aria-hidden>{on ? "✓" : ""}</span>;
}
