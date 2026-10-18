import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { currentDpi, topRate, type AutoReason, type BackupMeta, type ButtonAction, type ButtonEvent, type ConnStats, type DeviceState, type GameSensitivity, type KnownMouse, type MeterSample, type PadView, type PointerSettings, type Prefs, type RunningApp, type Settings, type Stroke, type WheelEvent } from "@/api";

/** This PC's data (src/lab/data), read through a glob: it's git-ignored, so a fresh clone has none
 *  and still type-checks. Without it the lab says how to make it. */
const DATA = import.meta.glob<unknown>("../data/*.json", { eager: true, import: "default" });
function data(name: string): unknown {
  const v = DATA[`../data/${name}.json`];
  if (v === undefined) {
    throw new Error(
      `The lab's stand-in needs this PC's data (src/lab/data/${name}.json). With the mouse connected, run in src-tauri: cargo test --lib dump_lab -- --ignored`,
    );
  }
  return v;
}
const appsJson = data("apps");
const backupsJson = data("backups");
const deviceJson = data("device");
const gamesJson = data("games");
const padSimJson = data("pad-sim");
const padYoursJson = data("pad-yours");
const pointerJson = data("pointer");
const prefsJson = data("prefs");
/** Written by the dump since several mice (2026-10-18); before, the mouse's part was in the prefs. */
const miceJson = DATA["../data/mice.json"] as KnownMouse[] | undefined;

// Throwaway, like the labs: a stand-in for the Rust backend, so the real app runs in a browser
// tab on this PC's own data (src/lab/data, written by the `dump_lab_*` tests). Commands answer
// from memory, writes change it, and events go out as the app's would. Raw input comes from the
// browser's own clicks, wheel and pointer while a test listens, plus scripted runs from the lab's
// panel.

export type Backup = BackupMeta & { settings: Settings };

/** The stand-in update's state, from the page's `?update=` (see "plugin:updater|check"). */
const UPDATE = new URLSearchParams(location.search).get("update");

const clone = <T>(x: T): T => structuredClone(x);
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The dumps went through serde_json's sorted maps, which put `button` before `kind`. The app's
 *  own replies put `kind` first, and the app compares actions by their JSON, so put it back. A dump
 *  from before "Sleep after" (2026-10-17) gets the 10 s the mouse comes with. */
const kindFirst = (s: Settings): Settings => ({ ...s, sleepS: s.sleepS ?? 10, buttons: s.buttons?.map(({ kind, ...rest }) => ({ kind, ...rest }) as ButtonAction) });
/** `?mouse=superlight`: a PRO X SUPERLIGHT on its receiver instead of this PC's mouse, as the
 *  Logitech driver reads the simulated one (src-tauri/src/mice/hidpp/sim.rs). */
const SUPERLIGHT: DeviceState = {
  rev: 1,
  key: "hidpp/c094/1a2b3c4d",
  connected: true,
  online: true,
  model: "PRO X SUPERLIGHT",
  cid: 0,
  mid: 0,
  deviceType: 0,
  variant: 1,
  firmware: "MPM 25.01",
  battery: { level: 80, rawLevel: 80, charging: false, millivolts: 3989 },
  profile: 0,
  longRange: null,
  settings: {
    pollingHz: 1000,
    dpiStageCount: 1,
    dpiStage: 0,
    stages: [800, 1200, 1600, 2400, 3200].map((dpi) => ({ dpi, color: "#FFFFFF" })),
  },
  capabilities: {
    wired: false,
    dpi: { min: 100, max: 25600, step: 50, maxStages: 5, colors: false },
    polling: { rates: [125, 250, 500, 1000] },
    motionSync: false,
    angleSnapping: false,
    rippleControl: false,
    profiles: 5,
    battery: true,
    longRange: false,
    pairing: false,
    factoryReset: false,
    backups: true,
  },
} as unknown as DeviceState;
const device = (new URLSearchParams(location.search).get("mouse") === "superlight" ? SUPERLIGHT : deviceJson) as unknown as DeviceState;
/** `?ghub`: G HUB's agent running; `?zaopinapp`: Zaopin's app; `?host`: the stand-in Superlight in
 *  host mode (G HUB's On-board memory mode off). */
const LAB_VENDOR_APPS = [...(new URLSearchParams(location.search).has("zaopinapp") ? ["compx"] : []), ...(new URLSearchParams(location.search).has("ghub") ? ["hidpp"] : [])];
if (new URLSearchParams(location.search).has("host")) SUPERLIGHT.hostMode = true;

