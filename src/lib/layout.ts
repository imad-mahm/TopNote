export type Layout = "strip" | "compact" | "wide" | "tall";

export function layoutFor(w: number, h: number): Layout {
  if (h < 120) return "strip";
  if (w <= 320) return "compact";
  if (w / h > 1.6) return "wide";
  return "tall";
}

export function fmtTime(ms: number): string {
  if (!isFinite(ms) || ms < 0) ms = 0;
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
