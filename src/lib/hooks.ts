import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { effectiveOffset, useApp } from "../store/app";
import { estimate } from "./clock";

/** Run `cb` every animation frame with the current estimated playback position (incl. offset). */
export function useFrame(cb: (posMs: number, now: number) => void, deps: unknown[] = []) {
  const ref = useRef(cb);
  ref.current = cb;
  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const st = useApp.getState();
      const now = Date.now();
      const pos = estimate(st.anchor, now) + effectiveOffset(st);
      ref.current(pos, now);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

export function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState({ w: window.innerWidth, h: window.innerHeight });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const r = e.contentRect;
      setSize({ w: Math.round(r.width), h: Math.round(r.height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size] as const;
}
