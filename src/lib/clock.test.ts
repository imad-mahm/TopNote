import { describe, expect, it } from "vitest";
import { activeIndex, emptyAnchor, estimate, reconcile, EASE_MS } from "./clock";
import { barWidth, fitControls, layoutFor } from "./layout";

const sample = (p: Partial<Parameters<typeof reconcile>[1]> = {}) => ({
  progressMs: 10_000,
  ts: 1000,
  isPlaying: true,
  durationMs: 200_000,
  trackId: "a",
  ...p,
});

describe("clock", () => {
  it("interpolates while playing", () => {
    const a = reconcile(emptyAnchor, sample(), 1000);
    expect(estimate(a, 1500)).toBe(10_500);
  });

  it("does not advance while paused", () => {
    const a = reconcile(emptyAnchor, sample({ isPlaying: false }), 1000);
    expect(estimate(a, 5000)).toBe(10_000);
  });

  it("snaps on large drift", () => {
    const a = reconcile(emptyAnchor, sample(), 1000);
    const b = reconcile(a, sample({ progressMs: 30_000, ts: 2000 }), 2000);
    expect(estimate(b, 2000)).toBe(30_000);
  });

  it("eases small drift without jumping", () => {
    const a = reconcile(emptyAnchor, sample(), 1000);
    // predicted at 2000 is 11000; server says 11200
    const b = reconcile(a, sample({ progressMs: 11_200, ts: 2000 }), 2000);
    expect(estimate(b, 2000)).toBe(11_000);
    expect(estimate(b, 2000 + EASE_MS)).toBe(11_000 + EASE_MS + 200);
  });

  it("snaps on track change", () => {
    const a = reconcile(emptyAnchor, sample(), 1000);
    const b = reconcile(a, sample({ trackId: "b", progressMs: 0, ts: 1100 }), 1100);
    expect(estimate(b, 1100)).toBe(0);
  });

  it("clamps to duration", () => {
    const a = reconcile(emptyAnchor, sample({ progressMs: 199_900 }), 1000);
    expect(estimate(a, 9000)).toBe(200_000);
  });
});

describe("activeIndex", () => {
  const lines = [{ startMs: 1000 }, { startMs: 2000 }, { startMs: 3000 }];
  it("finds the current line", () => {
    expect(activeIndex(lines, 500)).toBe(-1);
    expect(activeIndex(lines, 1000)).toBe(0);
    expect(activeIndex(lines, 2500)).toBe(1);
    expect(activeIndex(lines, 99999)).toBe(2);
    expect(activeIndex([], 5)).toBe(-1);
  });
});

describe("layoutFor", () => {
  it("picks breakpoints", () => {
    expect(layoutFor(400, 100)).toBe("strip");
    expect(layoutFor(300, 300)).toBe("compact");
    expect(layoutFor(600, 300)).toBe("wide");
    expect(layoutFor(420, 520)).toBe("tall");
  });
});

describe("fitControls", () => {
  const has = { like: true, volume: true, view: true };
  it("shows everything when wide", () => {
    expect(fitControls(600, has)).toEqual({ pair: true, queue: true, like: true, volume: true, view: true });
  });
  it("drops the least important buttons first and never overflows", () => {
    for (let w = 220; w <= 600; w += 5) {
      const f = fitControls(w, has);
      expect(barWidth(f)).toBeLessThanOrEqual(w);
    }
    const def = fitControls(340, has);
    expect(def.queue).toBe(false);
    expect(def.pair && def.like && def.volume && def.view).toBe(true);
    const narrow = fitControls(230, has);
    expect(narrow.pair || narrow.like || narrow.volume).toBe(false);
  });
  it("respects buttons that don't apply", () => {
    expect(fitControls(600, { like: false, volume: false, view: false })).toMatchObject({ like: false, volume: false, view: false });
  });
});