export const real = {
  // The receiver's port on this PC, as Windows names it (the dump leaves it to the stand-in).
  device: {
    ...device,
    settings: device.settings && kindFirst(device.settings),
    capabilities: device.capabilities && { ...device.capabilities, sleepTimesS: device.capabilities.sleepTimesS ?? [10, 30, 60, 120] },
    port: device.port ?? { path: "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(11)", hubs: 0 },
  } as DeviceState,
  prefs: (() => {
    const prefs = prefsJson as unknown as Prefs;
    // A dump from before the click log (2026-10-17) counts, as the app does unless told not to.
    return { ...prefs, clickHistory: prefs.clickHistory ?? true };
  })(),
  /** This PC's Zaopin as hrtz knows it, its snapshots' actions with `kind` first. */
  mice: (() => {
    const mice: KnownMouse[] = miceJson ?? [
      {
        key: "compx/7-4",
        model: "Z1 PRO MAX",
        firstSeen: 0,
        lastSeen: 0,
        prefs: prefsJson as unknown as KnownMouse["prefs"],
      },
    ];
    if (device === SUPERLIGHT) mice.push({ key: SUPERLIGHT.key!, model: SUPERLIGHT.model!, firstSeen: 0, lastSeen: 0, prefs: { ...mice[0].prefs, profileNames: [], profileSnapshots: [], rules: [] } } as KnownMouse);
    // `?many`: a long list of mice known from before, to see the switcher scroll.
    if (new URLSearchParams(location.search).has("many")) {
      const away = (key: string, model: string, hoursAgo: number) =>
        ({ key, model, firstSeen: 0, lastSeen: Math.floor(Date.now() / 1000) - hoursAgo * 3600, prefs: { ...mice[0].prefs, rules: [] }, last: null }) as KnownMouse;
      mice.push(
        away("compx/7-3", "Z2 mini", 2),
        away("hidpp/c09b/5e6f7081", "PRO X SUPERLIGHT 2", 5),
        away("compx/7-5", "Z2 MAX", 30),
        away("hidpp/c0a0/0a0b0c0d", "PRO X SUPERLIGHT 2 DEX", 50),
        away("compx/7-1", "ZI PRO", 80),
        away("compx/7-6", "Z2 mini MAX", 200),
      );
    }
    return mice.map((m) => ({
      ...m,
      // What the stand-in's mouse held, for its pages while it's "unplugged" (?away).
      last: m.last ?? (m.key === device.key ? { ...device, settings: device.settings && kindFirst(device.settings) } : null),
      prefs: {
        ...m.prefs,
        profileSnapshots: m.prefs.profileSnapshots.map((x) => x && { ...x, buttons: x.buttons.map(({ kind, ...rest }) => ({ kind, ...rest }) as ButtonAction) }),
      },
    }));
  })(),
  backups: (backupsJson as unknown as { meta: BackupMeta; settings: Settings }[]).map((b) => ({ ...b.meta, key: b.meta.key ?? null, auto: b.meta.auto ?? null, settings: kindFirst(b.settings) })),
  apps: appsJson as { exe: string; name: string; path: string; icon: string | null }[],
  games: gamesJson as unknown as Record<string, GameSensitivity | null>,
  pointer: pointerJson as PointerSettings,
  padYours: padYoursJson as unknown as PadView,
  padSim: padSimJson as unknown as Record<string, PadView>,
};

/** The simulated session's games, with this PC's own install paths, so their icons show, and the
 *  names the app gives them (games.rs). */
const pathOf = (exe: string) => real.apps.find((a) => a.exe.toLowerCase() === exe.toLowerCase())?.path ?? "";
for (const v of Object.values(real.padSim)) {
  for (const a of v.apps) {
    a.path = pathOf(a.exe);
    if (a.name === "VALORANT") a.name = "Valorant";
  }
}

export const fake = {
  device: clone(real.device),
  prefs: clone(real.prefs),
  mice: clone(real.mice),
  backups: clone(real.backups) as Backup[],
  pointer: clone(real.pointer),
  /** Which pad record the lab shows: yours as saved (calibrated, nothing recorded), or the simulator's. */
  pad: "sim" as "yours" | "sim",
  /** Ms a flash read or write takes, as on the mouse (about 35 reads of 200 bytes). */
  flashMs: 1400,
  listeners: new Set<() => void>(),
};
const changed = () => fake.listeners.forEach((l) => l());

let rev = 1;
export function pushDevice(patch: Partial<DeviceState>) {
  fake.device = { ...fake.device, ...patch, rev: ++rev };
  void emit("device-state", fake.device);
  changed();
}
export function pushPrefs(prefs: Prefs) {
  fake.prefs = prefs;
  void emit("prefs", prefs);
  changed();
}

async function progress(total: number) {
  const steps = 12;
  for (let i = 1; i <= steps; i++) {
    await wait(fake.flashMs / steps);
    void emit("device-progress", [Math.round((i / steps) * total), total]);
  }
}

const stamp = (d: Date) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
};
const idOf = (created: string) => `${created.replace(/[-:]/g, "").replace("T", "-")}_backup.json`;

/** A backup of the mouse as it is now. `auto` says why hrtz made it by itself; it keeps ten. */
export async function saveBackup(note: string, auto: AutoReason | null = null): Promise<Backup> {
  await progress(6912);
  const created = stamp(new Date());
  const b: Backup = {
    id: idOf(created),
    created,
    key: fake.device.key ?? null,
    model: fake.device.model ?? "",
    firmware: fake.device.firmware ?? "",
    profile: fake.device.profile,
    note,
    sha256: Math.random().toString(16).slice(2).padEnd(64, "0"),
    settings: clone(fake.device.settings!),
    auto,
  };
  fake.backups = [b, ...fake.backups];
  // As the app does: only the newest ten automatic ones stay.
  const autos = fake.backups.filter((x) => x.auto);
  if (autos.length > 10) fake.backups = fake.backups.filter((x) => !autos.slice(10).includes(x));
  changed();
  return b;
}

