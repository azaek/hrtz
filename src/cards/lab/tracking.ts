import { useRef, useState } from "react";
import { api, type MeterSample, type Stroke } from "@/api";
import { useRawInput } from "@/rawInput";
import type { TestResult } from "./result";

// The tracking check, on game mode's screen: whether the sensor holds still on your surface, with
// nothing touching it and under a resting hand, and whether it keeps count at speed. Each part
// moves on by itself. Strokes are cut and measured in Rust (diag.rs); this decides what they say.

/** A still part that sees movement counts this many 100 ms ticks, to say how much. */
export const STILL_TICKS = 100;
/** One that sees nothing at all is done after this many. */
const QUICK_TICKS = 50;
/** Hands off, a still tick is one with no more than this many reports: a hand on the mouse sends
 *  dozens, a sensor creeping by itself a few. Counting starts after this many in a row. */
const QUIET_REPORTS = 2;
const SETTLE_TICKS = 10;
/** Resting a hand on the mouse takes a moment. */
const REST_SETTLE_TICKS = 20;
/** Movement past this (mm) during a check is a hand moving it: the check starts over. */
const MOVED_MM = 2;
/** A swipe between two stops is at least this long (cm). */
const SWIPE_CM = 5;
/** Swipes the check asks for; the slowest three set the length every swipe should have. */
export const SWIPES = 8;
export const SLOW_SWIPES = 3;
/** A swipe this much shorter than the slow ones lost count. */
const SHORT = 0.02;
/** The sensor's rated top speed (750 inches a second). */
const SENSOR_MPS = 19;

export const mmOf = (counts: number, dpi: number) => (counts / dpi) * 25.4;
export const cmOf = (counts: number, dpi: number) => (counts / dpi) * 2.54;
export const mpsOf = (countsPerS: number, dpi: number) => (countsPerS / dpi) * 0.0254;
export const mm = (n: number) => `${n < 1 ? n.toFixed(2).replace(/0$/, "") : n.toFixed(1)} mm`;
export const mps = (n: number) => `${n.toFixed(1)} m/s`;

// ---- Holding still ----------------------------------------------------------------------------------

export type StillPart = "off" | "rest";
export type StillResult = { travel: number; net: number; reports: number };

/** What a still check says: nothing, the sensor drifting, or jitter under a resting hand. */
export function stillFinding(part: StillPart, r: StillResult, dpi: number): { tone: "good" | "warn" | "neutral"; text: string } {
  const moved = mmOf(r.travel, dpi);
  if (r.travel <= 2) return { tone: "good", text: part === "off" ? "No drift." : "Held still under your hand." };
  if (part === "off") return { tone: "warn", text: `Drifted ${mm(moved)} in 10 s. Check for hair or dust on the sensor, or a reflective surface.` };
  // Under a hand, movement that goes nowhere is the sensor jittering; movement that goes somewhere is the hand.
  const wander = mmOf(r.net, dpi);
  return wander < moved / 4 && moved > 0.3
    ? { tone: "warn", text: `Jittered ${mm(moved)} under your hand in 10 s.` }
    : { tone: "neutral", text: `Moved ${mm(moved)}, from your hand rather than the sensor.` };
}

// ---- At speed ---------------------------------------------------------------------------------------

export type Issue = "spin" | "dropout" | "short" | "fast";
export type Swipe = { n: number; speed: number; cm: number; flips: number; dropouts: number; dropoutMs: number };
export type SwipeRow = Swipe & { short: number | null; issue: Issue | null };

export const ISSUE_TEXT: Record<Issue, string> = {
  spin: "spun out",
  dropout: "dropped reports",
  short: "lost count",
  fast: "beyond the sensor's rated speed",
};

/**
 * What the swipes say. The slowest three set the length every swipe between the stops should
 * have; a faster one coming up 2% short lost count. Without stops the lengths wander, and only
 * spin-outs and dropouts are judged.
 */
