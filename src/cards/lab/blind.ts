import { useEffect, useRef, useState } from "react";
import { api, type Capabilities, type Settings } from "@/api";
import { hzText } from "@/rates";
import { commit, useStore } from "@/store";
import type { TestResult } from "./result";

// The blind test: can you feel the difference between two settings of your mouse? hrtz sets one or
// the other before each run without saying which, you follow a dot with the pointer, and after
// two runs you say which was which. The pointer is Windows' own, which moves with every report the
// mouse sends: that's what a faster polling rate changes on the desktop. How closely the pointer
// followed the dot is measured too, so a difference you can't name may still show.

/** Rounds in a test, each two runs. */
export const ROUNDS = 10;
/** One run of following the dot (ms). */
export const RUN_MS = 4000;
/** Before each run, as long whether the mouse needs a write or not, so timing gives nothing away
 *  (ms). A write to an ATK takes up to about half a second. */
export const READY_MS = 1400;
/** The start of a run, while the hand catches the dot, isn't measured (ms). */
const LEAD_IN_MS = 600;

/** Two values of one setting the mouse has: the one asked about, and the other. */
export type Pair = {
  id: "polling" | "motionSync";
  /** What the choice is called ("Polling rate"). */
  name: string;
  /** The one asked about ("8,000 Hz") and the other ("1,000 Hz"). */
  asked: string;
  other: string;
  /** What you'd feel ("8,000 Hz", "motion sync"), and the question after a round. */
  feel: string;
  question: string;
  /** The slower rate, for a polling rate. */
  low?: number;
  /** `s` with the asked one, or the other. */
  with: (s: Settings, asked: boolean) => Settings;
};

/** What this mouse can be blind-tested on: its fastest polling rate against 1,000 Hz (or half the
 *  fastest, on a mouse that tops out at 1,000), and motion sync where it has it. */
export function pairsFor(caps: Capabilities | null | undefined, s: Settings | null | undefined): Pair[] {
  const out: Pair[] = [];
  const rates = caps?.polling?.rates ?? [];
  if (rates.length >= 2 && s?.pollingHz !== undefined) {
    const top = rates[rates.length - 1];
    const low = top >= 2000 && rates.includes(1000) ? 1000 : rates.includes(top / 2) ? top / 2 : rates[rates.length - 2];
    out.push({
      id: "polling",
      name: "Polling rate",
      asked: hzText(top),
      other: hzText(low),
      feel: hzText(top),
      question: `Which was ${hzText(top)}?`,
      low,
      with: (x, a) => ({ ...x, pollingHz: a ? top : low }),
    });
  }
  if (caps?.motionSync && s?.motionSync !== undefined) {
    out.push({
      id: "motionSync",
      name: "Motion sync",
      asked: "motion sync on",
      other: "motion sync off",
      feel: "motion sync",
      question: "Which had motion sync on?",
      with: (x, a) => ({ ...x, motionSync: a }),
    });
  }
  return out;
}

/** The chance of `k` or more right of `n` by guessing. */
export function byChance(k: number, n: number): number {
  let sum = 0;
  for (let i = k; i <= n; i++) sum += choose(n, i);
  return sum / 2 ** n;
}
const choose = (n: number, k: number) => {
  let c = 1;
  for (let i = 1; i <= k; i++) c = (c * (n - k + i)) / i;
  return c;
};

/** Where the dot is, `t` ms into a run, in a box `w` by `h`: a slow loop that turns back on itself.
 *  Both runs of a round follow the same path, so their misses compare. */
export function dotAt(t: number, seed: number, w: number, h: number): { x: number; y: number } {
  const s = t / 1000;
  return {
    x: w / 2 + w * 0.36 * Math.sin(2 * Math.PI * 0.23 * s + seed),
    y: h / 2 + h * 0.3 * Math.sin(2 * Math.PI * 0.37 * s + seed * 1.7),
  };
}

type Run = { asked: boolean; miss: number; frames: number };
type Round = { askedFirst: boolean; seed: number; runs: Run[]; right?: boolean };
export type Phase = "ready" | "run" | "answer";

export type Result = {
  pair: Pair;
  rounds: number;
  right: number;
  /** The chance of as many right by guessing. */
  chance: number;
  /** How much closer the pointer followed the dot with the asked setting, as a share (0.06 is 6%
   *  closer; below 0 is farther), and whether that held round after round. */
  closer: number;
  consistent: boolean;
  /** The screen's refresh rate, as the runs saw it. */
  refreshHz: number | null;
};

/** What a result says, in a few words, for the Lab's list. */
export function blindSummary(r: Result): TestResult {
  const value = `${r.right} of ${r.rounds} right`;
  if (r.right >= 8) return { value, detail: `can feel ${r.pair.feel}`, tone: "good" };
  return { value, detail: r.chance < 0.2 ? `${r.pair.feel}: maybe` : `${r.pair.feel}: can't tell`, tone: "neutral" };
}

/** The result in a line to paste anywhere. */
export const shareText = (r: Result) =>
  `${r.right} of ${r.rounds} right telling ${r.pair.asked} from ${r.pair.other} in hrtz's blind test` + (r.right >= 8 ? ". I can feel it." : ".");

// The last result, kept for the session.
let kept: Result | null = null;