/** Writes a backup back to the mouse as the app does: when it differs, the mouse as it is gets
 *  saved first, and the reply names that backup so Undo can put it back. */
export async function restore(id: string, saveFirst = true): Promise<{ written: number; saved: string | null }> {
  const b = fake.backups.find((x) => x.id === id);
  if (!b) throw "That backup is gone.";
  if (JSON.stringify(fake.device.settings) === JSON.stringify(b.settings)) {
    await progress(1);
    return { written: 0, saved: null };
  }
  const saved = saveFirst ? (await saveBackup("", { kind: "restore", note: b.note, created: b.created })).id : null;
  await progress(9);
  pushDevice({ settings: clone(b.settings) });
  return { written: 9, saved };
}

// ---- Raw input ----------------------------------------------------------------------------------

let meterOn = false;
let meterTimer: number | undefined;
const t0 = performance.now();
const us = () => Math.round((performance.now() - t0) * 1000);
const pending = { buttons: [] as ButtonEvent[], wheel: [] as WheelEvent[], counts: 0, dx: 0, dy: 0, held: [0, 0] as [number, number] };

/** Browser buttons (0 left, 1 middle, 2 right) in raw input's order (0 left, 1 right, 2 middle). */
const RAW_BUTTON = [0, 2, 1, 3, 4];
const DPI = () => currentDpi(fake.device.settings) ?? 1600;
const counts = (cm: number) => (cm / 2.54) * DPI();
const countsPerS = (mps: number) => (mps / 0.0254) * DPI();
const setHz = () => Math.min(fake.device.settings?.pollingHz ?? 1000, topRate(fake.device.capabilities) ?? 8000);

/**
 * The world around the mouse, as the lab's panel sets it: where the receiver is, and what the
 * mouse and its sensor do. The connection runs and strokes follow from it.
 */
export const env = {
  /** The receiver far from the mouse, or behind something: interference comes in bursts. */
  far: false,
  /** The mouse's radio carries about 3,600 reports a second; the rest merge into the next. */
  merged: true,
  /** The sensor spins out on fast swipes over 3 m/s. */
  spin: false,
  /** The sensor creeps while nobody touches it. */
  drift: false,
  /** hrtz is behind another window: Windows throttles its input, and timing waits. */
  behind: false,
};
/** A timing test is open: its timing counts only in game mode. */
let holdWanted = false;
/** Game mode's screen is up (the stand-in has none to show; Esc in the page closes it). */
let captured = false;

/** Plug the receiver into port `n`, through a hub or not: the app sees it reconnect there. */
export function plugInto(n: number, hub: boolean) {
  pushDevice({ port: { path: `PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(${n})${hub ? "#USB(2)" : ""}`, hubs: hub ? 1 : 0 } });
}

// The connection run, as Rust keeps it: only movement fast enough to fill every poll counts.
const blank = (): ConnStats => ({ ms: 0, reports: 0, missed: 0, gaps: [0, 0, 0, 0, 0, 0], stutters: 0, longestUs: 0, pattern: "few", everyMs: null, longPattern: "few", longEveryMs: null });
let conn = blank();
/** The rate the run is measured against: a new one starts a new run, as in Rust. */
let connHz = 0;
/** What made the gaps, for the pattern. */
const causes = { merged: 0, bursts: 0, scattered: 0 };

/** A 100 ms tick of fast movement: polls that came without a report, and the stutters among them. */
function connTick(): number {
  const hz = setHz();
  if (hz !== connHz) {
    resetConn();
    connHz = hz;
  }
  const polls = hz / 10;
  const add = (missed: number, times = 1) => {
    if (times <= 0) return;
    const bin = missed <= 3 ? missed - 1 : missed <= 7 ? 3 : missed <= 15 ? 4 : 5;
    conn.gaps[bin] += times;
    conn.missed += missed * times;
    conn.longestUs = Math.max(conn.longestUs, Math.round(((missed + 1) / hz) * 1e6));
  };
  let stutters = 0;
  // Merging: a gap of a poll, now and then two, every other report or so.
  const wired = fake.device.capabilities?.wired;
  const merged = env.merged && !wired ? Math.max(0, polls - 360 * (0.97 + Math.random() * 0.06)) : 0;
  if (merged > 0) {
    const twos = Math.round(merged * 0.15);
    add(1, Math.round(merged - 2 * twos));
    add(2, twos);
    causes.merged += merged;
  }
  // Interference, with the receiver far off: a burst now and then, with a long gap in it.
  if (env.far && !wired && Math.random() < 0.08) {
    for (let i = 0; i < 14; i++) add(1 + Math.floor(Math.random() * 3));
    if (Math.random() < 0.5) {
      add(Math.round(hz * (0.002 + Math.random() * 0.004)));
      stutters += 1;
    }
    causes.bursts += 20;
  }
  // A hub: a long gap here and there.
  if ((fake.device.port?.hubs ?? 0) > 0 && Math.random() < 0.05) {
    add(Math.round(hz * 0.0025));
    stutters += 1;
    causes.scattered += 5;
  }
  // And very rarely, a poll anywhere.
  if (Math.random() < 0.02) add(1);
  conn.stutters += stutters;
  conn.ms += 100;
  conn.reports = Math.round((conn.ms / 1000) * hz - conn.missed);
  const gaps = conn.gaps.reduce((a, b) => a + b, 0);
  const top = Math.max(causes.merged, causes.bursts, causes.scattered);
  conn.pattern = gaps < 8 ? "few" : top === causes.bursts ? "bursts" : top === causes.merged ? "regular" : "scattered";
  conn.everyMs = conn.pattern === "regular" ? conn.ms / gaps : null;
  // The long gaps, told apart from the merging: bursts from interference, scattered from a hub.
  const long = conn.gaps[3] + conn.gaps[4] + conn.gaps[5];
  conn.longPattern = long < 8 ? "few" : causes.bursts >= causes.scattered ? "bursts" : "scattered";
  return stutters;
}

