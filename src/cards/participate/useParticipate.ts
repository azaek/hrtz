import { useEffect, useRef, useState } from "react";
import { api, message, type ButtonAction, type CaptureScreen, type DeviceState, type MeterSample, type Settings } from "@/api";
import { useCapture, useHold } from "@/cards/lab/parts";
import { actionLabel } from "@/hid";
import { english, msg, num, t } from "@/i18n";
import { useRawInput } from "@/rawInput";
import { useStore } from "@/store";
import { changesFor, noted, tryChanges, type Said, type WriteCheck } from "./changes";
import { ENGLISH, SCREEN, type Voice } from "./report";

export type { WriteCheck };

/** Movement counted toward the polling check, and how long each button is waited for. */
const MOVE_MS = 8000;
const BUTTON_MS = 10000;
/** Movement counted while the check has another polling rate set, and how long it's waited for. */
const RATE_MS = 3000;
const RATE_WAIT_MS = 20000;
/** Raw input's button numbers. */
const RAW = { left: 0, right: 1, middle: 2, back: 3, forward: 4 } as const;

export type Stage = "idle" | "reading" | "moving" | "buttons" | "writing" | "rate" | "done";

/** A button, and whether pressing it was seen: by raw input for a mouse button, by the mouse's
 *  stage or profile changing for one that switches them. `null`: nothing hrtz can see (a key), or
 *  nothing a press would change, and `why` says which. */
export interface ButtonCheck {
  name: string;
  /** What it's set to, in English for the report; `shown` says it in the language shown. */
  does: string;
  shown?: () => string;
  seen: boolean | null;
  why?: string;
}

/** What the check found, for the card and the report. */
export interface Checks {
  /** `settingsOnly`: a model whose memory hrtz doesn't read whole yet, read without it. */
  read: { ok: boolean; bytes?: number; error?: string; settingsOnly?: boolean };
  polling: { set: number | null; measured: number | null; stutters: number } | null;
  buttons: ButtonCheck[];
  /** The first change tried, as reports had it before every setting was tried. */
  write: WriteCheck | null;
  /** Every change tried: each setting hrtz offers on the mouse, then its profile switch. */
  writes?: WriteCheck[];
  /** Its memory once every change was put back, held against how it read at the start: where
   *  they differ ("0x004C"), none if it's as it was. Missing where it couldn't be compared. */
  left?: string[];
  /** Where `left` found it changed, on a mouse whose whole memory can be written back: that the
   *  memory the check first read was written back (`ok`), or why it couldn't be. */
  wroteBack?: { ok: boolean; error?: string };
}

/** Where two readings of a mouse's memory (base64) differ, as addresses; the first few. */
export function differs(before: string, after: string): string[] {
  const [a, b] = [atob(before), atob(after)];
  const at: string[] = [];
  for (let i = 0; i < Math.min(a.length, b.length) && at.length < 16; i++) {
    if (a[i] !== b[i]) at.push(`0x${i.toString(16).toUpperCase().padStart(4, "0")}`);
  }
  return at;
}

const median = (xs: number[]) => (xs.length ? [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] : null);
/** Seconds counted, to a tenth: "3.2". */
const tenths = (s: number) => num(s, { minimumFractionDigits: 1, maximumFractionDigits: 1 });

/** What a button does, and how pressing it shows: a raw button number, "state" (its stage or
 *  profile changes), or nothing to see. */
function seenOn(a: ButtonAction | undefined): number | "state" | null {
  if (a?.kind === "mouse") return RAW[a.button];
  if (a?.kind === "dpi" || a?.kind === "profile") return "state";
  return null;
}

/**
 * The check in Inspect a mouse: on game mode's screen, the mouse's memory read for the report,
 * eight seconds of quick circles (polling), each button it can see pressed in turn, then every
 * setting hrtz offers on the mouse changed, checked and put back, one at a time (`changesFor`),
 * with its reports counted while another polling rate is set, and its profile switched and
 * switched back. One check then says all a model keeps, so its owner isn't asked again for the
 * rest. Esc stops it, and what it changed goes back. With `write: false` (a model hrtz doesn't
 * know yet) nothing is written: it ends after the buttons.
 */
