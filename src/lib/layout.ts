export type Layout = "strip" | "compact" | "wide" | "tall";

export function layoutFor(w: number, h: number): Layout {
  if (h < 120) return "strip";
  if (w <= 320) return "compact";
  if (w / h > 1.6) return "wide";
  return "tall";
}

/** Which control-bar buttons fit at a given window width. Hidden ones move into the ⋯ menu. */
export type BarFit = { pair: boolean; like: boolean; queue: boolean; volume: boolean; view: boolean };
export type BarHas = { like: boolean; volume: boolean; view: boolean };

const BAR = { pad: 20, core: 108, pair: 72, btn: 26, gap: 8 } as const;
/** Least important first. */
const DROP: (keyof BarFit)[] = ["queue", "pair", "like", "volume", "view"];

/** Side buttons in display order; the ⋯ button is always last. */
export function sideButtons(f: BarFit): { left: (keyof BarFit)[]; right: (keyof BarFit | "more")[] } {
  const all = (["volume", "like", "queue", "view"] as const).filter((k) => f[k]);
  const list: (keyof BarFit | "more")[] = [...all, "more"];
  const n = Math.floor(list.length / 2);
  return { left: list.slice(0, n) as (keyof BarFit)[], right: list.slice(n) };
}

export function barWidth(f: BarFit): number {
  const { left, right } = sideButtons(f);
  const side = Math.max(left.length, right.length) * BAR.btn + BAR.gap;
  return BAR.pad + BAR.core + (f.pair ? BAR.pair : 0) + 2 * side;
}

export function fitControls(width: number, has: BarHas): BarFit {
  const f: BarFit = { pair: true, queue: true, like: has.like, volume: has.volume, view: has.view };
  for (const k of DROP) {
    if (barWidth(f) <= width) break;
    f[k] = false;
  }
  return f;
}

export function fmtTime(ms: number): string {
  if (!isFinite(ms) || ms < 0) ms = 0;
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