function resetConn() {
  conn = blank();
  causes.merged = causes.bursts = causes.scattered = 0;
}

// Strokes from the browser's pointer, cut as Rust cuts them: at a stop of 150 ms or a turn of 3 mm.
// A hand moves a browser pointer far slower than a mouse in a game, so speeds read 10 times higher.
const BOOST = 10;
let reports: { t: number; dx: number; dy: number }[] = [];
let strokesDone: Stroke[] = [];

function stroke(r: { t: number; dx: number; dy: number }[], end: Stroke["end"]): Stroke {
  let [x, y, far, farAt, path] = [0, 0, 0, 0, 0];
  r.forEach((q, i) => {
    x += q.dx;
    y += q.dy;
    path += Math.hypot(q.dx, q.dy);
    if (Math.hypot(x, y) > far) [far, farAt] = [Math.hypot(x, y), i];
  });
  const [fx, fy] = r.slice(0, farAt + 1).reduce(([a, b], q) => [a + q.dx, b + q.dy], [0, 0]);
  const dir = far ? [fx / far, fy / far] : [1, 0];
  const back = r.reduce((a, q) => a + Math.max(0, -(q.dx * dir[0] + q.dy * dir[1])), 0);
  let peak = 0;
  for (let i = 0, j = 0, sum = 0; i < r.length; i++) {
    sum += Math.hypot(r[i].dx, r[i].dy);
    while (r[j].t <= r[i].t - 30) sum -= Math.hypot(r[j].dx, r[j++].dy);
    peak = Math.max(peak, (sum / 0.03) * BOOST);
  }
  const flips = env.spin && peak > countsPerS(3) ? 3 : 0;
  return { startUs: r[0].t * 1000, endUs: r[r.length - 1].t * 1000, dx: x, dy: y, reach: far, path, back, peak, flips, dropouts: 0, longestDropoutUs: 0, stalls: 0, end };
}

function pointerStroke(t: number, dx: number, dy: number) {
  const last = reports[reports.length - 1];
  if (last && t - last.t >= 150) {
    strokesDone.push(stroke(reports, "pause"));
    reports = [];
  }
  reports.push({ t, dx, dy });
  let [x, y, far, farAt] = [0, 0, 0, 0];
  reports.forEach((q, i) => {
    x += q.dx;
    y += q.dy;
    if (Math.hypot(x, y) > far) [far, farAt] = [Math.hypot(x, y), i];
  });
  if (far - Math.hypot(x, y) > counts(0.3)) {
    strokesDone.push(stroke(reports.slice(0, farAt + 1), "turn"));
    reports = reports.slice(farAt + 1);
  }
}

function setupInput() {
  window.addEventListener("pointermove", (e) => {
    if (!meterOn) return;
    // About 1,600 DPI at 100% scaling with the pointer at 6 of 11: one count a CSS pixel, times
    // the fraction a real hand moves faster than the pointer shows in a browser.
    const dx = e.movementX * 4;
    const dy = e.movementY * 4;
    pending.counts += Math.abs(dx) + Math.abs(dy);
    pending.dx += dx;
    pending.dy += dy;
    pending.held[0] += dx;
    pending.held[1] += dy;
    if (dx || dy) pointerStroke(performance.now(), dx, dy);
  });
  window.addEventListener("mousedown", (e) => {
    if (!meterOn) return;
    pending.held = [0, 0];
    pending.buttons.push({ button: RAW_BUTTON[e.button] ?? e.button, down: true, tUs: us(), dx: 0, dy: 0 });
  });
  window.addEventListener("mouseup", (e) => {
    if (!meterOn) return;
    pending.buttons.push({ button: RAW_BUTTON[e.button] ?? e.button, down: false, tUs: us(), dx: pending.held[0], dy: pending.held[1] });
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && captured) {
      captured = false;
      void emit("capture-closed", "esc");
    }
  });
  window.addEventListener(
    "wheel",
    (e) => {
      if (!meterOn || !e.deltaY) return;
      // Browsers give pixels; a notch is about 100. Raw input's notch is 120, positive away from you.
      pending.wheel.push({ delta: -Math.sign(e.deltaY) * 120, horizontal: false, tUs: us() });
    },
    { passive: true },
  );
}

/** A scripted sample: anything it leaves out is a still mouse. `fast` marks movement that fills
 *  every poll, which the connection run counts. */
export type Scripted = Partial<MeterSample> & { fast?: boolean };
/** Something the panel set going: a sample per tick until it returns null. */
type Player = () => Scripted | null;
const players: Player[] = [];