export function useParticipate() {
  const device = useStore((x) => x.device);
  const progress = useStore((x) => x.progress);
  const cap = useCapture();
  const [stage, setStage] = useState<Stage>("idle");
  const [checks, setChecks] = useState<Checks | null>(null);
  const [dump, setDump] = useState<unknown>(null);
  const [stopped, setStopped] = useState(false);
  // The run's working state: what to put back, movement counted, the button waited on.
  const run = useRef({
    key: null as string | null,
    write: true,
    original: null as Settings | null,
    profile: null as number | null,
    movedOn: false,
    hz: [] as number[],
    ms: 0,
    stutters: 0,
    rate: null as number | null,
    queue: [] as { name: string; does: string; by: number | "state"; detail?: string }[],
    at: 0,
    since: 0,
    state: "",
    /** Its memory as the check's first reading found it, to hold the end against. */
    memory: null as string | null,
    /** Esc, or another window in front: the changes stop where they are. */
    stopped: false,
    /** The change being tried, and where it is among them. */
    doing: "",
    n: 0,
    of: 0,
    /** Movement counted at the rate the check set. */
    count: { hz: [] as number[], ms: 0, to: 0, done: null as ((hz: number | null) => void) | null, timer: 0 },
  });
  const [, tick] = useState(0);
  const redraw = () => tick((n) => n + 1);

  /** The count at the rate set ends: with what was counted, or nothing if the mouse wasn't moved. */
  const counted = () => {
    const c = run.current.count;
    const done = c.done;
    c.done = null;
    clearTimeout(c.timer);
    done?.(median(c.hz));
  };
  /** Counts the mouse's reports while `hz` is set: quick circles once more. */
  const countRate = (hz: number) =>
    new Promise<number | null>((resolve) => {
      const r = run.current;
      r.rate = null;
      r.count = { hz: [], ms: 0, to: hz, done: resolve, timer: window.setTimeout(counted, RATE_WAIT_MS) };
      setStage("rate");
    });

  useHold(stage === "moving" || stage === "rate");
  useRawInput(stage === "moving" || stage === "buttons" || stage === "rate", (s: MeterSample) => {
    const r = run.current;
    if (stage === "rate") {
      r.rate = s.held && s.hz ? s.hz : null;
      if (s.held && s.hz) {
        r.count.hz.push(s.hz);
        r.count.ms += 100;
      }
      if (r.count.ms >= RATE_MS) counted();
      redraw();
    } else if (stage === "moving") {
      r.rate = s.held && s.hz ? s.hz : null;
      if (s.held && s.hz) {
        r.hz.push(s.hz);
        r.ms += 100;
        r.stutters += s.stutters;
      }
      if (r.ms >= MOVE_MS && !r.movedOn) {
        r.movedOn = true;
        setChecks((c) => c && { ...c, polling: { set: device?.settings?.pollingHz ?? null, measured: median(r.hz), stutters: r.stutters } });
        nextButton(0);
      }
      redraw();
    } else {
      const want = r.queue[r.at];
      if (want && typeof want.by === "number" && s.buttons.some((b) => b.down && b.button === want.by)) see(true);
    }
  });

  // A button that switches stage or profile shows by the mouse's state changing.
  const stateKey = (d: DeviceState | null) => `${d?.profile}:${d?.settings?.dpiStage}`;
  useEffect(() => {
    const r = run.current;
    if (stage === "buttons" && r.queue[r.at]?.by === "state" && stateKey(device) !== r.state) see(true);
  }, [device]);

  // Each button gets its time, then is noted as not seen.
  useEffect(() => {
    if (stage !== "buttons") return;
    const t = setInterval(() => {
      if (Date.now() - run.current.since >= BUTTON_MS) see(false);
      redraw();
    }, 250);
    return () => clearInterval(t);
  }, [stage]);

  const nextButton = (i: number) => {
    const r = run.current;
    r.at = i;
    r.since = Date.now();
    r.state = stateKey(device);
    if (i >= r.queue.length && !r.write) {
      setStage("done");
      cap.close();
    } else if (i >= r.queue.length) {
      setStage("writing");
      void writeCheck();
    } else setStage("buttons");
  };
  const see = (seen: boolean) => {
    const r = run.current;
    const want = r.queue[r.at];
    if (!want) return;
    setChecks((c) => c && { ...c, buttons: c.buttons.map((b) => (b.name === want.name ? { ...b, seen } : b)) });
    nextButton(r.at + 1);
  };

  /** Puts back what the run changed: the settings it started with, and the profile. Then its
   *  memory, as hrtz holds it after each checked write, is held against the start's (`left`).
   *  Where they differ, the memory the check first read is written back, on a mouse that can take
   *  it whole (`backups`): a setting can fail to go back, as a Z2's 4,000 Hz did over its cable,
   *  which goes to 1,000, leaving it at 500 (issue #22). A run that writes nothing changes
   *  nothing. */
  const putBack = async (): Promise<Pick<Checks, "left" | "wroteBack">> => {
    const r = run.current;
    if (!r.write) return {};
    if (r.original) await api.applySettings(r.original).catch(() => null);
    const now = (await api.states().catch(() => [])).find((d) => d.key === r.key);
    if (r.profile !== null && now && now.profile !== r.profile) await api.setProfile(r.profile).catch(() => null);
    const held = r.memory ? await api.mouseTrace().catch(() => null) : null;
    const left = held?.memory && r.memory ? differs(r.memory, held.memory) : undefined;
    if (!left?.length || !r.memory || !device?.capabilities?.backups) return left ? { left } : {};
    try {
      await api.mousePutBack(r.memory);
      return { left, wroteBack: { ok: true } };
    } catch (e) {
      return { left, wroteBack: { ok: false, error: message(e) } };
    }
  };

  /** Every setting hrtz offers on the mouse, each changed to another value, checked and put back
   *  before the next (`changesFor`); its reports counted while another polling rate is set; then
   *  its profile switched and switched back. One that fails doesn't stop the rest: the report says
   *  which held and, for one that didn't, what was said between hrtz and the mouse. */
  const writeCheck = async () => {
    const r = run.current;
    const s = r.original;
    const caps = device?.capabilities;
    const said = async () => (await api.mouseTrace().catch(() => null))?.said ?? null;
    const plan = s && caps ? changesFor(s, caps) : [];
    const profiles = r.profile !== null && (caps?.profiles ?? 0) > 1 ? (caps?.profiles ?? 0) : 0;
    r.of = plan.length + (profiles ? 1 : 0);
    const doing = (n: number, what: string) => {
      Object.assign(r, { n, doing: what });
      redraw();
    };
    let writes: WriteCheck[] | null = [];
    try {
      // The DPI button may have moved the stage meanwhile: back to where it started first.
      if (s) await api.applySettings(s);
    } catch (e) {
      // Refused whole, so every change would be: said once.
      writes.push(await noted({ what: "A setting", from: "", to: "", kept: false, restored: false, error: message(e) }, said));
    }
    if (s && !writes.length) {
      writes = await tryChanges(plan, s, {
        apply: api.applySettings,
        count: async (hz) => {
          const counted = await countRate(hz);
          if (!r.stopped) setStage("writing");
          return counted;
        },
        said,
        stopped: () => r.stopped,
        doing,
      });
      // Stopped: what it changed goes back from there (`putBack`).
      if (!writes) return;
      if (profiles && r.profile !== null && !r.stopped) {
        const [from, to] = [r.profile, (r.profile + 1) % profiles];
        doing(plan.length, t("Profile"));
        const say = ({ t }: Voice): Said => ({ what: t("Profile"), from: t("Profile {{profile}}", { profile: from + 1 }), to: t("Profile {{profile}}", { profile: to + 1 }) });
        const w: WriteCheck = { ...say(ENGLISH), shown: () => say(SCREEN), kept: false, restored: false };
        const now = async () => (await api.states()).find((d) => d.key === r.key)?.profile;
        try {
          await api.setProfile(to);
          w.kept = (await now()) === to;
          await api.setProfile(from);
          w.restored = (await now()) === from;
        } catch (e) {
          w.error = message(e);
        }
        writes.push(await noted(w, said));
      }
    }
    if (r.stopped) return;
    const back = await putBack();
    const tried = writes;
    setChecks((c) => c && { ...c, write: tried[0] ?? null, writes: tried, ...back });
    setStage("done");
    cap.close();
  };

  const start = async ({ write = true }: { write?: boolean } = {}) => {
    if (!device) return;
    const r = run.current;
    const physical = device.capabilities?.buttons ?? [];
    const actions = device.settings?.buttons ?? [];
    // A simulated mouse has no buttons of its own: the real mouse's clicks stand in, but one seen
    // only by the mouse's stage or profile changing (the DPI button) can't be pressed on it.
    const simulated = !!device.key?.startsWith("sim/");
    // With one DPI stage, a DPI button has no stage to move to (Mayank_j's Z11, 2026-11-12).
    const oneStage = (a: ButtonAction | undefined) => a?.kind === "dpi" && device.settings?.dpiStageCount === 1;
    const seenBy = (a: ButtonAction | undefined) => {
      const by = seenOn(a);
      return (simulated && by === "state") || oneStage(a) ? null : by;
    };
    // Where hrtz sets no buttons yet (an ATK) but follows the DPI button, that one is asked for:
    // seen by the stage changing.
    const dpi: ButtonAction = { kind: "dpi", action: "cycle" };
    const dpiOnly = !physical.length && !!device.capabilities?.dpiButton && !!device.capabilities.dpi && (device.settings?.dpiStageCount ?? 1) > 1 && !simulated;
    const buttons: ButtonCheck[] = dpiOnly
      ? [{ name: "DPI", does: actionLabel(dpi, english), shown: () => actionLabel(dpi), seen: false }]
      : physical.map(([i, name]) => ({
          name,
          does: actionLabel(actions[i] ?? { kind: "disabled" }, english),
          shown: () => actionLabel(actions[i] ?? { kind: "disabled" }),
          seen: seenBy(actions[i]) === null ? null : false,
          ...(oneStage(actions[i]) && { why: msg("only one DPI stage is on, so a press changes nothing") }),
        }));
    Object.assign(run.current, {
      key: device.key ?? null,
      write,
      movedOn: false,
      original: device.settings ? structuredClone(device.settings) : null,
      profile: device.profile,
      hz: [],
      ms: 0,
      stutters: 0,
      rate: null,
      queue: dpiOnly
        ? [{ name: "DPI", does: actionLabel(dpi), by: "state" as const, detail: t("The DPI button: it may be under the mouse. Once is enough.") }]
        : physical.flatMap(([i, name]) => {
            const by = seenBy(actions[i]);
            return by === null ? [] : [{ name, does: actionLabel(actions[i]), by }];
          }),
      at: 0,
      memory: null,
      stopped: false,
      doing: "",
      n: 0,
      of: 0,
    });
    setChecks({ read: { ok: false }, polling: null, buttons, write: null });
    setDump(null);
    setStopped(false);
    setStage("reading");
    cap.start(screenOf("reading", r, 0));
    try {
      const d = (await api.mouseReport()) as { memoryWhole?: boolean; memorySize?: number; memory?: string };
      run.current.memory = d?.memory ?? null;
      setDump(d);
      const read = d?.memoryWhole === false ? { ok: true, settingsOnly: true } : { ok: true, bytes: d?.memorySize };
      setChecks((c) => c && { ...c, read });
    } catch (e) {
      setChecks((c) => c && { ...c, read: { ok: false, error: message(e) } });
    }
    setStage((st) => (st === "reading" ? "moving" : st));
  };

  // The screen follows the run.
  useEffect(() => {
    if (stage === "idle" || stage === "done") return;
    cap.update(screenOf(stage, run.current, progress ? progress[0] / progress[1] : 0));
  });

  // Esc, or another window in front: stopped, and what it changed goes back.
  useEffect(() => {
    if (!cap.closed || cap.closed === "done" || stage === "idle" || stage === "done") return;
    run.current.stopped = true;
    counted();
    setStage("idle");
    setStopped(true);
    void putBack();
  }, [cap.closed]);

  return { stage, checks, dump, stopped, start, stop: () => cap.close() };
}

