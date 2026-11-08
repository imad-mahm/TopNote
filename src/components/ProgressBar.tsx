import { useRef, useState } from "react";
import { useFrame } from "../lib/hooks";
import { fmtTime } from "../lib/layout";
import { control, useApp, useEffectiveOffset } from "../store/app";

export function ProgressBar({ showTimes = true }: { showTimes?: boolean }) {
  const fill = useRef<HTMLDivElement>(null);
  const cur = useRef<HTMLSpanElement>(null);
  const track = useRef<HTMLDivElement>(null);
  const duration = useApp((s) => s.playback?.durationMs ?? 0);
  const [drag, setDrag] = useState<number | null>(null);
  const offset = useEffectiveOffset();

  useFrame(
    (pos) => {
      const p = drag ?? Math.max(0, pos - offset);
      const pct = duration ? Math.min(1, p / duration) : 0;
      if (fill.current) fill.current.style.transform = `scaleX(${pct})`;
      if (cur.current) {
        const t = fmtTime(p);
        if (cur.current.textContent !== t) cur.current.textContent = t;
      }
    },
    [drag, duration, offset],
  );

  const at = (clientX: number) => {
    const r = track.current!.getBoundingClientRect();
    return Math.max(0, Math.min(1, (clientX - r.left) / r.width)) * duration;
  };

  return (
    <div className="progress">
      {showTimes && <span ref={cur} className="time">0:00</span>}
      <div
        ref={track}
        className="progress-track"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          setDrag(at(e.clientX));
        }}
        onPointerMove={(e) => drag != null && setDrag(at(e.clientX))}
        onPointerUp={(e) => {
          if (drag == null) return;
          const v = at(e.clientX);
          setDrag(null);
          control("seek", Math.round(v));
        }}
      >
        <div className="progress-rail" />
        <div ref={fill} className="progress-fill" />
      </div>
      {showTimes && <span className="time">{fmtTime(duration)}</span>}
    </div>
  );
}

/** Thin always-visible progress line, shown while the controls are hidden. */
export function MiniProgress() {
  const fill = useRef<HTMLDivElement>(null);
  const duration = useApp((s) => s.playback?.durationMs ?? 0);
  const offset = useEffectiveOffset();
  useFrame(
    (pos) => {
      const pct = duration ? Math.min(1, Math.max(0, pos - offset) / duration) : 0;
      if (fill.current) fill.current.style.transform = `scaleX(${pct})`;
    },
    [duration, offset],
  );
  return (
    <div className="mini-progress" aria-hidden>
      <div ref={fill} />
    </div>
  );
}
