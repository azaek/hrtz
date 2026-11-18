import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { topRate, type ConnStats, type DeviceState, type GapPattern, type MeterSample } from "@/api";
import { num, t } from "@/i18n";
import { useRawInput } from "@/rawInput";
import { commit, useStore } from "@/store";

// The connection check: 10 s of fast movement at your polling rate makes a run. If the reports
// don't come through clean (stutters, or no more than half the rate would carry), hrtz tries the
// next rate down by itself, as long as that helps, and says which ran best on this PC; your rate
// goes back at the end. Each run knows its setup (the rate, motion sync, and the port the receiver
// is in, read from Windows), so a check after moving the receiver compares by itself. The movement,
// gaps and their pattern are worked out in Rust (diag.rs).

/** A run is this much movement fast enough to fill every poll. */
export const RUN_MS = 10_000;
/** Stutters a minute worth doing something about. */
const STUTTERS_WARN = 2;
/** A rate whose reports fill less than this share of its polls gives no more than half the rate
 *  would, for more battery. */
const HALF = 0.55;
/** A check steps down no lower than this, and tries this many rates at most. */
const FLOOR_HZ = 1000;
const MOST_RATES = 3;
/** Waiting this long for the mouse to take a rate, a check makes do with what it has. */
const SWITCH_MS = 8000;

/** What a run was measured with. */
export type Setup = {
  hz: number;
  motionSync: boolean;
  /** Windows' path for the receiver's port, or null when it isn't known. */
  port: string | null;
  hubs: number;
  wired: boolean;
};

export type ConnRun = { setup: Setup; stats: ConnStats };
/** One check: a run at each rate tried, yours first. */
export type Check = { n: number; at: number; runs: ConnRun[] };

export function setupOf(d: DeviceState | null): Setup | null {
  const s = d?.settings;
  const c = d?.capabilities;
  if (!d || !s || !c || s.pollingHz === undefined) return null;
  const hz = Math.min(s.pollingHz, topRate(c) ?? s.pollingHz);
  return { hz, motionSync: !!s.motionSync, port: d.port?.path ?? null, hubs: d.port?.hubs ?? 0, wired: c.wired };
}

// ---- Checks, kept for the session, so one before a change and one after can be compared --------

let checks: Check[] = [];
/** Ports in the order they were first seen: port 1, port 2. */
const ports: string[] = [];
const subscribers = new Set<() => void>();
const changed = () => subscribers.forEach((f) => f());

export function useChecks(): Check[] {
  return useSyncExternalStore(
    (f) => {
      subscribers.add(f);
      return () => subscribers.delete(f);
    },
    () => checks,
  );
}

export function clearChecks() {
  checks = [];
  changed();
}

function seePort(path: string | null) {
  if (path && !ports.includes(path)) ports.push(path);
}

/** "port 2", or null for a port not seen this session. */
export function portName(path: string | null) {
  const i = path ? ports.indexOf(path) : -1;
  return i < 0 ? null : t("port {{n}}", { n: i + 1 });
}

type Going = { original: number; hz: number; waiting: boolean; since: number; runs: ConnRun[] };

/**
 * A check while it runs. `start` measures your rate and steps down while that helps; the check is
 * kept when it has its answer, and your rate goes back then, on `stop`, or when the test closes.
 * Nothing is counted outside a check.
 */
