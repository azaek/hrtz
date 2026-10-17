import { useEffect, useRef, useState } from "react";
import type { Lod, MeterSample, Settings } from "@/api";
import { api, currentDpi } from "@/api";
import { useRawInput } from "@/rawInput";
import { commit, useStore } from "@/store";
import { cmOf, mm, mmOf } from "./tracking";

// The lift-off check. The lowest lift-off moves the cursor least when you lift the mouse to
// reposition it, so hrtz sets that first, and you slide, lift and carry the mouse back. If the
// sensor loses track on those slides, the surface needs more height: hrtz moves up a setting and
// you go again. The first that holds is the one. Your own setting goes back afterwards, until you
// pick one.

export const LOD_NAME: Record<Lod, string> = { "0.7mm": "0.7 mm", "1mm": "1 mm", "2mm": "2 mm" };
/** Returns counted at each setting. */
export const RETURNS = 10;
/** A slide is at least this long (cm). */
const SLIDE_CM = 3;
/** Coming back past this share of the slides is sliding back, not the sensor tracking in the air. */
const SLID_BACK = 0.3;
/** Losing track this often, a minute of slow slides, rules a setting out. */
const STALLS_BAD = 1;

export type LodResult = {
  lod: Lod;
  /** Slides that ended with a lift, and their length in counts. */
  returns: number;
  slides: number;
  /** Movement the other way while lifting, carrying and setting down. */
  leak: number;
  slideMs: number;
  /** Times the sensor lost track mid-slide. */
  stalls: number;
  /** Times it came back along the pad instead of through the air. */
  slidBack: number;
};

const fresh = (lod: Lod): LodResult => ({ lod, returns: 0, slides: 0, leak: 0, slideMs: 0, stalls: 0, slidBack: 0 });

/** Lift-off distances this mouse offers, lowest first. */
export const stallsPerMinute = (r: LodResult) => (r.slideMs > 0 ? (r.stalls / r.slideMs) * 60_000 : 0);
export const leakPerLift = (r: LodResult, dpi: number) => (r.returns ? mmOf(r.leak, dpi) / r.returns : 0);
/** The sensor kept track on the slides. */
const held = (r: LodResult) => stallsPerMinute(r) < STALLS_BAD;
const one = (n: number) => (n >= 10 ? String(Math.round(n)) : n.toFixed(1).replace(/\.0$/, ""));

export type Verdict = {
  lod: Lod;
  /** It kept track. When none did, the one that lost track least. */
  held: boolean;
  title: string;
  why: string;
  /** For the list: "0.7 mm lost track". */
  note: string;
};

/** The setting that suits the surface, from those tried lowest first: the first that held, or the
 *  one that lost track least. */
export function recommend(tried: LodResult[], dpi: number): Verdict | null {
  if (!tried.length) return null;
  const pick = tried.find(held) ?? tried.reduce((a, b) => (stallsPerMinute(b) < stallsPerMinute(a) ? b : a));
  const name = LOD_NAME[pick.lod];
  const moved = `lifting moved the cursor ${mm(leakPerLift(pick, dpi))} each time`;
  const lost = (r: LodResult) => `${LOD_NAME[r.lod]} lost track ${one(stallsPerMinute(r))} times a minute on slow slides`;
  if (!held(pick)) {
    const others = tried.filter((r) => r !== pick).map((r) => ` ${lost(r)}.`);
    return {
      lod: pick.lod,
      held: false,
      title: `${name} lost track least`,
      why: `The sensor lost track at every setting on this surface. ${name} did best, at ${one(stallsPerMinute(pick))} times a minute on slow slides, and ${moved}.${others.join("")}`,
      note: "Lost track at every setting",
    };
  }
  const below = tried.slice(0, tried.indexOf(pick));
  if (!below.length) {
    return {
      lod: pick.lod,
      held: true,
      title: `${name} suits your pad`,
      why: `The sensor tracked every slide, and ${moved}. As the lowest setting, it moves the cursor least when you lift.`,
      note: "The lowest held",
    };
  }
  return {
    lod: pick.lod,
    held: true,
    title: `${name} suits your pad`,
    why: `${below.map(lost).join(", and ")}. At ${name} the sensor tracked every slide, and ${moved}.`,
    note: `${below.map((r) => LOD_NAME[r.lod]).join(" and ")} lost track`,
  };
}

type Run = {
  /** The settings to try, lowest first, and the one on now. */
  lods: Lod[];
  i: number;
  original: Lod;
  /** The setting is on the mouse, and counting has begun. */
  ready: boolean;
  /** The way the slides go, from the first. */
  dir: [number, number] | null;
};