function tick() {
  const hz = setHz();
  let sample: MeterSample;
  let fast = false;
  let scripted: Scripted | null = null;
  while (players.length && !(scripted = players[0]())) players.shift();
  if (scripted) {
    fast = !!scripted.fast;
    sample = { hz: null, countsPerS: null, dx: 0, dy: 0, reports: 0, travel: 0, stutters: 0, conn, strokes: [], buttons: [], wheel: [], ...scripted, foreground: !env.behind, held: false };
    if (fast) {
      const { fast: _, ...now } = circle();
      Object.assign(sample, now);
    }
  } else {
    const cps = pending.counts * 10;
    // Quick circles in a browser count as movement that fills every poll.
    fast = cps >= 6000;
    // Reports a second: the set rate when the movement fills every poll, less when it can't.
    const rate = cps ? Math.round(Math.min(hz * (0.994 + Math.random() * 0.005), cps / 1.25)) : null;
    const drift = env.drift && !cps && Math.random() < 0.15 ? 2 : 0;
    const now = performance.now();
    if (reports.length && now - reports[reports.length - 1].t >= 150) {
      strokesDone.push(stroke(reports, "pause"));
      reports = [];
    }
    sample = {
      hz: rate,
      countsPerS: cps || null,
      dx: pending.dx + drift,
      dy: pending.dy,
      reports: cps ? Math.max(1, Math.round((rate ?? 0) / 10)) : drift ? 1 : 0,
      travel: pending.counts + drift,
      stutters: 0,
      conn,
      strokes: strokesDone,
      buttons: pending.buttons,
      wheel: pending.wheel,
      foreground: !env.behind,
      held: false,
    };
    strokesDone = [];
  }
  if (env.behind) {
    // As Rust does: no rate, run or strokes while behind another window.
    sample = { ...sample, hz: null, countsPerS: null, strokes: [] };
    fast = false;
  }
  // As Rust does: with a timing test open, timing counts only in game mode, where the pointer is held.
  sample.held = captured;
  if (holdWanted && !captured) fast = false;
  if (fast) {
    const stutters = connTick();
    if (!scripted) {
      sample.stutters = stutters;
      // The rate the merging allows, as the PC would see it.
      if (sample.hz !== null && env.merged && !fake.device.capabilities?.wired) sample.hz = Math.min(sample.hz, 3600 * (0.97 + Math.random() * 0.06));
    }
  }
  sample.conn = { ...conn, gaps: [...conn.gaps] as ConnStats["gaps"] };
  pending.buttons = [];
  pending.wheel = [];
  pending.counts = pending.dx = pending.dy = 0;
  void emit("meter", sample);
  // Scripts play ten times faster than life: the tests count ticks, not seconds.
  const want = players.length ? 10 : 100;
  if (want !== tickMs) restartTimer(want);
}

let tickMs = 100;
function restartTimer(ms: number) {
  tickMs = ms;
  if (meterTimer !== undefined) window.clearInterval(meterTimer);
  meterTimer = window.setInterval(tick, ms);
}

function meter(on: boolean) {
  meterOn = on;
  if (on && meterTimer === undefined) restartTimer(players.length ? 10 : 100);
  if (!on && meterTimer !== undefined) {
    window.clearInterval(meterTimer);
    meterTimer = undefined;
    players.length = 0;
  }
  if (on) resetConn();
}

/** Queue samples for the next ticks (the panel's "make something happen"). */
export function play(samples: Scripted[]) {
  const queue = [...samples];
  players.push(() => queue.shift() ?? null);
  if (meterOn && tickMs !== 10) restartTimer(10);
}

/** Set a player going: a sample a tick until it returns null. */
function run(p: Player) {
  players.push(p);
  if (meterOn && tickMs !== 10) restartTimer(10);
}

/** Three runs across and three up and down on a 10 cm ruler, reading 1.2% high, one slow and one
 *  fast each way. */
export function dpiRuns(): Scripted[] {
  const dpi = DPI();
  const out: Scripted[] = [];
  let t = us();
  [0.4, 0.9, 1.6, 0.5, 1.0, 1.8].forEach((secs, i) => {
    const c = (dpi * 1.012 * (1 + (Math.random() - 0.5) * 0.006) * 10) / 2.54;
    const across = i < 3;
    const press: ButtonEvent = { button: 0, down: true, tUs: t, dx: 0, dy: 0 };
    t += secs * 1e6;
    const lift: ButtonEvent = { button: 0, down: false, tUs: t, dx: across ? Math.round(c) : 12, dy: across ? -9 : Math.round(c) };
    t += 0.6e6;
    out.push({ buttons: [press] }, { buttons: [lift] }, {});
  });
  return out;
}

/** Twelve clicks on Left, one of which bounces 9 ms after letting go. */
export function doubleClick(): Scripted[] {
  const out: Scripted[] = [];
  let t = us();
  for (let i = 0; i < 12; i++) {
    const ev: ButtonEvent[] = [
      { button: 0, down: true, tUs: t, dx: 0, dy: 0 },
      { button: 0, down: false, tUs: t + 70_000, dx: 0, dy: 0 },
    ];
    if (i === 7) ev.push({ button: 0, down: true, tUs: t + 79_000, dx: 0, dy: 0 }, { button: 0, down: false, tUs: t + 95_000, dx: 0, dy: 0 });
    t += 300_000;
    out.push({ buttons: ev });
  }
  return out;
}

