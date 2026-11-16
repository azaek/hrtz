import { useEffect, useRef, useState } from "react";
import { api, on, type CaptureInput, type Capabilities, type Settings } from "@/api";
import { hzText } from "@/rates";
import { commit, useStore } from "@/store";
import type { TestResult } from "./result";

// The blind test: can you feel the difference between two settings of your mouse? hrtz sets one or
// the other before each run without saying which, you keep a crosshair on a moving dot, and after
// two runs you say which was which. It runs on game mode's screen (capture.rs): hrtz draws the dot
// and the crosshair itself every frame of the screen, the crosshair moved by the mouse's own reports
// as a game moves its camera, so what you feel is what a game would give you. How closely the
// crosshair followed the dot is measured there too, so a difference you can't name may still show.

/** Rounds in a test, each two runs. */
export const ROUNDS = 10;
/** Rounds that make a result, if you stop early. */
export const FEWEST = 5;
/** One run of following the dot (ms). */
export const RUN_MS = 4000;
/** Before each run, as long whether the mouse needs a write or not, so timing gives nothing away
 *  (ms). A write to an ATK takes up to about half a second. */
export const READY_MS = 1400;

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

type Run = { asked: boolean; miss: number; frames: number };
type Round = { askedFirst: boolean; seed: number; runs: Run[]; right?: boolean };
export type Phase = "ready" | "run" | "answer" | "reveal";

/** A round's answer, shown before the next: right or not, which run was the asked one, and the
 *  score so far. */
export type Reveal = { right: boolean; askedFirst: boolean; score: number; done: number };

export type View = { round: number; run: 0 | 1; phase: Phase; seed: number; pair: Pair; reveal: Reveal | null };

export type Result = {
  pair: Pair;
  rounds: number;
  right: number;
  /** The chance of as many right by guessing. */
  chance: number;
  /** How much closer the crosshair followed the dot with the asked setting, as a share (0.06 is 6%
   *  closer; below 0 is farther), and whether that held round after round. */
  closer: number;
  consistent: boolean;
  /** The screen's refresh rate, as game mode's screen saw it. */
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
 * The test. `start` takes the pair; then up to ten rounds of two runs, each run set up out of
 * sight first. The page shows each part on game mode's screen (`view`); a run's measure comes from
 * there when it ends. Clicks and keys on that screen answer: left or 1 for the first run, right or 2
 * for the second; then the round's answer shows with the score so far, and a click or Enter starts
 * the next (the user's call: "test show test show"). `stop` (Esc) ends it with a result from five
 * rounds on. The user's own settings go back at the end, on `stop`, and if the test goes away
 * mid-way. hrtz's pop-up for a rate change stays quiet throughout.
 */
export function useBlindTest() {
  const device = useStore((x) => x.device);
  const latest = useRef(device);
  latest.current = device;
  const [result, setResult] = useState<Result | null>(kept);
  const [view, setView] = useState<View | null>(null);
  const going = useRef<{
    pair: Pair;
    rounds: Round[];
    i: number;
    run: 0 | 1;
    phase: Phase;
    original: Settings;
    timer?: ReturnType<typeof setTimeout>;
    refreshHz: number | null;
  } | null>(null);

  const show = () => {
    const g = going.current;
    if (!g) return setView(null);
    const round = g.rounds[g.i];
    const done = g.rounds.filter((r) => r.right !== undefined);
    const reveal =
      g.phase === "reveal" && round.right !== undefined
        ? { right: round.right, askedFirst: round.askedFirst, score: done.filter((r) => r.right).length, done: done.length }
        : null;
    setView({ round: g.i, run: g.run, phase: g.phase, seed: round.seed, pair: g.pair, reveal });
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
    if (keep && done.length >= FEWEST) {
      kept = resultOf(g.pair, done, g.refreshHz);
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
        show();
        g.timer = setTimeout(async () => {
          if (going.current !== g) return;
          // How closely the crosshair followed, from game mode's screen, which counted it.
          const measured = await api.captureArenaTake().catch(() => null);
          if (going.current !== g) return;
          round.runs[g.run] = { asked, miss: measured?.miss ?? 0, frames: measured?.frames ?? 0 };
          g.refreshHz = measured?.refreshHz ?? g.refreshHz;
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

  const answer = (first: boolean) => {
    const g = going.current;
    if (!g || g.phase !== "answer") return;
    const round = g.rounds[g.i];
    round.right = first === round.askedFirst;
    g.phase = "reveal";
    show();
  };
  const next = () => {
    const g = going.current;
    if (!g || g.phase !== "reveal") return;
    if (g.i + 1 >= ROUNDS) return end(true);
    g.i += 1;
    g.run = 0;
    void begin();
  };

  // Clicks and keys on game mode's screen.
  useEffect(() => {
    const unlisten = on.captureInput((kind: CaptureInput) => {
      const g = going.current;
      if (g?.phase === "answer") {
        if (kind === "left" || kind === "one") answer(true);
        else if (kind === "right" || kind === "two") answer(false);
      } else if (g?.phase === "reveal" && (kind === "left" || kind === "right" || kind === "next")) next();
    });
    return () => void unlisten.then((f) => f());
  }, []);

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
      going.current = { pair, rounds, i: 0, run: 0, phase: "ready", original: s, refreshHz: null };
      void begin();
    },
    /** Esc, another window in front, or Stop: five rounds or more still make a result. */
    stop: () => end(true),
  };
}

function resultOf(pair: Pair, rounds: Round[], refreshHz: number | null): Result {
  const right = rounds.filter((r) => r.right).length;
  // Per round, how much closer the asked run followed than the other.
  const gains = rounds
    .map((r) => {
      const a = r.runs.find((x) => x.asked);
      const o = r.runs.find((x) => !x.asked);
      if (!a?.frames || !o?.frames || o.miss <= 0) return null;
      return (o.miss - a.miss) / o.miss;
    })
    .filter((x): x is number => x !== null);
  const mean = gains.length ? gains.reduce((n, x) => n + x, 0) / gains.length : 0;
  const sd = gains.length > 1 ? Math.sqrt(gains.reduce((n, x) => n + (x - mean) ** 2, 0) / (gains.length - 1)) : Infinity;
  return {
    pair,
    rounds: rounds.length,
    right,
    chance: byChance(right, rounds.length),
    closer: mean,
    consistent: gains.length >= 5 && Math.abs(mean) > 2 * (sd / Math.sqrt(gains.length)),
    refreshHz,
  };
}
