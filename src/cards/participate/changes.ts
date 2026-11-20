import { lodName, sensorAngleName, sensorModeName, type Capabilities, type SensorMode, type Settings } from "@/api";
import { dpiRange, stepNear } from "@/dpi";
import { msg, type Speech } from "@/i18n";
import { ENGLISH, SCREEN, type Say, type Voice } from "./report";

const onOff = (t: Say, b: boolean) => (b ? t("On") : t("Off"));
const secs = (t: Say, d: Voice["d"], s: number) => (s >= 60 && s % 60 === 0 ? t("{{minutes}} min", { minutes: d(s / 60) }) : t("{{seconds}} s", { seconds: d(s) }));

/** A change as it's said: the setting, and its value before and after. */
export interface Said {
  what: string;
  from: string;
  to: string;
}

/**
 * One setting the check changes: the settings with it changed (`next`), and where it reads in
 * settings (`read`), to hold what the mouse kept against what was sent and, put back, against what
 * it had. `what`, `from` and `to` are English, for the report; `shown` says them in the language
 * shown, for the screen.
 */
export interface Change {
  id: string;
  what: string;
  from: string;
  to: string;
  shown: () => Said;
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
  /** `say` says the change in a `Voice`: once in English, and on screen when it's shown. */
  const add = (id: string, say: (v: Voice) => Said, patch: Partial<Settings>, read: Change["read"], hz?: number) =>
    out.push({ id, ...say(ENGLISH), shown: () => say(SCREEN), next: { ...s, ...patch }, read, ...(hz && { hz }) });
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
    add("dpi", ({ t, n }) => ({ what: t("Stage {{stage}} DPI", { stage: i + 1 }), from: n(stage.dpi), to: n(dpi) }), { stages: s.stages.map((x, j) => (j === i ? { ...x, dpi } : x)) }, (x) => x.stages?.[i]?.dpi);
    const count = s.dpiStageCount;
    if (count) {
      // One more where there's room; one fewer otherwise, unless that's the stage in use.
      const most = Math.min(caps.dpi.maxStages, s.stages.length);
      const to = count < most ? count + 1 : i < count - 1 ? count - 1 : null;
      if (to) add("stageCount", ({ t, d }) => ({ what: t("DPI stages in use"), from: d(count), to: d(to) }), { dpiStageCount: to }, (x) => x.dpiStageCount);
      if (count > 1 && s.dpiStage !== undefined) {
        const next = (i + 1) % count;
        add("stage", ({ t }) => ({ what: t("DPI stage in use"), from: t("Stage {{stage}}", { stage: i + 1 }), to: t("Stage {{stage}}", { stage: next + 1 }) }), { dpiStage: next }, (x) => x.dpiStage);
      }
    }
    if (caps.dpi.colors) {
      const color = stage.color.toUpperCase() === "#00FF00" ? "#0000FF" : "#00FF00";
      add("color", ({ t }) => ({ what: t("Stage {{stage}} colour", { stage: i + 1 }), from: stage.color, to: color }), { stages: s.stages.map((x, j) => (j === i ? { ...x, color } : x)) }, (x) => x.stages?.[i]?.color?.toUpperCase());
    }
    if (caps.dpi.light && s.dpiLight) {
      const light = s.dpiLight;
      if (light.effect !== "off") {
        add("light", ({ t }) => ({ what: t("DPI light"), from: onOff(t, true), to: onOff(t, false) }), { dpiLight: { ...light, effect: "off" } }, (x) => x.dpiLight?.effect);
      } else {
        // Off, the mouse still keeps an effect, which the settings don't say: switching it on would
        // have to pick one, and off again would leave that one kept (an R1 PRO kept breathing,
        // off, 2026-11-15). Its brightness is in the same row and goes back as it was.
        const steps = caps.dpi.brightnessSteps ?? [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        const brightness = other(steps, light.brightness);
        if (brightness) add("light", ({ t, d }) => ({ what: t("DPI light brightness"), from: d(light.brightness), to: d(brightness) }), { dpiLight: { ...light, brightness } }, (x) => x.dpiLight?.brightness);
      }
    }
  }
  // Only where this connection carries the rate the mouse has set: a Zaopin's cable goes to 1,000 Hz,
  // and the 4,000 its receiver set couldn't be sent back over it, so the mouse was left at 500 and
  // every change after it was refused (a Z2, issue #22, 2026-11-17).
  if (caps.polling && s.pollingHz && caps.polling.rates.includes(s.pollingHz)) {
    // To a rate quick circles can tell from the one set: 500 Hz, or 1,000 from 500.
    const rates = caps.polling.rates;
    const hz = [500, 1000].find((x) => x !== s.pollingHz && rates.includes(x)) ?? other(rates, s.pollingHz);
    const now = s.pollingHz;
    if (hz) add("polling", ({ t, n }) => ({ what: t("Polling rate"), from: t("{{hz}} Hz", { hz: n(now) }), to: t("{{hz}} Hz", { hz: n(hz) }) }), { pollingHz: hz }, (x) => x.pollingHz, hz);
  }
  if (caps.lod?.length && s.lod) {
    const lod = other(caps.lod, s.lod);
    const now = s.lod;
    if (lod) add("lod", ({ t, speech }) => ({ what: t("Lift-off distance"), from: lodName(now, speech), to: lodName(lod, speech) }), { lod }, (x) => x.lod);
  }
  if (caps.sensorMode && s.sensorMode) {
    const sensorMode = other(caps.sensorMode.modes, s.sensorMode);
    const name = (m: SensorMode, speech: Speech) => sensorModeName(m, caps.sensorMode?.cordedByChoice, speech);
    const now = s.sensorMode;
    if (sensorMode) add("sensorMode", ({ t, speech }) => ({ what: t("Sensor mode"), from: name(now, speech), to: name(sensorMode, speech) }), { sensorMode }, (x) => x.sensorMode);
  }
  if (caps.sensorAngleDeg !== undefined && s.sensorAngle) {
    // One degree on, or one back at the end. Whether it's applied stays as it is.
    const { on, degrees } = s.sensorAngle;
    const now = s.sensorAngle;
    const sensorAngle = { on, degrees: degrees < caps.sensorAngleDeg ? degrees + 1 : degrees - 1 };
    add("sensorAngle", ({ t, speech }) => ({ what: t("Sensor angle"), from: sensorAngleName(now, speech), to: sensorAngleName(sensorAngle, speech) }), { sensorAngle }, (x) => x.sensorAngle);
  }
  if (caps.debounceMs && s.debounceMs !== undefined) {
    const { min, max } = caps.debounceMs;
    const now = s.debounceMs;
    const ms = caps.debounceChoicesMs ? other(caps.debounceChoicesMs.filter((x) => x >= min && x <= max), now) : now < max ? now + 1 : now > min ? now - 1 : undefined;
    if (ms !== undefined) add("debounce", ({ t, d }) => ({ what: t("Debounce"), from: t("{{ms}} ms", { ms: d(now) }), to: t("{{ms}} ms", { ms: d(ms) }) }), { debounceMs: ms }, (x) => x.debounceMs);
  }
  for (const [id, what, offered] of [
    ["motionSync", msg("Motion sync"), caps.motionSync],
    ["angleSnapping", msg("Angle snapping"), caps.angleSnapping],
    ["rippleControl", msg("Ripple control"), caps.rippleControl],
  ] as const) {
    const now = s[id];
    if (offered && now !== undefined) add(id, ({ t }) => ({ what: t(what), from: onOff(t, now), to: onOff(t, !now) }), { [id]: !now }, (x) => x[id]);
  }
  if (caps.peakTimeoutsS && s.peakTimeoutS !== undefined) {
    const peakTimeoutS = other(caps.peakTimeoutsS, s.peakTimeoutS);
    const now = s.peakTimeoutS;
    if (peakTimeoutS) add("peakTimeout", ({ t, d }) => ({ what: t("Peak performance timeout"), from: secs(t, d, now), to: secs(t, d, peakTimeoutS) }), { peakTimeoutS }, (x) => x.peakTimeoutS);
  }
  if (caps.sleepTimesS && s.sleepS !== undefined) {
    const sleepS = other(caps.sleepTimesS, s.sleepS);
    const now = s.sleepS;
    if (sleepS) add("sleep", ({ t, d }) => ({ what: t("Sleep after"), from: secs(t, d, now), to: secs(t, d, sleepS) }), { sleepS }, (x) => x.sleepS);
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
  /** `what`, `from` and `to` in the language shown, for the screen; they're English, for the
   *  report. A function, so the report's JSON leaves it out. */
  shown?: () => Said;
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
  /** The change about to be tried, and which one it is: `what`, in the language shown. */
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
    io.doing(n, c.shown().what);
    const w: WriteCheck = { what: c.what, from: c.from, to: c.to, shown: c.shown, kept: false, restored: false, ...(c.hz && { hz: c.hz }) };
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