/** Twenty notches down and twenty up at a steady pace, with one read the wrong way. */
export function scrollNotches(): Scripted[] {
  const out: Scripted[] = [];
  let t = us();
  for (const dir of [-1, 1]) {
    for (let i = 0; i < 20; i += 4) {
      const wheel: WheelEvent[] = [];
      for (let j = 0; j < 4; j++) {
        const wrong = dir < 0 && i + j === 13;
        wheel.push({ delta: (wrong ? -dir : dir) * 120, horizontal: false, tUs: t });
        t += 45_000;
      }
      out.push({ wheel });
    }
  }
  return out;
}

/** A tick of fast circles at the rate the mouse has now: movement that fills every poll, as far
 *  as the link allows. */
function circle(): Scripted {
  const hz = setHz();
  const got = env.merged && !fake.device.capabilities?.wired ? Math.min(hz, 3600) : hz;
  return {
    fast: true,
    hz: Math.round(got * (0.993 + Math.random() * 0.006)),
    countsPerS: hz * (2.2 + Math.random() * 0.6),
    dx: 600,
    dy: -300,
    reports: Math.round(got / 10),
    travel: hz * 0.25,
  };
}

/** `secs` of fast circles. */
export function movement(secs = 3): Scripted[] {
  return Array.from({ length: secs * 10 }, circle);
}

/** Quick circles for as long as game mode's screen is up (a minute at most): a connection check
 *  from start to finish, rate changes and all. */
export function circlesAlong() {
  let ticks = 0;
  run(() => (captured && ticks++ < 600 ? circle() : null));
}

/** Let go of the mouse for `secs`: nothing, or a creep with `env.drift`. */
export function stillFor(secs: number, hand = false): Scripted[] {
  return Array.from({ length: secs * 10 }, (_, i) => {
    const creep = env.drift && i % 7 === 3 ? 3 : 0;
    const shake = hand && i % 4 === 1 ? (i % 8 === 1 ? 2 : -2) : 0;
    return { dx: creep + shake, travel: Math.abs(creep) + Math.abs(shake), reports: creep || shake ? 1 : 0 };
  });
}

/** Swipes of `cm` between two stops, back and forth, at these peak speeds (m/s). A spin-out with
 *  `env.spin` over 3 m/s, and the swipe after it coming up 3% short. */
export function swipes(speeds = [0.5, 0.7, 0.9, 1.4, 2.0, 2.6, 3.3, 3.9], cm = 12): Scripted[] {
  let t = us();
  return speeds.map((v, i) => {
    const spun = env.spin && v > 3;
    const reach = counts(cm) * (1 + (Math.random() - 0.5) * 0.004) * (spun && v > 3.5 ? 0.97 : 1);
    const secs = (2 * cm) / 100 / v;
    const sign = i % 2 ? -1 : 1;
    const s: Stroke = {
      startUs: t,
      endUs: t + secs * 1e6,
      dx: sign * reach,
      dy: 0,
      reach,
      path: reach * 1.01,
      back: 4,
      peak: countsPerS(v),
      flips: spun ? 4 : 0,
      dropouts: 0,
      longestDropoutUs: 0,
      stalls: 0,
      end: i === speeds.length - 1 ? "pause" : "turn",
    };
    t += secs * 1e6;
    return { strokes: [s], travel: s.path, reports: 400, dx: s.dx };
  });
}

/** How the sensor behaves in the air and on slow slides at each lift-off, on a glass pad. */
const AIR: Record<string, { leakMm: number; stallEvery: number }> = {
  "0.7mm": { leakMm: 0.06, stallEvery: 10 },
  "1mm": { leakMm: 0.3, stallEvery: 0 },
  "2mm": { leakMm: 1.2, stallEvery: 0 },
};

/** Ten slides to the right, each lifted and carried back, at the lift-off the mouse has now. */
export function returns(n = 10): Scripted[] {
  const air = AIR[fake.device.settings?.lod ?? "1mm"];
  let t = us();
  const out: Scripted[] = [];
  for (let i = 0; i < n; i++) {
    const reach = counts(8 * (1 + (Math.random() - 0.5) * 0.1));
    const leak = counts((air.leakMm / 10) * (0.7 + Math.random() * 0.6));
    const slide: Stroke = {
      startUs: t,
      endUs: t + 1_600_000,
      dx: reach - leak / 2,
      dy: 0,
      reach,
      path: reach + leak / 2,
      back: leak / 2,
      peak: countsPerS(0.25),
      flips: 0,
      dropouts: 0,
      longestDropoutUs: 0,
      stalls: air.stallEvery && i % air.stallEvery === 1 ? 1 : 0,
      end: "pause",
    };
    t += 2_100_000;
    const strokes = [slide];
    // At 2 mm the sensor follows the start of the carry back for a moment.
    if (air.leakMm > 1) strokes.push({ ...slide, startUs: t - 400_000, endUs: t - 380_000, dx: -leak / 2, reach: leak / 2, path: leak / 2, back: 0, peak: countsPerS(0.1), stalls: 0 });
    out.push({ strokes, travel: slide.path, reports: 300, dx: slide.dx }, {}, {});
  }
  return out;
}