export function speedFindings(swipes: Swipe[]) {
  const slow = [...swipes].sort((a, b) => a.speed - b.speed).slice(0, 3);
  const lengths = slow.map((x) => x.cm).sort((a, b) => a - b);
  const ref = slow.length === 3 && (lengths[2] - lengths[0]) / lengths[1] <= 0.03 ? lengths[1] : null;
  const slowSpeed = slow.length ? Math.max(...slow.map((x) => x.speed)) : 0;
  const rows: SwipeRow[] = swipes.map((x) => {
    const short = ref === null ? null : 1 - x.cm / ref;
    const issue: Issue | null =
      x.flips >= 2 ? "spin" : x.dropouts > 0 ? "dropout" : short !== null && short > SHORT && x.speed > slowSpeed * 1.3 ? "short" : x.speed > SENSOR_MPS ? "fast" : null;
    return { ...x, short, issue };
  });
  const clean = rows.filter((x) => !x.issue);
  const top = clean.length ? Math.max(...clean.map((x) => x.speed)) : null;
  const trouble = rows.filter((x) => x.issue).sort((a, b) => a.speed - b.speed);
  return { rows, ref, top, trouble, stops: ref !== null || swipes.length < 3 };
}

/** The speed check in a sentence. */
export function speedText(f: ReturnType<typeof speedFindings>, count: number): { tone: "good" | "warn" | "neutral"; text: string } {
  if (count < 4) return { tone: "neutral", text: `${count} of 4 swipes. Faster each time.` };
  const first = f.trouble[0];
  if (first) {
    const at = mps(first.speed);
    const what =
      first.issue === "short"
        ? `came up ${Math.round((first.short ?? 0) * 100)}% short at ${at}, losing count`
        : first.issue === "spin"
          ? `spun out at ${at}, reporting the wrong direction`
          : first.issue === "dropout"
            ? `dropped reports for ${first.dropoutMs.toFixed(1)} ms at ${at}`
            : `went past the sensor's rated ${SENSOR_MPS} m/s`;
    return { tone: "warn", text: f.top ? `Clean up to ${mps(f.top)}, then it ${what}.` : `It ${what}.` };
  }
  if (!f.stops) return { tone: "neutral", text: `Clean up to ${mps(f.top ?? 0)}, with no spin-outs or dropouts. Use two stops to also check the count.` };
  return { tone: "good", text: `Clean up to ${mps(f.top ?? 0)}. Every swipe kept its count.` };
}

// ---- The check ------------------------------------------------------------------------------------

export type Part = "off" | "rest" | "swipes";
export const PARTS: Part[] = ["off", "rest", "swipes"];

/** The part in progress. A still part waits for quiet (`settle`), then counts. */
export type Going = { part: Part; phase: "settle" | "count"; ticks: number; travel: number; x: number; y: number; reports: number; restarted: boolean };

/** What a check measured, and whether it ran to the end or was stopped with something done. */
export type Measured = { off?: StillResult; rest?: StillResult; swipes: Swipe[]; done: boolean };

const fresh = (part: Part): Going => ({ part, phase: "settle", ticks: 0, travel: 0, x: 0, y: 0, reports: 0, restarted: false });

/** How long a still part counts: the full time once something has moved. */
export const stillTicks = (g: Going) => (g.travel > 2 ? STILL_TICKS : QUICK_TICKS);

// Kept for the session, so results outlast switching tests.
let kept: Measured = { swipes: [], done: false };

/**
 * The check: hands off, a hand resting, then swipes between two stops. Each part moves on by
 * itself; a still part with nothing to show is done in 5 s. `stop` keeps what's done.
 */
