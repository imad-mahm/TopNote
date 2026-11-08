import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { effectiveOffset, updateSettings, useApp } from "../store/app";
import { estimate } from "./clock";
import { queueDock, type QueueDock } from "./layout";

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

/** Live window inner size. */
export function useWindowSize() {
  const [s, setS] = useState({ w: window.innerWidth, h: window.innerHeight });
  useEffect(() => {
    const on = () => setS({ w: window.innerWidth, h: window.innerHeight });
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return s;
}

const POPUP: QueueDock = { mode: "popup", size: 0 };

/**
 * Queue placement and visibility. When the window has room the queue is part of the
 * layout (toggled by the `dockQueue` setting); otherwise it is a popup that is only
 * open while the user wants it (`panel === "queue"`), so it never covers the lyrics for good.
 */
export function useQueueDock() {
  const { w, h } = useWindowSize();
  const overlay = useApp((s) => s.settings?.overlay ?? false);
  const dockQueue = useApp((s) => s.settings?.dockQueue ?? true);
  const panel = useApp((s) => s.panel);
  const hasTrack = useApp((s) => !!s.playback?.trackId);
  const base = queueDock(w, h);
  // the lyrics overlay stays minimal: queue only on demand
  const dock = overlay && base.mode !== "none" ? POPUP : base;
  const dockable = dock.mode === "side" || dock.mode === "bottom";
  const docked = hasTrack && dockable && dockQueue;
  const popup = hasTrack && dock.mode === "popup" && panel === "queue";
  const toggle = () => {
    if (dockable) updateSettings({ dockQueue: !dockQueue });
    else useApp.setState({ panel: panel === "queue" ? null : "queue" });
  };
  /** Width left for the lyrics/artwork column. */
  const stageW = docked && dock.mode === "side" ? w - dock.size : w;
  return { dock, dockable, docked, popup, open: docked || popup, toggle, stageW };
}

/** Live window inner width. */
export function useWindowWidth() {
  const [w, setW] = useState(window.innerWidth);
  useEffect(() => {
    const on = () => setW(window.innerWidth);
    window.addEventListener("resize", on);
    return () => window.removeEventListener("resize", on);
  }, []);
  return w;
}