/** Play along with the lift-off check: ten returns at each lift-off it sets, until it puts the
 *  mouse's own back. */
export function liftOffAlong() {
  const seen = new Set<string>();
  let lod = "";
  let queue: Scripted[] = [];
  let quiet = 0;
  run(() => {
    const now = fake.device.settings?.lod ?? "";
    if (now !== lod) {
      lod = now;
      // The setting it started on comes round again at the end: that's the test finishing.
      if (seen.has(lod) && seen.size >= 2) return null;
      seen.add(lod);
      // A moment for the setting to land before moving.
      queue = [{}, {}, {}, ...returns()];
    }
    if (!queue.length) return ++quiet > 600 ? null : {};
    return queue.shift() ?? {};
  });
}

// ---- Commands -------------------------------------------------------------------------------------

function padView(app: string | null): PadView | null {
  if (fake.pad === "yours") return { ...clone(real.padYours), tracking: fake.prefs.padTracking };
  const all = real.padSim;
  const tracking = fake.prefs.padTracking;
  const v = clone((app && all[app]) || (tracking ? all.tracking : all.all));
  // The simulator's games are listed for recording only once the lab's prefs list them.
  return { ...v, tracking };
}

const running = (): RunningApp[] =>
  real.apps
    .filter((a) => /valorant|r5apex|marvel/i.test(a.exe))
    .map((a) => ({ exe: a.exe, name: a.name, title: a.name, path: a.path, icon: a.icon }));