export function useConnCheck() {
  const device = useStore((x) => x.device);
  const busy = useStore((x) => x.busy);
  const setup = setupOf(device);
  const [live, setLive] = useState<ConnStats | null>(null);
  const [view, setView] = useState<{ hz: number; waiting: boolean; runs: ConnRun[] } | null>(null);
  const going = useRef<Going | null>(null);
  const latest = useRef({ device, busy, setup });
  latest.current = { device, busy, setup };

  const setRate = (hz: number) => {
    const s = latest.current.device?.settings;
    if (s && s.pollingHz !== hz) void commit({ ...s, pollingHz: hz });
  };
  const show = (g: Going | null) => setView(g ? { hz: g.hz, waiting: g.waiting, runs: g.runs } : null);

  const finish = (keep: boolean) => {
    const g = going.current;
    going.current = null;
    if (!g) return;
    setRate(g.original);
    if (keep && g.runs.length) {
      checks = [...checks, { n: checks.length + 1, at: Date.now(), runs: g.runs }];
      changed();
    }
    setLive(null);
    show(null);
  };

  useRawInput(
    true,
    (s: MeterSample) => {
      const g = going.current;
      if (!g) return;
      const { busy, setup } = latest.current;
      if (g.waiting) {
        // Until the mouse has the rate and the run has started over: a new rate starts a new run,
        // and so does the game screen opening. Until then, ticks carry the run before.
        if (Date.now() - g.since > SWITCH_MS) return finish(true);
        if (busy || setup?.hz !== g.hz || s.conn.ms >= RUN_MS) return;
        g.waiting = false;
        show(g);
      }
      if (s.conn.ms < RUN_MS || !setup) {
        setLive(s.conn.ms > 0 ? s.conn : null);
        return;
      }
      seePort(setup.port);
      g.runs = [...g.runs, { setup, stats: s.conn }];
      setLive(null);
      const next = nextRate(g.runs);
      if (next === null) return finish(true);
      g.hz = next;
      g.waiting = true;
      g.since = Date.now();
      setRate(next);
      show(g);
    },
    setup?.hz,
  );

  // Closing the test mid-check puts your rate back.
  useEffect(
    () => () => {
      const g = going.current;
      going.current = null;
      const s = latest.current.device?.settings;
      if (g && s && s.pollingHz !== g.original) void commit({ ...s, pollingHz: g.original });
    },
    [],
  );

  const start = () => {
    const hz = latest.current.setup?.hz;
    if (!hz) return;
    going.current = { original: hz, hz, waiting: true, since: Date.now(), runs: [] };
    setLive(null);
    show(going.current);
  };

  return {
    running: !!view,
    /** The rate being measured, and whether it's still being set. */
    hz: view?.hz ?? null,
    waiting: view?.waiting ?? false,
    /** Runs so far, at the rates already tried. */
    tried: view?.runs ?? [],
    /** The run in progress. */
    live,
    start,
    stop: () => finish(false),
  };
}

// ---- What a run says ------------------------------------------------------------------------------

export const fmt = (n: number) => num(Math.round(n));
/** `n` to a tenth below 10 and whole from there, as `one` shows it: the number a plural goes by. */
const tenths = (n: number) => (n >= 10 ? Math.round(n) : Math.round(n * 10) / 10);
const one = (n: number) => num(tenths(n));
const ms = (us: number) => {
  const digits = us >= 10_000 ? 0 : 1;
  return t("{{ms}} ms", { ms: num(us / 1000, { minimumFractionDigits: digits, maximumFractionDigits: digits }) });
};
/** Items in a sentence: "8,000 Hz and 4,000 Hz". */
const and = (items: string[]) => (items.length ? items.reduce((first, second) => t("{{first}} and {{second}}", { first, second })) : "");

/** Share of polls that carried a report. */
export const fill = (s: ConnStats) => (s.reports + s.missed > 0 ? s.reports / (s.reports + s.missed) : 1);
export const perSecond = (s: ConnStats) => (s.ms > 0 ? (s.reports / s.ms) * 1000 : 0);
export const perMinute = (s: ConnStats) => (s.ms > 0 ? (s.stutters / s.ms) * 60_000 : 0);
export const longest = (s: ConnStats) => (s.longestUs ? ms(s.longestUs) : t("none"));
/** Reports a second the PC got, which is what aiming feels. */
export const got = (r: ConnRun) => Math.min(perSecond(r.stats), r.setup.hz);

/** Gap sizes as the histogram labels them. */
export const GAP_BINS = ["1", "2", "3", "4–7", "8–15", "16+"];