/**
 * The test. `start` takes the pair; then ten rounds of two runs, each run set up out of sight
 * first. The screen shows the dot and calls `sample` each frame of a run with how far the pointer
 * was from it, and `answer` once both runs of a round are done. The user's own settings go back at
 * the end, on `stop`, and if the test goes away mid-way. hrtz's pop-up for a rate change stays
 * quiet throughout.
 */
export function useBlindTest() {
  const device = useStore((x) => x.device);
  const latest = useRef(device);
  latest.current = device;
  const [result, setResult] = useState<Result | null>(kept);
  const [view, setView] = useState<{ round: number; run: 0 | 1; phase: Phase; seed: number; pair: Pair } | null>(null);
  const going = useRef<{
    pair: Pair;
    rounds: Round[];
    i: number;
    run: 0 | 1;
    phase: Phase;
    runAt: number;
    original: Settings;
    timer?: ReturnType<typeof setTimeout>;
    frames: number[];
  } | null>(null);

  const show = () => {
    const g = going.current;
    setView(g ? { round: g.i, run: g.run, phase: g.phase, seed: g.rounds[g.i].seed, pair: g.pair } : null);
  };

  const putBack = (original: Settings) => {
    const s = latest.current?.settings;
    if (!s) return;
    const back = { ...s, pollingHz: original.pollingHz, motionSync: original.motionSync };
    if (back.pollingHz !== s.pollingHz || back.motionSync !== s.motionSync) void commit(back);
  };

  const end = (keep: boolean) => {
    const g = going.current;
    going.current = null;
    if (!g) return;
    clearTimeout(g.timer);
    putBack(g.original);
    void api.osdQuiet(false);
    const done = g.rounds.filter((r) => r.right !== undefined);
    if (keep && done.length >= 5) {
      kept = resultOf(g.pair, done, g.frames);
      setResult(kept);
    }
    show();
  };

  const begin = async () => {
    const g = going.current;
    if (!g) return;
    g.phase = "ready";
    show();
    const round = g.rounds[g.i];
    const asked = round.askedFirst === (g.run === 0);
    const since = Date.now();
    const s = latest.current?.settings;
    if (s) await commit(g.pair.with(s, asked)).catch(() => {});
    if (going.current !== g) return;
    g.timer = setTimeout(
      () => {
        if (going.current !== g) return;
        g.phase = "run";
        g.runAt = performance.now();
        round.runs[g.run] = { asked, miss: 0, frames: 0 };
        show();
        g.timer = setTimeout(() => {
          if (going.current !== g) return;
          if (g.run === 0) {
            g.run = 1;
            void begin();
          } else {
            g.phase = "answer";
            show();
          }
        }, RUN_MS);
      },
      Math.max(0, READY_MS - (Date.now() - since)),
    );
  };

  // Leaving the test puts everything back.
  useEffect(() => () => end(false), []);

  return {
    result,
    view,
    running: !!view,
    start: (pair: Pair) => {
      const s = latest.current?.settings;
      if (!s || going.current) return;
      void api.osdQuiet(true);
      const rounds = Array.from({ length: ROUNDS }, () => ({ askedFirst: Math.random() < 0.5, seed: Math.random() * 6.28, runs: [] }));
      going.current = { pair, rounds, i: 0, run: 0, phase: "ready", runAt: 0, original: s, frames: [] };
      void begin();
    },
    /** How far the pointer was from the dot this frame (px), and the frame's length (ms). */
    sample: (miss: number, frameMs: number) => {
      const g = going.current;
      if (!g || g.phase !== "run") return;
      if (frameMs > 0 && frameMs < 100) g.frames.push(frameMs);
      if (performance.now() - g.runAt < LEAD_IN_MS) return;
      const run = g.rounds[g.i].runs[g.run];
      if (!run) return;
      run.miss += miss;
      run.frames += 1;
    },
    /** Which run was the asked one. */
    answer: (first: boolean) => {
      const g = going.current;
      if (!g || g.phase !== "answer") return;
      const round = g.rounds[g.i];
      round.right = first === round.askedFirst;
      if (g.i + 1 >= ROUNDS) return end(true);
      g.i += 1;
      g.run = 0;
      void begin();
    },
    /** Esc or Stop: five rounds or more still make a result. */
    stop: () => end(true),
  };
}

function resultOf(pair: Pair, rounds: Round[], frames: number[]): Result {
  const right = rounds.filter((r) => r.right).length;
  // Per round, how much closer the asked run followed than the other.
  const gains = rounds
    .map((r) => {
      const a = r.runs.find((x) => x.asked);
      const o = r.runs.find((x) => !x.asked);
      if (!a?.frames || !o?.frames) return null;
      const am = a.miss / a.frames;
      const om = o.miss / o.frames;
      return om > 0 ? (om - am) / om : null;
    })
    .filter((x): x is number => x !== null);
  const mean = gains.length ? gains.reduce((n, x) => n + x, 0) / gains.length : 0;
  const sd = gains.length > 1 ? Math.sqrt(gains.reduce((n, x) => n + (x - mean) ** 2, 0) / (gains.length - 1)) : Infinity;
  const sorted = [...frames].sort((a, b) => a - b);
  const median = sorted.length ? sorted[Math.floor(sorted.length / 2)] : 0;
  return {
    pair,
    rounds: rounds.length,
    right,
    chance: byChance(right, rounds.length),
    closer: mean,
    consistent: gains.length >= 5 && Math.abs(mean) > 2 * (sd / Math.sqrt(gains.length)),
    refreshHz: median > 0 ? Math.round(1000 / median) : null,
  };
}
