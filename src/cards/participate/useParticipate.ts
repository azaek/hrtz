import { useEffect, useRef, useState } from "react";
import { api, message, type ButtonAction, type CaptureScreen, type DeviceState, type MeterSample, type Settings } from "@/api";
import { useCapture, useHold } from "@/cards/lab/parts";
import { actionLabel } from "@/hid";
import { useRawInput } from "@/rawInput";
import { useStore } from "@/store";

/** Movement counted toward the polling check, and how long each button is waited for. */
const MOVE_MS = 8000;
const BUTTON_MS = 10000;
/** Raw input's button numbers. */
const RAW = { left: 0, right: 1, middle: 2, back: 3, forward: 4 } as const;

export type Stage = "idle" | "reading" | "moving" | "buttons" | "writing" | "done";

/** A button, and whether pressing it was seen: by raw input for a mouse button, by the mouse's
 *  stage or profile changing for one that switches them. `null`: nothing hrtz can see (a key). */
export interface ButtonCheck {
  name: string;
  does: string;
  seen: boolean | null;
}

/** What the check found, for the card and the report. */
export interface Checks {
  read: { ok: boolean; bytes?: number; error?: string };
  polling: { set: number | null; measured: number | null; stutters: number } | null;
  buttons: ButtonCheck[];
  write: { what: string; from: string; to: string; kept: boolean; restored: boolean; error?: string } | null;
}

const median = (xs: number[]) => (xs.length ? [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] : null);
const fmt = (n: number) => n.toLocaleString("en-US");

/** What a button does, and how pressing it shows: a raw button number, "state" (its stage or
 *  profile changes), or nothing to see. */
function seenBy(a: ButtonAction | undefined): number | "state" | null {
  if (a?.kind === "mouse") return RAW[a.button];
  if (a?.kind === "dpi" || a?.kind === "profile") return "state";
  return null;
}