export function useTrackingCheck(dpi: number) {
  const [measured, setMeasured] = useState(kept);
  const [going, setGoing] = useState<Going | null>(null);
  const cur = useRef<Going | null>(null);
  const moved = (MOVED_MM / 25.4) * dpi;

  const save = (next: Measured) => {
    kept = next;
    setMeasured(kept);
  };
  const end = (keep: boolean) => {
    cur.current = null;
    setGoing(null);
    if (keep) save({ ...kept, done: true });
  };
  const next = (g: Going) => {
    const i = PARTS.indexOf(g.part) + 1;
    if (i >= PARTS.length) return end(true);
    cur.current = fresh(PARTS[i]);
    // Swipes start from nothing: no stroke carried over from a resting hand.
    if (PARTS[i] === "swipes") void api.meterReset();
    setGoing({ ...cur.current });
  };

  useRawInput(true, (s: MeterSample) => {
    const g = cur.current;
    if (!g) return;
    if (g.part === "swipes") {
      const got = s.strokes.filter((x: Stroke) => cmOf(x.reach, dpi) >= SWIPE_CM);
      if (!got.length) return;
      const n = kept.swipes.length;
      const swipes = [
        ...kept.swipes,
        ...got.map((x, i) => ({ n: n + i + 1, speed: mpsOf(x.peak, dpi), cm: cmOf(x.reach, dpi), flips: x.flips, dropouts: x.dropouts, dropoutMs: x.longestDropoutUs / 1000 })),
      ];
      save({ ...kept, swipes });
      if (swipes.length >= SWIPES) end(true);
      return;
    }
    if (g.phase === "settle") {
      // Hands off: wait for a second without a hand on it. A hand: give it a moment to settle.
      const quiet = g.part === "off" ? s.reports <= QUIET_REPORTS : s.travel < moved / 4;
      g.ticks = quiet ? g.ticks + 1 : 0;
      if (g.ticks >= (g.part === "off" ? SETTLE_TICKS : REST_SETTLE_TICKS)) Object.assign(g, { phase: "count", ticks: 0 });
    } else {
      g.ticks += 1;
      g.travel += s.travel;
      g.x += s.dx;
      g.y += s.dy;
      g.reports += s.reports;
      if (g.travel > moved) {
        // A real push: start the part over.
        Object.assign(g, { phase: "settle", ticks: 0, travel: 0, x: 0, y: 0, reports: 0, restarted: true });
      } else if (g.ticks >= stillTicks(g)) {
        save({ ...kept, [g.part]: { travel: g.travel, net: Math.hypot(g.x, g.y), reports: g.reports } });
        return next(g);
      }
    }
    setGoing({ ...g });
  });

  const start = () => {
    save({ swipes: [], done: false });
    cur.current = fresh("off");
    setGoing({ ...cur.current });
  };

  return {
    measured,
    going,
    running: !!going,
    start,
    /** Esc or Stop: what's done is kept, if anything is. */
    stop: () => end(!!(kept.off || kept.rest || kept.swipes.length)),
  };
}

const ISSUE_SHORT: Record<Issue, string> = { spin: "Spun out", dropout: "Dropped reports", short: "Lost count", fast: "Past the sensor's rating" };

/** The Lab list's line: how fast it stayed clean, or where it went wrong, and how it held still. */
export function trackingSummary(results: Partial<Record<StillPart, StillResult>>, swipes: Swipe[], dpi: number): TestResult | null {
  const off = results.off && stillFinding("off", results.off, dpi);
  const rest = results.rest && stillFinding("rest", results.rest, dpi);
  const f = speedFindings(swipes);
  const first = f.trouble[0];
  const speed = first ? `${ISSUE_SHORT[first.issue!]} at ${mps(first.speed)}` : swipes.length >= 4 && f.top ? `Clean to ${mps(f.top)}` : null;
  const still = off ? (off.tone === "warn" ? "Drifts at rest" : "Still at rest") : null;
  const jitter = rest?.tone === "warn" ? "Jitters under a hand" : null;
  if (!speed && !still && !jitter) return null;
  const [value, ...rest_] = [speed, still, jitter].filter((x): x is string => !!x);
  return { value, detail: rest_.join(" · "), tone: [off, rest].some((x) => x?.tone === "warn") || !!first ? "warn" : "good" };
}
