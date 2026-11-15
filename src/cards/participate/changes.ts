import type { Capabilities, Settings } from "@/api";
import { dpiRange, stepNear } from "@/dpi";

const fmt = (n: number) => n.toLocaleString("en-US");
const onOff = (b: boolean) => (b ? "On" : "Off");
const secs = (s: number) => (s >= 60 && s % 60 === 0 ? `${s / 60} min` : `${s} s`);

/**
 * One setting the check changes: the settings with it changed (`next`), and where it reads in
 * settings (`read`), to hold what the mouse kept against what was sent and, put back, against what
 * it had.
 */
export interface Change {
  id: string;
  what: string;
  from: string;
  to: string;
  next: Settings;
  read: (s: Settings) => unknown;
  /** A polling rate: the mouse's reports are counted while it's set. */
  hz?: number;
}

/**
 * Every setting hrtz offers on this mouse, each changed to another value it offers: what the check
 * tries, one at a time, each put back before the next. So one check says which of them a model
 * keeps, and a model can be marked tested without going back to its owner for the rest.
 *
 * Only what the mouse's page shows: a setting it doesn't hold (`s`) or doesn't offer (`caps`) is
 * left out. Buttons are left as they are: pressing them is their check.
 */
export function changesFor(s: Settings, caps: Capabilities): Change[] {
  const out: Change[] = [];
  const add = (id: string, what: string, from: string, to: string, patch: Partial<Settings>, read: Change["read"], hz?: number) =>
    out.push({ id, what, from, to, next: { ...s, ...patch }, read, ...(hz && { hz }) });
  /** Another of `list` than `now`: the one after it, or before it, so a setting moves a step and
   *  not to the far end (a sleep time of 30 minutes to 25, not to 30 seconds). */
  const other = <T>(list: readonly T[] | undefined, now: T) => {
    const i = list?.indexOf(now) ?? -1;
    return i < 0 ? list?.find((x) => x !== now) : (list?.[i + 1] ?? list?.[i - 1]);
  };

  if (caps.dpi && s.stages?.length) {
    const i = Math.min(s.dpiStage ?? 0, s.stages.length - 1);
    const stage = s.stages[i];
    const r = dpiRange(caps.dpi);
    const up = stage.dpi + stepNear(r, stage.dpi, 1);
    const dpi = up <= r.max ? up : stage.dpi - stepNear(r, stage.dpi, -1);
    add("dpi", `Stage ${i + 1} DPI`, fmt(stage.dpi), fmt(dpi), { stages: s.stages.map((x, j) => (j === i ? { ...x, dpi } : x)) }, (x) => x.stages?.[i]?.dpi);
    const count = s.dpiStageCount;
    if (count) {
      // One more where there's room; one fewer otherwise, unless that's the stage in use.
      const most = Math.min(caps.dpi.maxStages, s.stages.length);
      const to = count < most ? count + 1 : i < count - 1 ? count - 1 : null;
      if (to) add("stageCount", "DPI stages in use", String(count), String(to), { dpiStageCount: to }, (x) => x.dpiStageCount);
      if (count > 1 && s.dpiStage !== undefined) {
        const next = (i + 1) % count;
        add("stage", "DPI stage in use", `Stage ${i + 1}`, `Stage ${next + 1}`, { dpiStage: next }, (x) => x.dpiStage);
      }
    }
    if (caps.dpi.colors) {
      const color = stage.color.toUpperCase() === "#00FF00" ? "#0000FF" : "#00FF00";
      add("color", `Stage ${i + 1} colour`, stage.color, color, { stages: s.stages.map((x, j) => (j === i ? { ...x, color } : x)) }, (x) => x.stages?.[i]?.color?.toUpperCase());
    }
    if (caps.dpi.light && s.dpiLight) {
      const light = s.dpiLight;
      if (light.effect !== "off") {
        add("light", "DPI light", onOff(true), onOff(false), { dpiLight: { ...light, effect: "off" } }, (x) => x.dpiLight?.effect);
      } else {
        // Off, the mouse still keeps an effect, which the settings don't say: switching it on would
        // have to pick one, and off again would leave that one kept (an R1 PRO kept breathing,
        // off, 2026-11-15). Its brightness is in the same row and goes back as it was.
        const steps = caps.dpi.brightnessSteps ?? [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        const brightness = other(steps, light.brightness);
        if (brightness) add("light", "DPI light brightness", String(light.brightness), String(brightness), { dpiLight: { ...light, brightness } }, (x) => x.dpiLight?.brightness);
      }
    }
  }
  if (caps.polling && s.pollingHz) {
    // To a rate quick circles can tell from the one set: 500 Hz, or 1,000 from 500.
    const rates = caps.polling.rates;
    const hz = [500, 1000].find((x) => x !== s.pollingHz && rates.includes(x)) ?? other(rates, s.pollingHz);
    if (hz) add("polling", "Polling rate", `${fmt(s.pollingHz)} Hz`, `${fmt(hz)} Hz`, { pollingHz: hz }, (x) => x.pollingHz, hz);
  }
  if (caps.lod?.length && s.lod) {
    const lod = other(caps.lod, s.lod);
    if (lod) add("lod", "Lift-off distance", s.lod, lod, { lod }, (x) => x.lod);
  }
  if (caps.sensorMode && s.sensorMode) {
    const sensorMode = other(caps.sensorMode.modes, s.sensorMode);
    if (sensorMode) add("sensorMode", "Sensor mode", s.sensorMode, sensorMode, { sensorMode }, (x) => x.sensorMode);
  }
  if (caps.debounceMs && s.debounceMs !== undefined) {
    const { min, max } = caps.debounceMs;
    const now = s.debounceMs;
    const ms = caps.debounceChoicesMs ? other(caps.debounceChoicesMs.filter((x) => x >= min && x <= max), now) : now < max ? now + 1 : now > min ? now - 1 : undefined;
    if (ms !== undefined) add("debounce", "Debounce", `${now} ms`, `${ms} ms`, { debounceMs: ms }, (x) => x.debounceMs);
  }
  for (const [id, what, offered] of [
    ["motionSync", "Motion sync", caps.motionSync],
    ["angleSnapping", "Angle snapping", caps.angleSnapping],
    ["rippleControl", "Ripple control", caps.rippleControl],
  ] as const) {
    const now = s[id];
    if (offered && now !== undefined) add(id, what, onOff(now), onOff(!now), { [id]: !now }, (x) => x[id]);
  }
  if (caps.peakTimeoutsS && s.peakTimeoutS !== undefined) {
    const peakTimeoutS = other(caps.peakTimeoutsS, s.peakTimeoutS);
    if (peakTimeoutS) add("peakTimeout", "Peak performance timeout", secs(s.peakTimeoutS), secs(peakTimeoutS), { peakTimeoutS }, (x) => x.peakTimeoutS);
  }
  if (caps.sleepTimesS && s.sleepS !== undefined) {
    const sleepS = other(caps.sleepTimesS, s.sleepS);
    if (sleepS) add("sleep", "Sleep after", secs(s.sleepS), secs(sleepS), { sleepS }, (x) => x.sleepS);
  }
  return out;
}

/** Whether two readings of a setting are the same. */
export const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** One change the check tried: whether the mouse kept it, and whether it went back. */
export interface WriteCheck {
  what: string;
  from: string;
  to: string;
  kept: boolean;
  restored: boolean;
  error?: string;
  /** A polling rate set (`hz`): the reports a second counted while it was, null if the mouse
   *  wasn't moved. */
  hz?: number;
  measured?: number | null;
  /** Where it didn't hold: what hrtz last sent the mouse and what it answered, oldest first, so
   *  it can be worked out from the report. */
  said?: string[];
}

/** How much of what was said around a change that didn't hold goes into the report. */
const SAID_LINES = 30;

/** What trying the changes needs: the mouse, and the screen its owner follows. */
export interface Trying {
  /** Sends the mouse these settings; what it holds after. */
  apply: (s: Settings) => Promise<Settings>;
  /** Counts the mouse's reports while this polling rate is set; null if it wasn't moved. */
  count: (hz: number) => Promise<number | null>;
  /** What was last said between hrtz and the mouse, where that's kept. */
  said: () => Promise<string[] | null>;
  /** Its owner stopped the check: nothing more is sent from here. */
  stopped: () => boolean;
  /** The change about to be tried, and which one it is. */
  doing: (n: number, what: string) => void;
}

/** `w` goes into the list; one that didn't hold takes what was said around it with it. */
export async function noted(w: WriteCheck, said: Trying["said"]): Promise<WriteCheck> {
  if (w.error || !w.kept || !w.restored) {
    const lines = await said().catch(() => null);
    if (lines?.length) w.said = lines.slice(-SAID_LINES);
  }
  return w;
}

/**
 * Tries each change of `plan` on the mouse: sent, held against what it then says it keeps, its
 * reports counted where it's a polling rate, and `original` sent again before the next. One that
 * fails doesn't stop the rest: each has its own result. Null if its owner stopped the check, and
 * what's changed is then put back by whoever stopped it.
 */
export async function tryChanges(plan: Change[], original: Settings, io: Trying): Promise<WriteCheck[] | null> {
  const writes: WriteCheck[] = [];
  for (const [n, c] of plan.entries()) {
    if (io.stopped()) return null;
    io.doing(n, c.what);
    const w: WriteCheck = { what: c.what, from: c.from, to: c.to, kept: false, restored: false, ...(c.hz && { hz: c.hz }) };
    try {
      const after = await io.apply(c.next);
      w.kept = same(c.read(after), c.read(c.next));
      if (w.kept && c.hz && !io.stopped()) w.measured = await io.count(c.hz);
    } catch (e) {
      w.error = reason(e);
    }
    if (io.stopped()) return null;
    try {
      const back = await io.apply(original);
      w.restored = same(c.read(back), c.read(original));
    } catch (e) {
      w.error ??= reason(e);
    }
    writes.push(await noted(w, io.said));
  }
  return writes;
}

/** An error as text: a command's comes as a string. */
const reason = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