/**
 * The check behind "Help test": on game mode's screen, the mouse's memory read for the report,
 * eight seconds of quick circles (polling), each button it can see pressed in turn, then one DPI
 * stage (or the polling rate) changed, checked and put back. Esc stops it, and what it changed
 * goes back.
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
    original: null as Settings | null,
    profile: null as number | null,
    movedOn: false,
    hz: [] as number[],
    ms: 0,
    stutters: 0,
    rate: null as number | null,
    queue: [] as { name: string; does: string; by: number | "state" }[],
    at: 0,
    since: 0,
    state: "",
  });
  const [, tick] = useState(0);
  const redraw = () => tick((n) => n + 1);

  useHold(stage === "moving");
  useRawInput(stage === "moving" || stage === "buttons", (s: MeterSample) => {
    const r = run.current;
    if (stage === "moving") {
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
    if (i >= r.queue.length) {
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

  /** Puts back what the run changed: the settings it started with, and the profile. */
  const putBack = async () => {
    const r = run.current;
    if (r.original) await api.applySettings(r.original).catch(() => null);
    const now = (await api.states().catch(() => [])).find((d) => d.key === r.key);
    if (r.profile !== null && now && now.profile !== r.profile) await api.setProfile(r.profile).catch(() => null);
  };

  /** One DPI stage up a step (or down, at the top), checked, then put back; the polling rate for a
   *  mouse without DPI stages. */
  const writeCheck = async () => {
    const r = run.current;
    const s = r.original;
    const caps = device?.capabilities;
    let write: Checks["write"] = null;
    try {
      // The DPI button may have moved the stage meanwhile: back to where it started first.
      if (s) await api.applySettings(s);
      if (s?.stages && caps?.dpi) {
        const i = s.dpiStage ?? 0;
        const from = s.stages[i].dpi;
        const to = from + caps.dpi.step <= caps.dpi.max ? from + caps.dpi.step : from - caps.dpi.step;
        const after = await api.applySettings({ ...s, stages: s.stages.map((x, j) => (j === i ? { ...x, dpi: to } : x)) });
        const back = await api.applySettings(s);
        write = {
          what: `Stage ${i + 1} DPI`,
          from: fmt(from),
          to: fmt(to),
          kept: after.stages?.[i]?.dpi === to,
          restored: back.stages?.[i]?.dpi === from,
        };
      } else if (s?.pollingHz && caps?.polling) {
        const from = s.pollingHz;
        const to = caps.polling.rates.find((hz) => hz !== from);
        if (to) {
          const after = await api.applySettings({ ...s, pollingHz: to });
          const back = await api.applySettings(s);
          write = { what: "Polling rate", from: `${fmt(from)} Hz`, to: `${fmt(to)} Hz`, kept: after.pollingHz === to, restored: back.pollingHz === from };
        }
      }
    } catch (e) {
      write = { what: "A setting", from: "", to: "", kept: false, restored: false, error: message(e) };
      await putBack();
    }
    await putBack();
    setChecks((c) => c && { ...c, write });
    setStage("done");
    cap.close();
  };

  const start = async () => {
    if (!device) return;
    const r = run.current;
    const physical = device.capabilities?.buttons ?? [];
    const actions = device.settings?.buttons ?? [];
    const buttons: ButtonCheck[] = physical.map(([i, name]) => ({ name, does: actionLabel(actions[i] ?? { kind: "disabled" }), seen: seenBy(actions[i]) === null ? null : false }));
    Object.assign(run.current, {
      key: device.key ?? null,
      movedOn: false,
      original: device.settings ? structuredClone(device.settings) : null,
      profile: device.profile,
      hz: [],
      ms: 0,
      stutters: 0,
      rate: null,
      queue: physical.flatMap(([i, name]) => {
        const by = seenBy(actions[i]);
        return by === null ? [] : [{ name, does: actionLabel(actions[i]), by }];
      }),
      at: 0,
    });
    setChecks({ read: { ok: false }, polling: null, buttons, write: null });
    setDump(null);
    setStopped(false);
    setStage("reading");
    cap.start(screenOf("reading", r, 0));
    try {
      const d = (await api.mouseReport()) as { memorySize?: number };
      setDump(d);
      setChecks((c) => c && { ...c, read: { ok: true, bytes: d?.memorySize } });
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
    setStage("idle");
    setStopped(true);
    void putBack();
  }, [cap.closed]);

  return { stage, checks, dump, stopped, start, stop: () => cap.close() };
}

/** Game mode's screen at each stage. */
function screenOf(stage: Stage, r: { rate: number | null; ms: number; queue: { name: string; does: string }[]; at: number; since: number }, read: number): CaptureScreen {
  const blank = { figure: "", figureLabel: "", progressLabel: "" };
  switch (stage) {
    case "reading":
      return { ...blank, title: "Reading the mouse", detail: "Its settings and memory, for the report. Keep it still.", progress: read, progressLabel: `${Math.round(read * 100)}%` };
    case "moving":
      return {
        title: "Move the mouse in quick circles",
        detail: "Measures how often it reports, the way a game reads it.",
        figure: r.rate ? fmt(r.rate) : "–",
        figureLabel: r.rate ? "reports a second" : "Start moving",
        progress: r.ms / MOVE_MS,
        progressLabel: `${(r.ms / 1000).toFixed(1)} of ${MOVE_MS / 1000} s counted`,
      };
    case "buttons": {
      const b = r.queue[r.at];
      const left = Math.max(0, BUTTON_MS - (Date.now() - r.since));
      return {
        ...blank,
        title: b ? `Press ${b.name}` : "",
        detail: b ? `It's set to ${b.does}. Once is enough.` : "",
        progress: left / BUTTON_MS,
        progressLabel: `Button ${r.at + 1} of ${r.queue.length} · moves on in ${Math.ceil(left / 1000)} s`,
      };
    }
    default:
      return { ...blank, title: "Checking a change", detail: "hrtz changes one setting, checks the mouse kept it, and puts it back.", progress: 0.5 };
  }
}