/** Game mode's screen at each stage. */
function screenOf(
  stage: Stage,
  r: { rate: number | null; ms: number; queue: { name: string; does: string; detail?: string }[]; at: number; since: number; doing: string; n: number; of: number; count: { ms: number; to: number } },
  read: number,
): CaptureScreen {
  const blank = { figure: "", figureLabel: "", progressLabel: "" };
  switch (stage) {
    case "reading":
      return { ...blank, title: t("Reading the mouse"), detail: t("Its settings and memory, for the report. Keep it still."), progress: read, progressLabel: t("{{percent}}%", { percent: num(Math.round(read * 100)) }) };
    case "moving":
      return {
        title: t("Move the mouse in quick circles"),
        detail: t("Measures how often it reports, the way a game reads it."),
        figure: r.rate ? num(r.rate) : "–",
        figureLabel: r.rate ? t("reports a second") : t("Start moving"),
        progress: r.ms / MOVE_MS,
        progressLabel: t("{{counted}} of {{total}} s counted", { counted: tenths(r.ms / 1000), total: num(MOVE_MS / 1000) }),
      };
    case "buttons": {
      const b = r.queue[r.at];
      const left = Math.max(0, BUTTON_MS - (Date.now() - r.since));
      return {
        ...blank,
        title: b ? t("Press {{button}}", { button: b.name }) : "",
        detail: b ? (b.detail ?? t("It's set to {{action}}. Once is enough.", { action: b.does })) : "",
        progress: left / BUTTON_MS,
        progressLabel: t("Button {{n}} of {{total}} · moves on in {{seconds}} s", { n: num(r.at + 1), total: num(r.queue.length), seconds: num(Math.ceil(left / 1000)) }),
      };
    }
    case "rate":
      return {
        title: t("Move the mouse in quick circles again"),
        detail: t("Its polling rate is set to {{hz}} Hz for a moment, to count that it changed.", { hz: num(r.count.to) }),
        figure: r.rate ? num(r.rate) : "–",
        figureLabel: r.rate ? t("reports a second") : t("Start moving"),
        progress: r.count.ms / RATE_MS,
        progressLabel: t("{{counted}} of {{total}} s counted", { counted: tenths(r.count.ms / 1000), total: num(RATE_MS / 1000) }),
      };
    default:
      return {
        ...blank,
        title: t("Checking each setting"),
        detail: r.doing ? t("{{setting}}: changed, checked and put back.", { setting: r.doing }) : t("hrtz changes each setting, checks the mouse kept it, and puts it back."),
        progress: r.of ? r.n / r.of : 0,
        progressLabel: r.of ? t("{{n}} of {{total}}", { n: num(Math.min(r.n + 1, r.of)), total: num(r.of) }) : "",
      };
  }
}
