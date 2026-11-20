import { useRef, useState } from "react";
import { api, type MeterSample, type SensorAngle, type Stroke } from "@/api";
import { num, t } from "@/i18n";
import { useRawInput } from "@/rawInput";
import type { TestResult } from "./result";
import { cmOf } from "./tracking";

// The sensor angle check, on game mode's screen: how far a grip tilts sideways swipes, and the
// sensor angle that levels them. Strokes are cut and measured in Rust (diag.rs); this decides
// which count and what they say. The angle is found as ATK's own app finds it (ATK HUB 3.2.32,
// read as text): the main direction of the movement, turned the other way, added to the angle
// that was applied while it was measured.

/** Swipes the check asks for. */
export const SWIPES = 12;
/** Stopped with fewer than this, there's too little to say. */
const FEWEST = 6;
/** A swipe counts from this length (cm)... */
export const SWIPE_CM = 3;
/** ...when it runs sideways, its rise no more than this much of its run. That's 37 degrees: past
 *  the 30 a sensor angle goes to, and short of a diagonal. */
const STEEPEST = 0.75;
/** ...and straight: its path no longer than this times its length. */
const WANDER = 1.25;
/** Swipes that vary by more than this (degrees) make a rough reading. */
export const ROUGH_DEG = 5;

/** A counted swipe: where it ended from where it started, in sensor counts, x right and y down. */
export type Swipe = { dx: number; dy: number };

/** What the swipes say: their tilt in degrees, above 0 when a swipe to the right drops down the
 *  screen, and how much they vary around it. */
export type Reading = { tilt: number; spread: number };

const deg = (rad: number) => (rad * 180) / Math.PI;

/** Whether a stroke is a sideways swipe to count. */
export function counts(s: Pick<Stroke, "dx" | "dy" | "path">, dpi: number) {
  const length = Math.hypot(s.dx, s.dy);
  return cmOf(length, dpi) >= SWIPE_CM && Math.abs(s.dy) <= STEEPEST * Math.abs(s.dx) && s.path <= WANDER * length;
}

/** The tilt of `swipes`: the main direction of all their movement together, so a long swipe
 *  weighs more than a short one, and a swipe to the left counts as its mirror to the right. */
export function reading(swipes: Swipe[]): Reading | null {
  if (!swipes.length) return null;
  let [xx, yy, xy] = [0, 0, 0];
  for (const { dx, dy } of swipes) {
    xx += dx * dx;
    yy += dy * dy;
    xy += dx * dy;
  }
  const tilt = deg(0.5 * Math.atan2(2 * xy, xx - yy));
  const each = swipes.map(({ dx, dy }) => deg(Math.atan(dy / dx)));
  return { tilt, spread: Math.sqrt(each.reduce((sum, t) => sum + (t - tilt) ** 2, 0) / each.length) };
}

/** How far the sensor angle has to turn from where it was to level `tilt`: the tilt the other
 *  way, in whole degrees, as a mouse keeps it. */
export const turnFor = (tilt: number) => Math.round(-tilt) + 0;

/** The angle in effect on a mouse: its degrees while the turn is applied, and none otherwise. */
export const inEffect = (a: SensorAngle | undefined) => (a?.on ? a.degrees : 0);

/** "4°", "-4°". */
export const angleText = (degrees: number) => `${num(degrees)}°`;

export type Measured = {
  swipes: Swipe[];
  done: boolean;
  /** The sensor angle in effect while the swipes were made. */
  applied: number;
  /** The mouse they were made for: a reading is one mouse's, as it's held. */
  mouse: string | null;
};

// Kept for the session, so a result outlasts switching tests.
let kept: Measured = { swipes: [], done: false, applied: 0, mouse: null };

/**
 * The check: sideways swipes, counted until there are enough. `start` takes the sensor angle in
 * effect, which the result adds its turn to, and the mouse it's for. `stop` (Esc) keeps a reading
 * from half of them or more.
 */
export function useAngleCheck(dpi: number) {
  const [measured, setMeasured] = useState(kept);
  const [running, setRunning] = useState(false);
  const live = useRef(false);

  const save = (next: Measured) => {
    kept = next;
    setMeasured(kept);
  };
  const end = (keep: boolean) => {
    live.current = false;
    setRunning(false);
    save(keep ? { ...kept, done: true } : { ...kept, swipes: [], done: false });
  };

  useRawInput(true, (s: MeterSample) => {
    if (!live.current) return;
    const got = s.strokes.filter((x) => counts(x, dpi));
    if (!got.length) return;
    const swipes = [...kept.swipes, ...got.map(({ dx, dy }) => ({ dx, dy }))].slice(0, SWIPES);
    save({ ...kept, swipes });
    if (swipes.length >= SWIPES) end(true);
  });

  return {
    measured,
    running,
    start: (applied: number, mouse: string | null) => {
      save({ swipes: [], done: false, applied, mouse });
      // From nothing: no stroke carried over from before the screen opened.
      void api.meterReset();
      live.current = true;
      setRunning(true);
    },
    stop: () => end(kept.swipes.length >= FEWEST),
  };
}

/**
 * What a reading comes to on this mouse. `max` is how far its sensor angle turns either way, none
 * for a mouse without one; `now` is the angle in effect on it now.
 */
export function verdict(m: Measured, max: number | undefined, now: number) {
  const r = reading(m.swipes);
  if (!m.done || !r) return null;
  const turn = turnFor(r.tilt);
  const want = m.applied + turn;
  /** The angle to set: the one wanted, held to what the mouse has. */
  const set = max === undefined ? null : Math.max(-max, Math.min(max, want));
  return {
    ...r,
    turn,
    want,
    set,
    /** The swipes ran level as they were. */
    level: turn === 0,
    /** The mouse is at the angle to set. */
    isSet: set !== null && set === now,
    rough: r.spread > ROUGH_DEG,
  };
}

/** The Lab list's line. */
export function angleSummary(v: NonNullable<ReturnType<typeof verdict>>, applied: number): TestResult {
  if (v.level) return { value: t("Level"), detail: applied ? t("with the sensor angle at {{angle}}", { angle: angleText(applied) }) : t("no sensor angle needed"), tone: "good" };
  if (v.set === null) return { value: t("{{angle}} tilt", { angle: angleText(Math.abs(v.turn)) }), detail: t("this mouse has no sensor angle"), tone: "neutral" };
  return v.isSet ? { value: angleText(v.set), detail: t("set on the mouse"), tone: "good" } : { value: angleText(v.set), detail: t("levels your swipes"), tone: "neutral" };
}
