import type { BatteryEntry } from "@/api";
import { dateLocale, num, t } from "@/i18n";

/** A longer gap between log rows means the app wasn't running (matches `battery_log.rs`). */
const GAP = 45 * 60;
/** Room above the bars for the charging bolt, the bars themselves, and the time labels below (px). */
const TOP = 22;
const PLOT_H = 140;
const BELOW = 24;
export const CHART_HEIGHT = TOP + PLOT_H + BELOW;
/** The level labels, on the right. */
export const RIGHT = 40;
/** A bar's share of its slot's width. */
const BAR_WIDTH = 0.9;

/** Charging and battery saver, as in the level bar (`LevelBar`): Tailwind's emerald and amber,
 *  400 on dark and 600 on light, where the 400s fall under 3:1 against the card. */
export const tints = (dark: boolean) => (dark ? { charging: "#34d399", saver: "#fbbf24" } : { charging: "#059669", saver: "#d97706" });

/** One bar: a stretch of the clock and what the log saw in it. */
export interface Slot {
  start: number;
  end: number;
  /** The level at the last moment seen, or null if the app saw none of it. */
  level: number | null;
  charging: boolean;
  saver: boolean;
  /** Seconds awake at each rate, and asleep. */
  awake: Map<number, number>;
  asleep: number;
}

/** Hours a bar covers, so each range has about as many bars, as wide: 24 over a day, 28 over a
 *  week, 30 over a month. */
export const hoursPerBar = (days: number) => (days <= 1 ? 1 : days <= 7 ? 6 : 24);

/** The bars' edges on the local clock, the last at the first boundary after `now`. */
function edges(days: number, now: number): number[] {
  const h = hoursPerBar(days);
  const d = new Date(now * 1000);
  d.setHours(Math.floor(d.getHours() / h) * h + h, 0, 0, 0);
  const out = [d.getTime() / 1000];
  for (let i = 0; i < (days * 24) / h; i++) {
    d.setHours(d.getHours() - h);
    out.unshift(d.getTime() / 1000);
  }
  return out;
}

/** The log's rows sorted into bars. A row holds until the next one, unless the app stopped
 *  seeing the mouse in between. */
export function slotsOf(rows: BatteryEntry[], days: number, now: number): Slot[] {
  const e = edges(days, now);
  const slots: Slot[] = e.slice(1).map((end, i) => ({ start: e[i], end, level: null, charging: false, saver: false, awake: new Map(), asleep: 0 }));
  let first = 0;
  rows.forEach((a, k) => {
    const b = rows[k + 1];
    const until = b ? (b.t - a.t <= GAP ? b.t : a.t + 60) : now - a.t <= GAP ? now : a.t + 60;
    while (first < slots.length && slots[first].end <= a.t) first++;
    for (let j = first; j < slots.length && slots[j].start < until; j++) {
      const s = slots[j];
      const secs = Math.min(until, s.end) - Math.max(a.t, s.start);
      if (secs <= 0) continue;
      s.level = a.level;
      s.charging ||= a.charging;
      s.saver ||= !!a.saver;
      if (a.awake) s.awake.set(a.hz, (s.awake.get(a.hz) ?? 0) + secs);
      else s.asleep += secs;
    }
  });
  return slots;
}

/** Seconds awake in a slot, at any rate. */
export const awakeIn = (slot: Slot) => [...slot.awake.values()].reduce((a, b) => a + b, 0);

export interface Look {
  charging: string;
  saver: string;
  stroke: string;
  muted: string;
  ink: string;
  font: string;
}

/** A level marked across the bars, such as the low battery alert's. */
export interface Mark {
  level: number;
  color: string;
}

/** A rectangle with its top corners rounded. */
function topRounded(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  g.beginPath();
  g.roundRect(x, y, w, h, [r, r, 0, 0]);
  g.fill();
}

/** A lightning bolt about 8 × 11 px, centred on (x, y). */
function bolt(g: CanvasRenderingContext2D, x: number, y: number) {
  g.beginPath();
  g.moveTo(x + 1, y - 5.5);
  g.lineTo(x - 3.5, y + 0.8);
  g.lineTo(x - 0.2, y + 0.8);
  g.lineTo(x - 1, y + 5.5);
  g.lineTo(x + 3.5, y - 0.8);
  g.lineTo(x + 0.2, y - 0.8);
  g.closePath();
  g.fill();
}

/**
 * The chart, as picked in the battery chart lab (DECISIONS.md, "Battery chart"): a bar per slot,
 * grey, green while charging or amber under battery saver, fainter for a slot spent mostly asleep;
 * a cap and a bolt over each charge; the baseline dotted where the app wasn't running; `marks` as
 * dashed lines; time labels below. The bar at `hover` stands out.
 */
