import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { effectiveOffset, updateSettings, useApp } from "../store/app";
import { estimate } from "./clock";
import { queueDock, type QueueDock } from "./layout";

type FrameCb = { current: (posMs: number, now: number) => void };

/** One animation loop shared by every subscriber (progress bars, lyrics). */
const frameSubs = new Set<FrameCb>();
let raf = 0;
let lastRun = 0;
/** While paused nothing moves, so 4 updates a second are plenty (for timers like "resume follow"). */
const IDLE_MS = 250;

function frame() {
  raf = 0;
  if (!frameSubs.size) return;
  const st = useApp.getState();
  const now = Date.now();
  if (st.anchor.playing || now - lastRun >= IDLE_MS) {
    lastRun = now;
    const pos = estimate(st.anchor, now) + effectiveOffset(st);
    frameSubs.forEach((s) => s.current(pos, now));
  }
  raf = requestAnimationFrame(frame);
}

// a seek/track change while paused should show on the next frame, not the next idle tick
useApp.subscribe((s, prev) => {
  if (s.anchor !== prev.anchor || s.settings !== prev.settings) lastRun = 0;
});

/** Run `cb` every animation frame with the current estimated playback position (incl. offset). */
export function useFrame(cb: (posMs: number, now: number) => void, deps: unknown[] = []) {
  const ref = useRef(cb);
  ref.current = cb;
  useEffect(() => {
    frameSubs.add(ref);
    lastRun = 0;
    if (!raf) raf = requestAnimationFrame(frame);
    return () => {
      frameSubs.delete(ref);
    };
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