export type Finding = { tone: "good" | "warn" | "neutral"; title: string; detail: string };

const PATTERN: Record<GapPattern, (every: number | null) => string> = {
  few: () => "",
  regular: (every) =>
    every ? t("They recur every {{ms}} ms, which points to the mouse, not your setup.", { ms: one(every) }) : t("They recur steadily, which points to the mouse, not your setup."),
  bursts: () => t("They come in bursts, which points to radio interference."),
  scattered: () => t("They're scattered, which points to the radio or a busy PC."),
};

/** The headline of a run, and a line on what's behind it. */
export function finding(run: ConnRun): Finding {
  const { stats: s, setup } = run;
  const stutters = perMinute(s);
  if (stutters >= STUTTERS_WARN) {
    // Told by the long gaps alone: a radio merging reports leaves many short ones, steadily.
    return {
      tone: "warn",
      title: t("Stutters {{n}} times a minute", { count: tenths(stutters), n: one(stutters) }),
      detail: [PATTERN[s.longPattern](s.longEveryMs), t("The longest gap was {{gap}}.", { gap: longest(s) })].filter(Boolean).join(" "),
    };
  }
  if (fill(s) >= 0.995) {
    return {
      tone: "good",
      title: fmt(got(run)) === fmt(setup.hz) ? t("Every report arrived") : t("{{got}} of {{hz}} reports a second", { got: fmt(got(run)), hz: fmt(setup.hz) }),
      detail: s.stutters ? t("One gap was long enough to notice ({{gap}}).", { gap: longest(s) }) : t("No stutters. The longest gap was {{gap}}.", { gap: longest(s) }),
    };
  }
  if (fill(s) < 0.9) {
    return {
      tone: "neutral",
      title: t("About {{got}} of {{hz}} reports a second", { got: fmt(got(run)), hz: fmt(setup.hz) }),
      detail:
        s.pattern === "regular"
          ? t("The rest merge into the next report, so no movement is lost. It's steady, so this is the most the link or Windows carries at this rate, not interference.")
          : t("The rest merge into the next report, so no movement is lost."),
    };
  }
  return {
    tone: "good",
    title: t("{{got}} of {{hz}} reports a second", { got: fmt(got(run)), hz: fmt(setup.hz) }),
    detail: t("No stutters. The rest merge into the next report, so no movement is lost. The longest gap was {{gap}}.", { gap: longest(s) }),
  };
}

/** A run came through clean: no stutters to speak of, and more than half the rate would carry. */
export const clean = (r: ConnRun) => perMinute(r.stats) < STUTTERS_WARN && got(r) >= r.setup.hz * HALF;

/** Within this share, two runs got as many reports. */
const SAME = 0.05;

/** Which of two runs went better: fewer stutters, then more reports a second. With as many, the
 *  lower rate, which costs less battery. */
export function better(a: ConnRun, b: ConnRun) {
  const [sa, sb] = [perMinute(a.stats), perMinute(b.stats)];
  if (Math.abs(sa - sb) >= 1) return sa < sb ? a : b;
  const [ga, gb] = [got(a), got(b)];
  if (Math.abs(ga - gb) > Math.max(ga, gb) * SAME) return ga > gb ? a : b;
  return a.setup.hz <= b.setup.hz ? a : b;
}

/**
 * The rate to try after `runs`, or null once the check has its answer: the last run came through
 * clean, it's as low as a check goes, or the last step down did no better (then it isn't the rate).
 */
export function nextRate(runs: ConnRun[]): number | null {
  const last = runs[runs.length - 1];
  const hz = last.setup.hz / 2;
  if (clean(last) || hz < FLOOR_HZ || runs.length >= MOST_RATES) return null;
  if (runs.length >= 2 && better(runs[runs.length - 2], last) !== last) return null;
  return hz;
}

