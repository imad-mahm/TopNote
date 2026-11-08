/** Colours derived from artwork: a vivid accent plus a deep background pair for the "song colour" theme. */
export type Palette = { accent: string | null; bg: string; bg2: string };
const cache = new Map<string, Palette>();

export async function accentFromImage(url: string): Promise<string | null> {
  return (await paletteFromImage(url))?.accent ?? null;
}

export async function paletteFromImage(url: string): Promise<Palette | null> {
  if (cache.has(url)) return cache.get(url)!;
  try {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = url;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = c.height = 32;
    const ctx = c.getContext("2d", { willReadFrequently: true })!;
    ctx.drawImage(img, 0, 0, 32, 32);
    const d = ctx.getImageData(0, 0, 32, 32).data;
    let best = { score: -1, h: 0, s: 0 };
    // bucket hues, weight by saturation and mid lightness
    const buckets = new Array(36).fill(0).map(() => ({ w: 0, s: 0 }));
    for (let i = 0; i < d.length; i += 4) {
      const [h, s, l] = rgbToHsl(d[i], d[i + 1], d[i + 2]);
      const w = s * (1 - Math.abs(l - 0.5) * 2);
      const b = buckets[Math.floor(h / 10) % 36];
      b.w += w;
      b.s += s * w;
    }
    buckets.forEach((b, i) => {
      if (b.w > best.score) best = { score: b.w, h: i * 10 + 5, s: b.w ? b.s / b.w : 0 };
    });
    // second-strongest hue (at least 40 degrees away) for a gentle gradient
    let alt = { score: -1, h: best.h };
    buckets.forEach((b, i) => {
      const h = i * 10 + 5;
      const dist = Math.min(Math.abs(h - best.h), 360 - Math.abs(h - best.h));
      if (dist >= 40 && b.w > alt.score) alt = { score: b.w, h };
    });
    if (alt.score < best.score * 0.15) alt = { score: 0, h: (best.h + 30) % 360 };
    const grey = best.score < 2;
    const sat = Math.round(Math.max(55, best.s * 100));
    const out: Palette = grey
      ? { accent: null, bg: "hsl(0 0% 14%)", bg2: "hsl(0 0% 8%)" }
      : {
          accent: `hsl(${best.h} ${sat}% 72%)`,
          bg: `hsl(${best.h} ${Math.min(70, sat)}% 22%)`,
          bg2: `hsl(${alt.h} ${Math.min(60, sat)}% 11%)`,
        };
    cache.set(url, out);
    return out;
  } catch {
    return null;
  }
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const dlt = max - min;
  const s = l > 0.5 ? dlt / (2 - max - min) : dlt / (max + min);
  let h = 0;
  if (max === r) h = (g - b) / dlt + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / dlt + 2;
  else h = (r - g) / dlt + 4;
  return [h * 60, s, l];
}

/** Make a hex color light enough to read on a dark background. */
export function readable(hex: string): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex);
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  const [h, s, l] = rgbToHsl((n >> 16) & 255, (n >> 8) & 255, n & 255);
  return `hsl(${Math.round(h)} ${Math.round(Math.max(s, 0.5) * 100)}% ${Math.round(Math.max(l, 0.68) * 100)}%)`;
}