async function handle(cmd: string, args: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "get_state":
      return fake.device;
    case "get_states":
      return [fake.device];
    case "supported_mice":
      return [4, 1, 2, 5, 3, 6].map((mid) => ({
        family: "compx",
        brand: "Zaopin",
        model: ["", "ZI PRO", "Z2", "Z2 mini", "Z1 PRO MAX", "Z2 MAX", "Z2 mini MAX"][mid],
        key: `compx/7-${mid}`,
        tested: mid === 4,
      })).concat(
        ["PRO X SUPERLIGHT", "PRO X SUPERLIGHT 2", "PRO X SUPERLIGHT 2 DEX"].map((model, i) => ({
          family: "hidpp",
          brand: "Logitech",
          model,
          key: `hidpp/${["c094", "c09b", "c0a0"][i]}`,
          tested: i === 0,
        })),
      );
    case "get_mice":
      return clone(fake.mice);
    case "set_mouse_prefs": {
      const { key, prefs } = args as { key: string; prefs: KnownMouse["prefs"] };
      fake.mice = fake.mice.map((m) => (m.key === key ? { ...m, prefs } : m));
      void emit("mice", fake.mice);
      changed();
      return null;
    }
    case "forget_mouse": {
      const key = (args as { key: string }).key;
      const gone = fake.mice.find((m) => m.key === key);
      if (!gone) throw "hrtz doesn't know this mouse.";
      if (fake.device.connected && fake.device.key === key) throw "Unplug it first: a connected mouse comes straight back.";
      fake.mice = fake.mice.filter((m) => m.key !== key);
      void emit("mice", fake.mice);
      return clone(gone);
    }
    case "restore_mouse": {
      const mouse = (args as { mouse: KnownMouse }).mouse;
      if (!fake.mice.some((m) => m.key === mouse.key)) fake.mice = [...fake.mice, { ...mouse, last: mouse.last ?? null }];
      void emit("mice", fake.mice);
      return null;
    }
    case "get_prefs":
      return fake.prefs;
    case "set_prefs":
      pushPrefs(args.prefs as Prefs);
      return null;
    case "app_status":
      return { foregroundExe: null, inUse: fake.device.key ?? null, vendorApps: LAB_VENDOR_APPS, hudShowing: false };
    case "apply_settings":
      await wait(250);
      pushDevice({ settings: args.settings as Settings });
      return args.settings;
    case "set_profile":
      pushDevice({ profile: args.index as number });
      return null;
    case "list_backups":
      return fake.backups.map(({ settings: _, ...meta }) => meta);
    case "backup_settings": {
      const b = fake.backups.find((x) => x.id === args.id);
      if (!b) throw "That backup is gone.";
      return b.settings;
    }
    case "create_backup":
      return saveBackup(String(args.note ?? ""));
    case "restore_backup":
      return restore(String(args.id));
    case "delete_backup":
      fake.backups = fake.backups.filter((x) => x.id !== args.id);
      changed();
      return null;
    case "rename_backup": {
      // A name makes an automatic backup yours: it leaves the ten hrtz keeps.
      const note = String(args.note).trim();
      fake.backups = fake.backups.map((x) => (x.id === args.id ? { ...x, note, auto: note ? null : x.auto } : x));
      changed();
      const { settings: _, ...meta } = fake.backups.find((x) => x.id === args.id)!;
      return meta;
    }
    case "undo_restore": {
      // Puts back the setup saved just before a restore, and drops that backup: it's done its job.
      const { written } = await restore(String(args.id), false);
      // Only while it's still hrtz's own: one given a name since is yours, and stays.
      fake.backups = fake.backups.filter((x) => x.id !== args.id || !x.auto);
      changed();
      return written;
    }
    case "export_backup":
    case "open_backups_folder":
    case "hud_test":
      return null;
    case "import_backup": {
      const src = fake.backups[fake.backups.length - 1];
      const created = stamp(new Date());
      const b: Backup = { ...clone(src), id: idOf(created), created, note: "Imported", auto: null };
      fake.backups = [b, ...fake.backups];
      changed();
      return b;
    }
    case "factory_reset":
      await saveBackup("", { kind: "factory" });
      void emit("backups", null);
      await progress(1);
      return null;
    case "meter":
      meter(!!args.on);
      return null;
    case "meter_reset":
      resetConn();
      reports = [];
      return null;
    case "meter_hold":
      holdWanted = !!args.on;
      return null;
    case "capture_open":
      // As in Rust: each visit to the screen is a new run (opening it again only changes the text).
      if (!captured) {
        resetConn();
        reports = [];
      }
      captured = true;
      return null;
    case "capture_update":
      return null;
    case "capture_close":
      if (captured) {
        captured = false;
        void emit("capture-closed", "done");
      }
      return null;
    case "pair_receiver": {
      // The receiver looks for a mouse; this one is found after six seconds of pressing the buttons.
      for (let left = 30; left > 24; left--) {
        void emit("pairing", left);
        await wait(1000);
      }
      return true;
    }
    case "click_history": {
      // A week of use: a few thousand presses a day, and Left bouncing twice, 9 and 14 ms apart.
      const days = Array.from({ length: 7 }, (_, i) => {
        const d = new Date(Date.now() - (6 - i) * 86_400_000);
        const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
        const left = i === 3 ? { presses: 2210, doubles: 1, worstMs: 9 } : i === 5 ? { presses: 1874, doubles: 1, worstMs: 14 } : { presses: 1500 + i * 97, doubles: 0, worstMs: 0 };
        return { date, buttons: [left, { presses: 310 + i * 11, doubles: 0, worstMs: 0 }, { presses: 12, doubles: 0, worstMs: 0 }, { presses: 40, doubles: 0, worstMs: 0 }, { presses: 9, doubles: 0, worstMs: 0 }] };
      });
      return days;
    }
    case "pad_state":
      return padView((args.app as string | null) ?? null);
    case "pad_calibrate":
    case "pad_clear":
      return null;
    case "game_sensitivity":
      return real.games[String(args.exe).toLowerCase()] ?? null;
    case "running_apps":
      return running();
    case "installed_games":
      return real.apps.map((a) => ({ name: a.name, exe: a.exe, path: a.path, source: /valorant/i.test(a.exe) ? "Riot" : /marvel/i.test(a.exe) ? "Epic" : "Steam" }));
    case "app_icon":
      return real.apps.find((a) => a.path === args.path)?.icon ?? null;
    case "pointer_settings":
      return fake.pointer;
    case "fix_pointer":
      fake.pointer = { ...fake.pointer, acceleration: args.acceleration ? false : fake.pointer.acceleration, speed: args.speed ? 10 : fake.pointer.speed };
      changed();
      return fake.pointer;
    case "battery_summary":
      return { since: null, life: [], standbyPerDay: null, awakePerDay: null, usualHz: null, daysPerCharge: null, cycles: 0, charges: 0, capacity: null };
    case "battery_history":
      return [];
    case "plugin:dialog|save":
      return "C:\\Users\\you\\Documents\\mouse-backup.json";
    case "plugin:dialog|open":
      return "C:\\Users\\you\\Downloads\\mouse-backup.json";
    // A newer version, for the sidebar's update states: ?update=downloading (slow) or ?update=ready
    // (at once). Without the parameter there's none.
    case "plugin:updater|check":
      return UPDATE ? { rid: 1, currentVersion: "0.1.0", version: "0.2.0", body: "", rawJson: {} } : null;
    case "plugin:updater|download": {
      const send = (args.onEvent as { onmessage?: (e: unknown) => void } | undefined)?.onmessage;
      const total = 9_800_000;
      const steps = UPDATE === "ready" ? 1 : 40;
      send?.({ event: "Started", data: { contentLength: total } });
      for (let i = 0; i < steps; i++) {
        await wait(UPDATE === "ready" ? 0 : 500);
        send?.({ event: "Progress", data: { chunkLength: total / steps } });
      }
      send?.({ event: "Finished" });
      return 2;
    }
    case "plugin:updater|install":
    case "plugin:resources|close":
    case "update_ready":
    case "prepare_update":
      return null;
  }
  if (cmd.startsWith("plugin:window|") || cmd.startsWith("plugin:webview|") || cmd.startsWith("plugin:autostart|")) return false;
  console.warn("lab backend: no answer for", cmd, args);
  return null;
}

let started = false;
/** Install the stand-in. Call before the app mounts. */
export function startFakeBackend() {
  if (started) return;
  started = true;
  mockWindows("main");
  mockIPC((cmd, args) => handle(cmd, (args ?? {}) as Record<string, unknown>), { shouldMockEvents: true });
  setupInput();
  // For scripted screenshots: the panel's "make something happen", from outside the page.
  Object.assign(window, { lab: { play, movement, circlesAlong, dpiRuns, doubleClick, scrollNotches, stillFor, swipes, returns, liftOffAlong, plugInto, env, fake } });
}
