import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { api } from "../lib/ipc";
import { loadLyrics, updateSettings, useApp } from "../store/app";

export function ContextMenu({ x, y, onClose }: { x: number; y: number; onClose: () => void }) {
  const s = useApp((st) => st.settings);
  const clickThrough = useApp((st) => st.clickThrough);
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
  const offset = (d: number) => updateSettings({ offsetMs: Math.max(-5000, Math.min(5000, s.offsetMs + d)) });

  return (
    <div ref={ref} className="menu" style={{ left: pos.x, top: pos.y }} role="menu">
      <button role="menuitem" onClick={act(() => updateSettings({ view: s.view === "lyrics" ? "artwork" : "lyrics" }))}>
        {s.view === "lyrics" ? "Show artwork" : "Show lyrics"}
      </button>
      <button role="menuitem" onClick={act(() => loadLyrics(true))}>Refetch lyrics</button>
      <div className="menu-row">
        <span>Lyric offset</span>
        <div className="menu-stepper">
          <button onClick={() => offset(-50)} title="Earlier">−</button>
          <span className="mono">{s.offsetMs > 0 ? "+" : ""}{s.offsetMs}ms</span>
          <button onClick={() => offset(50)} title="Later">+</button>
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