export function drawChart(g: CanvasRenderingContext2D, { width, slots, days, hover, marks, look }: { width: number; slots: Slot[]; days: number; hover: number | null; marks: Mark[]; look: Look }) {
  const plotW = Math.max(0, width - RIGHT);
  const slotW = plotW / slots.length;
  const base = TOP + PLOT_H;
  const y = (level: number) => TOP + (1 - level / 100) * PLOT_H;
  const barW = Math.max(1, Math.min(slotW - 1, slotW * BAR_WIDTH));
  const barX = (i: number) => i * slotW + (slotW - barW) / 2;

  // The level scale, labelled on the right.
  g.font = `11px ${look.font}`;
  g.textBaseline = "middle";
  g.textAlign = "left";
  g.lineWidth = 1;
  g.fillStyle = look.muted;
  for (const level of [0, 50, 100]) g.fillText(t("{{percent}}%", { percent: num(level) }), plotW + 6, y(level));

  // The baseline: solid under what the log saw, dotted where the app wasn't running.
  for (let i = 0; i < slots.length; ) {
    const seen = slots[i].level != null;
    let last = i;
    while (last + 1 < slots.length && (slots[last + 1].level != null) === seen) last++;
    g.strokeStyle = look.stroke;
    g.setLineDash(seen ? [] : [1, 3]);
    g.beginPath();
    g.moveTo(i * slotW, base + 0.5);
    g.lineTo((last + 1) * slotW, base + 0.5);
    g.stroke();
    i = last + 1;
  }
  g.setLineDash([]);

  // Charging: a cap over each run of bars, with a bolt where there's room.
  g.fillStyle = look.charging;
  for (let i = 0; i < slots.length; i++) {
    if (!slots[i].charging || slots[i - 1]?.charging) continue;
    let last = i;
    while (slots[last + 1]?.charging) last++;
    const x0 = i * slotW + 1;
    const w = (last + 1) * slotW - 1 - x0;
    g.beginPath();
    g.roundRect(x0, 2, w, 3, 1.5);
    g.fill();
    if (w >= 14) bolt(g, x0 + w / 2, 13);
  }

  slots.forEach((slot, i) => {
    if (slot.level == null) return;
    const height = Math.max(2, (slot.level / 100) * PLOT_H);
    const plain = !slot.charging && !slot.saver;
    const awake = awakeIn(slot);
    const share = awake + slot.asleep > 0 ? awake / (awake + slot.asleep) : 1;
    g.fillStyle = slot.charging ? look.charging : slot.saver ? look.saver : look.ink;
    g.globalAlpha = (plain ? 0.4 : 1) * (hover == null || hover === i ? 1 : 0.5) * (0.35 + 0.65 * share);
    topRounded(g, barX(i), base - height, barW, height, Math.min(2, barW / 2));
  });
  g.globalAlpha = 1;

  // Marked levels, over the bars, labelled on the right unless that crowds the scale's labels.
  g.setLineDash([3, 3]);
  for (const m of marks) {
    const my = Math.round(y(m.level)) + 0.5;
    g.strokeStyle = m.color;
    g.globalAlpha = 0.8;
    g.beginPath();
    g.moveTo(0, my);
    g.lineTo(plotW, my);
    g.stroke();
    g.globalAlpha = 1;
    if ([0, 50, 100].every((l) => Math.abs(y(l) - my) >= 11)) {
      g.fillStyle = m.color;
      g.fillText(t("{{percent}}%", { percent: num(m.level) }), plotW + 6, my);
    }
  }
  g.setLineDash([]);

  // Time labels under the bars that start a period: every 6 hours over a day, each day over a
  // week, each week (counted back from today) over a month.
  g.fillStyle = look.muted;
  g.textBaseline = "top";
  const midnights = slots.filter((x) => new Date(x.start * 1000).getHours() === 0).length;
  let midnight = 0;
  slots.forEach((slot, i) => {
    const d = new Date(slot.start * 1000);
    const hour = d.getHours();
    let text: string | null = null;
    if (days <= 1) {
      if (hour % 6 === 0) text = d.toLocaleTimeString(dateLocale(), { hour: "numeric" });
    } else if (hour === 0) {
      midnight++;
      if (days <= 7) text = d.toLocaleDateString(dateLocale(), { weekday: "short" });
      else if ((midnights - midnight) % 7 === 0) text = d.toLocaleDateString(dateLocale(), { day: "numeric", month: "short" });
    }
    if (text && barX(i) + g.measureText(text).width <= width) g.fillText(text, barX(i), base + 6);
  });
}