/** Why a check stepped down from `run`: "8,000 Hz stuttered 9 times a minute". */
export function stepReason(run: ConnRun) {
  const s = perMinute(run.stats);
  const hz = fmt(run.setup.hz);
  return s >= STUTTERS_WARN
    ? t("{{hz}} Hz stuttered {{n}} times a minute", { hz, count: tenths(s), n: one(s) })
    : t("{{hz}} Hz carried about {{got}} reports a second", { hz, got: fmt(got(run)) });
}

export type Verdict = Finding & {
  /** The rate that ran best. */
  hz: number;
  /** For the list: "4,000 Hz runs best" and a few words. */
  value: string;
  note: string;
};

/** What a check found: the rate that ran best here, and why. */
export function verdict(check: Check): Verdict {
  const runs = check.runs;
  const first = runs[0];
  const pick = runs.reduce(better);
  const hz = pick.setup.hz;
  const [sf, sp] = [perMinute(first.stats), perMinute(pick.stats)];
  const filled = (r: ConnRun) => t("{{percent}}% of polls filled", { percent: Math.round(fill(r.stats) * 100) });
  const stutters = () => t("{{n}} stutters a minute", { count: tenths(sf), n: one(sf) });
  if (pick === first) {
    const value = t("{{hz}} Hz", { hz: fmt(got(first)) });
    if (runs.length === 1) {
      const f = finding(first);
      return { ...f, hz, value, note: sf >= STUTTERS_WARN ? stutters() : f.title === t("Every report arrived") ? f.title : filled(first) };
    }
    // The rates tried after yours, and how many, for the words that agree with them.
    const lower = and(runs.slice(1).map((r) => t("{{hz}} Hz", { hz: fmt(r.setup.hz) })));
    const tried = runs.length - 1;
    if (sf >= STUTTERS_WARN) {
      return {
        tone: "warn",
        title: t("Stutters {{n}} times a minute", { count: tenths(sf), n: one(sf) }),
        detail: [
          t("{{rates}} did no better, so the cause is the radio or the USB port, not the rate.", { rates: lower, count: tried }),
          PATTERN[first.stats.longPattern](first.stats.longEveryMs),
          t("The longest gap was {{gap}}.", { gap: longest(first.stats) }),
        ]
          .filter(Boolean)
          .join(" "),
        hz,
        value,
        note: stutters(),
      };
    }
    return {
      tone: "neutral",
      title: t("About {{got}} of {{hz}} reports a second", { got: fmt(got(first)), hz: fmt(hz) }),
      detail: t("Still more than {{rates}} delivers here ({{got}}), so {{hz}} Hz is the better choice. The rest merge into the next report, so no movement is lost.", {
        rates: lower,
        count: tried,
        got: fmt(got(runs[1])),
        hz: fmt(hz),
      }),
      hz,
      value,
      note: filled(first),
    };
  }
  const why =
    sf - sp >= 1
      ? t("At {{from}} Hz it stuttered {{n}} times a minute; at {{hz}} Hz, {{after}}, with {{got}} reports a second.", {
          from: fmt(first.setup.hz),
          count: tenths(sf),
          n: one(sf),
          hz: fmt(hz),
          after: sp < 0.5 ? t("not once") : t("{{n}} times", { count: tenths(sp), n: one(sp) }),
          got: fmt(got(pick)),
        })
      : t("At {{from}} Hz your PC got about {{fromGot}} reports a second. {{hz}} Hz delivers as many ({{got}}) and uses less battery.", {
          from: fmt(first.setup.hz),
          fromGot: fmt(got(first)),
          hz: fmt(hz),
          got: fmt(got(pick)),
        });
  const still = sp >= STUTTERS_WARN ? ` ${t("Some stutter remains, so the radio or USB port also plays a part.")}` : "";
  return {
    tone: sp >= STUTTERS_WARN ? "warn" : "good",
    title: t("{{hz}} Hz runs best here", { hz: fmt(hz) }),
    detail: why + still,
    hz,
    value: t("{{hz}} Hz runs best", { hz: fmt(hz) }),
    note: sf - sp >= 1 ? t("Fewer stutters") : t("As many reports, less battery"),
  };
}

