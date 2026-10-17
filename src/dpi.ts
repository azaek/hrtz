// DPI values for one mouse: its range and step, typed input, and where a new stage goes.

/** A mouse's DPI range and step, from its capabilities. */
export type DpiRange = { min: number; max: number; step: number };
export const dpiRange = (dpi: DpiRange): DpiRange => ({ min: dpi.min, max: dpi.max, step: dpi.step });

/** The values players use most. Drags settle on them. */
export const COMMON_DPI = [400, 800, 1600, 3200, 6400];

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
export const fmtDpi = (dpi: number) => dpi.toLocaleString("en-US");
/** 1600 → "1.6K", 30000 → "30K", 800 → "800". */
export const shortDpi = (dpi: number) => (dpi >= 1000 ? `${+(dpi / 1000).toFixed(dpi % 1000 ? 1 : 0)}K` : `${dpi}`);
export const toStep = (r: DpiRange, v: number) => clamp(Math.round(v / r.step) * r.step, r.min, r.max);

/** "1600", "1,600", "1.6k", "800*2", "3200/2" → a number, or null. */
export function parseDpi(text: string): number | null {
  const t = text.trim().toLowerCase().replace(/[,\s]/g, "");
  const m = /^(\d+(?:\.\d+)?)(k)?(?:([*x×/])(\d+(?:\.\d+)?))?$/.exec(t);
  if (!m) return null;
  let v = parseFloat(m[1]) * (m[2] ? 1000 : 1);
  if (m[3]) {
    const b = parseFloat(m[4]);
    v = m[3] === "/" ? v / b : v * b;
  }
  return Number.isFinite(v) && v > 0 ? v : null;
}

/** `v`, or the nearest free step from it in `dir`, then the other way. Two stages never share a value. */
export function nextFree(r: DpiRange, v: number, taken: Set<number>, dir: 1 | -1): number {
  for (const d of [dir, -dir]) {
    let x = toStep(r, v);
    while (taken.has(x) && x + d * r.step >= r.min && x + d * r.step <= r.max) x += d * r.step;
    if (!taken.has(x)) return x;
  }
  return toStep(r, v);
}

/** Where a new stage goes: double the last, or the middle of the widest gap if that's taken. */
export function newStageValue(r: DpiRange, values: number[]): number {
  const last = values[values.length - 1] ?? 800;
  const doubled = toStep(r, last * 2);
  if (doubled !== last && !values.includes(doubled) && last * 2 <= r.max) return doubled;
  const sorted = [r.min, ...[...values].sort((a, b) => a - b), r.max];
  let best = [r.min, r.max];
  for (let i = 1; i < sorted.length; i++) if (sorted[i] / sorted[i - 1] > best[1] / best[0]) best = [sorted[i - 1], sorted[i]];
  return nextFree(r, Math.sqrt(best[0] * best[1]), new Set(values), 1);
}
