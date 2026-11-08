/** Interpolated playback clock. Pure functions so they're testable. */
export type Anchor = {
  /** position (ms) at `at` */
  base: number;
  /** wall-clock ms (Date.now) the base refers to */
  at: number;
  playing: boolean;
  duration: number;
  /** drift being eased in over EASE_MS, starting at `at` */
  drift: number;
  trackId: string | null;
};

export const SNAP_MS = 400;
export const EASE_MS = 200;

export const emptyAnchor: Anchor = { base: 0, at: 0, playing: false, duration: 0, drift: 0, trackId: null };

export function estimate(a: Anchor, now: number): number {
  if (!a.playing) return clamp(a.base + a.drift, a.duration);
  const elapsed = Math.max(0, now - a.at);
  const easedDrift = a.drift * Math.min(1, elapsed / EASE_MS);
  return clamp(a.base + elapsed + easedDrift, a.duration);
}

function clamp(v: number, duration: number) {
  if (duration > 0) v = Math.min(v, duration);
  return Math.max(0, v);
}

/**
 * Merge a fresh server sample into the anchor. Big drift, track change or play/pause change
 * → snap; small drift → ease it in so the UI never visibly jumps.
 */
export function reconcile(
  prev: Anchor,
  sample: { progressMs: number; ts: number; isPlaying: boolean; durationMs: number; trackId: string | null },
  now: number,
): Anchor {
  const target = sample.isPlaying ? sample.progressMs + Math.max(0, now - sample.ts) : sample.progressMs;
  const fresh: Anchor = {
    base: target,
    at: now,
    playing: sample.isPlaying,
    duration: sample.durationMs,
    drift: 0,
    trackId: sample.trackId,
  };
  if (prev.trackId !== sample.trackId || prev.playing !== sample.isPlaying || prev.at === 0) return fresh;
  const predicted = estimate(prev, now);
  const drift = target - predicted;
  if (Math.abs(drift) > SNAP_MS) return fresh;
  return { ...fresh, base: predicted, drift };
}

/** Index of the last line whose start <= pos, or -1 before the first line. */
export function activeIndex(starts: { startMs: number }[], pos: number): number {
  let lo = 0;
  let hi = starts.length - 1;
  let ans = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (starts[mid].startMs <= pos) {
      ans = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return ans;
}