/** What's different about `b`'s setup from `a`'s, in a few words each. */
export function changes(a: Setup, b: Setup): string[] {
  const out: string[] = [];
  if (a.wired !== b.wired) out.push(b.wired ? t("on its cable") : t("wireless"));
  if (a.hz !== b.hz) out.push(t("{{hz}} Hz", { hz: fmt(b.hz) }));
  if (a.motionSync !== b.motionSync) out.push(b.motionSync ? t("motion sync on") : t("motion sync off"));
  if (a.port !== b.port) out.push(b.hubs > a.hubs ? t("through a hub") : b.hubs < a.hubs ? t("off the hub") : t("another port"));
  return out;
}

/** The setup in a few words: "8,000 Hz, port 1". */
export function setupText(s: Setup) {
  return [t("{{hz}} Hz", { hz: fmt(s.hz) }), s.motionSync && t("motion sync"), s.wired ? t("cable") : portName(s.port), s.hubs > 0 && t("hub")].filter(Boolean).join(", ");
}

/** How `next` went against `prev`: what changed and what it did. */
export function comparison(prev: ConnRun, next: ConnRun): string {
  const what = and(changes(prev.setup, next.setup));
  const [sa, sb] = [perMinute(prev.stats), perMinute(next.stats)];
  const [ga, gb] = [got(prev), got(next)];
  if (Math.abs(sa - sb) >= 1) {
    const v = { changes: what, from: one(sa), to: one(sb) };
    return what ? t("With {{changes}}, stutters went from {{from}} to {{to}} a minute.", v) : t("With the same settings and port, stutters went from {{from}} to {{to}} a minute.", v);
  }
  if (Math.abs(ga - gb) > Math.max(ga, gb) * SAME) {
    const v = { changes: what, from: fmt(ga), to: fmt(gb) };
    return what ? t("With {{changes}}, reports a second went from {{from}} to {{to}}.", v) : t("With the same settings and port, reports a second went from {{from}} to {{to}}.", v);
  }
  return what ? t("With {{changes}}, it ran about the same.", { changes: what }) : t("With the same settings and port, it ran about the same.");
}

/** Something to try that hrtz can't do itself; `game` for what a game wants rather than a fix. */
export type Tip = { key: string; text: string; game?: boolean };

/** What to try after `check`, the most likely to help first: the receiver's place when it still
 *  stutters at the rate that ran best, and what some games want at high rates. `games` are
 *  installed games' exe names in lower case. */
export function tips(check: Check, games: string[]): Tip[] {
  const { stats: s, setup } = check.runs.reduce(better);
  const out: Tip[] = [];
  const troubled = perMinute(s) >= STUTTERS_WARN || s.longPattern === "bursts";
  if (!setup.wired && setup.hubs > 0) out.push({ key: "hub", text: t("The receiver is on a USB hub. Plug it directly into the PC and check again.") });
  if (!setup.wired && troubled) {
    out.push({ key: "near", text: t("Move the receiver within 30 cm of the mouse, with a clear line of sight. A USB extension cable helps.") });
    out.push({ key: "usb3", text: t("Keep it away from USB 3 ports, drives and hubs, which emit noise on the same band.") });
    if (setup.hubs === 0) out.push({ key: "port", text: t("Try another USB port and check again. The results are compared automatically.") });
  }
  if (setup.hz > 1000 && games.includes("valorant-win64-shipping.exe")) {
    out.push({ key: "valorant", game: true, text: t("In Valorant, turn on Raw Input Buffer (Settings > Controls > Mouse). It handles high polling rates better.") });
  }
  if (setup.hz > 1000 && games.includes("fortniteclient-win64-shipping.exe")) {
    out.push({ key: "fortnite", game: true, text: t("Epic recommends 500 to 1,000 Hz for Fortnite. A per-app profile can lower the rate while you play.") });
  }
  return out;
}