// What's been counted, kept apart from any one test's lifetime so it outlasts switching tests: the
// settings tried, in order, and whether the check got to its answer.
let kept: { tried: LodResult[]; done: boolean } = { tried: [], done: false };

/**
 * Runs the check. `start` sets the lowest lift-off and counts returns, moving up a setting while the
 * sensor loses track; the mouse's own setting goes back when it's done, when it's stopped, or when
 * the test closes. Pick one with `use` afterwards.
 */
export function useLiftOff() {
  const device = useStore((x) => x.device);
  const busy = useStore((x) => x.busy);
  const settings = device?.settings ?? null;
  const dpi = currentDpi(settings) ?? 1600;
  // The mouse's own lift-off settings, lowest first.
  const lods = device?.capabilities?.lod ?? [];
  const [state, setState] = useState(kept);
  const [view, setView] = useState<{ lod: Lod | null; i: number; ready: boolean; count: number }>({ lod: null, i: 0, ready: false, count: 0 });
  const run = useRef<Run | null>(null);
  const latest = useRef({ settings, busy, dpi });
  latest.current = { settings, busy, dpi };

  const save = (next: typeof kept) => {
    kept = next;
    setState(kept);
  };

  const set = (lod: Lod) => {
    const s = latest.current.settings;
    if (s && s.lod !== lod) void commit({ ...s, lod } as Settings);
  };

  const show = (r: Run | null, count = 0) => setView(r ? { lod: r.lods[r.i], i: r.i, ready: r.ready, count } : { lod: null, i: 0, ready: false, count: 0 });

  const finish = (done: boolean) => {
    const r = run.current;
    run.current = null;
    if (r) set(r.original);
    if (done) save({ ...kept, done: true });
    show(null);
  };

  useRawInput(true, (s: MeterSample) => {
    const r = run.current;
    if (!r) return;
    const lod = r.lods[r.i];
    const { settings, busy, dpi } = latest.current;
    if (!r.ready) {
      // Each setting starts once the mouse has it, with nothing carried over from the last.
      if (busy || settings?.lod !== lod) return;
      r.ready = true;
      void api.meterReset();
      show(r);
      return;
    }
    const result = { ...(kept.tried[r.i] ?? fresh(lod)) };
    for (const st of s.strokes) {
      const len = Math.hypot(st.dx, st.dy) || 1;
      const way: [number, number] = [st.dx / len, st.dy / len];
      const long = cmOf(st.reach, dpi) >= SLIDE_CM;
      if (!r.dir && long) r.dir = way;
      if (!r.dir) continue;
      const forward = long && way[0] * r.dir[0] + way[1] * r.dir[1] > 0.7;
      const usual = result.returns ? result.slides / result.returns : st.reach;
      if (forward) {
        result.slideMs += (st.endUs - st.startUs) / 1000;
        result.stalls += st.stalls;
        if (st.end === "pause") {
          result.returns += 1;
          result.slides += st.reach;
          result.leak += st.back;
        } else if (st.end === "turn") {
          result.slidBack += 1;
        }
      } else if (st.reach >= usual * SLID_BACK) {
        result.slidBack += 1;
      } else {
        // A little movement between slides: the sensor tracking in the air.
        result.leak += st.path;
      }
    }
    const tried = [...kept.tried];
    tried[r.i] = result;
    save({ tried, done: false });
    if (result.returns < RETURNS) return show(r, result.returns);
    // Ten in. The first setting that holds is the one; otherwise, the next one up.
    if (held(result) || r.i + 1 >= r.lods.length) return finish(true);
    r.i += 1;
    r.ready = false;
    set(r.lods[r.i]);
    show(r);
  });

  // Closing the test mid-way puts the mouse's own setting back.
  useEffect(
    () => () => {
      const r = run.current;
      run.current = null;
      const s = latest.current.settings;
      if (r && s && s.lod !== r.original) void commit({ ...s, lod: r.original });
    },
    [],
  );

  /** From the lowest setting up, from scratch. */
  const start = () => {
    if (!settings?.lod || !lods.length) return;
    save({ tried: [], done: false });
    run.current = { lods, i: 0, original: run.current?.original ?? settings.lod, ready: false, dir: null };
    set(lods[0]);
    show(run.current);
  };

  return {
    lods,
    dpi,
    tried: state.tried,
    running: !!view.lod,
    /** The setting being tried, its place in the order, and the returns so far. */
    lod: view.lod,
    index: view.i,
    ready: view.ready,
    count: view.count,
    verdict: state.done ? recommend(state.tried, dpi) : null,
    current: settings?.lod ?? null,
    start,
    stop: () => finish(false),
    /** Keep `lod` on the mouse. */
    use: (lod: Lod) => set(lod),
  };
}
