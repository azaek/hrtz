import { useSyncExternalStore } from "react";
import { api, message, on, showMouse, type AppStatus, type DeviceState, type KnownMouse, type MousePrefs, type Prefs, type Settings, type Supported } from "./api";

/** The onboarding's steps: first run goes Welcome, Connect, Your mouse, Preferences; "Connect a new
 *  mouse" goes Connect, Your mouse. */
export type Step = "welcome" | "connect" | "found" | "prefs";
/** The onboarding while it's up: the mice known when it opened, so a new one can be told apart. */
export type Flow = { mode: "first" | "add"; step: Step; known: string[] };

interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

interface Store {
  /** The mouse the window shows (see `shown`). */
  device: DeviceState | null;
  /** Every mouse's state, by key: connected, or until its connection went. */
  devices: Record<string, DeviceState>;
  /** The mouse the window shows, by key: the first one shown, then wherever the switcher puts it. */
  selected: string | null;
  /** Every mouse hrtz can run. */
  supported: Supported[];
  /** The onboarding, while it's up. */
  flow: Flow | null;
  /** A page asked for from elsewhere than the sidebar, and the card on it to bring into view (the
   *  "Not tested" badge opens Settings at Help test). `n` tells two asks apart. */
  nav: { page: string; focus: string | null; n: number } | null;
  prefs: Prefs | null;
  /** The mice hrtz has met, each with what it keeps for it. */
  mice: KnownMouse[];
  status: AppStatus;
  progress: [number, number] | null;
  toasts: Toast[];
  busy: boolean;
}

let state: Store = {
  device: null,
  devices: {},
  selected: null,
  supported: [],
  flow: null,
  nav: null,
  prefs: null,
  mice: [],
  status: { foregroundExe: null, vendorApps: [], hudShowing: false },
  progress: null,
  toasts: [],
  busy: false,
};
const listeners = new Set<() => void>();

function set(patch: Partial<Store>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export function useStore<T>(pick: (s: Store) => T): T {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => pick(state),
  );
}

let toastId = 0;
export function toast(text: string, tone: Toast["tone"] = "info") {
  const id = ++toastId;
  set({ toasts: [...state.toasts, { id, text, tone }] });
  setTimeout(() => set({ toasts: state.toasts.filter((t) => t.id !== id) }), tone === "error" ? 6000 : 3000);
}

const NOTHING: DeviceState = {
  rev: 0,
  connected: false,
  online: false,
  model: null,
  cid: 0,
  mid: 0,
  deviceType: 0,
  firmware: null,
  battery: null,
  profile: null,
  longRange: null,
  settings: null,
  capabilities: null,
  port: null,
};

/** A known mouse that isn't connected, as its pages show it: what it held last time. */
function away(m: KnownMouse): DeviceState {
  return { ...NOTHING, ...(m.last ?? {}), key: m.key, model: m.last?.model ?? m.model, connected: false, online: false, battery: null };
}

/** The mouse the window shows: the one picked, connected or not; before any, the one in use, else
 *  one awake, else one connected, else the one seen last. */
function shown(): DeviceState | null {
  const { devices, mice, selected } = state;
  const live = (k: string | null | undefined) => (k && devices[k]?.connected ? devices[k] : undefined);
  const known = (k: string | null | undefined) => {
    const m = k ? mice.find((x) => x.key === k) : undefined;
    return m && away(m);
  };
  if (selected && (live(selected) || known(selected))) return live(selected) ?? known(selected)!;
  const all = Object.values(devices);
  const last = [...mice].sort((a, b) => b.lastSeen - a.lastSeen)[0];
  return live(state.status.inUse) ?? all.find((d) => d.connected && d.online) ?? all.find((d) => d.connected) ?? known(last?.key) ?? null;
}

/** Show the mouse to show, and send commands to it. The first connected one shown stays shown,
 *  until the switcher picks another. */
function reshow() {
  const device = shown();
  showMouse(device?.connected ? (device.key ?? null) : null);
  const selected = state.selected ?? (device?.connected ? (device.key ?? null) : null);
  set({ device, selected });
}

/** Put a mouse's state in, and show the one to show. */
function putDevice(d: DeviceState) {
  set({ devices: { ...state.devices, [d.key!]: d } });
  reshow();
}

/** Show mouse `key` (the switcher). */
export function pickMouse(key: string) {
  set({ selected: key });
  reshow();
}

/** Remove mouse `key` (not connected) from hrtz. Returns what was kept for it, for Undo; null if
 *  it couldn't be, as said in a toast. */
export async function forgetMouse(key: string): Promise<KnownMouse | null> {
  try {
    const gone = await api.forgetMouse(key);
    set({ mice: state.mice.filter((m) => m.key !== key), selected: state.selected === key ? null : state.selected });
    reshow();
    return gone;
  } catch (e) {
    toast(message(e), "error");
    return null;
  }
}

/** Put back a mouse just removed, and show it again. */
export async function restoreMouse(mouse: KnownMouse) {
  try {
    await api.restoreMouse(mouse);
    set({ mice: state.mice.some((m) => m.key === mouse.key) ? state.mice : [...state.mice, mouse], selected: mouse.key });
    reshow();
  } catch (e) {
    toast(message(e), "error");
  }
}

let navN = 0;
/** Open `page`, and bring the card `focus` names into view there. */
export function goTo(page: string, focus: string | null = null) {
  set({ nav: { page, focus, n: ++navN } });
}

/** The card asked for has been brought into view: opening the page again doesn't repeat it. */
export function focusDone() {
  if (state.nav?.focus) set({ nav: { ...state.nav, focus: null } });
}

/** Open the onboarding: first run from its welcome, a new mouse from connecting it (or straight to
 *  it, when it's just been plugged in). */
export function startFlow(mode: Flow["mode"], step?: Step, except?: string) {
  const known = state.mice.map((m) => m.key).filter((k) => k !== except);
  set({ flow: { mode, step: step ?? (mode === "first" ? "welcome" : "connect"), known } });
}

export function goStep(step: Step) {
  if (state.flow) set({ flow: { ...state.flow, step } });
}

export function endFlow() {
  set({ flow: null });
}

/** Device state from a reply or an event, whichever is newer: they can arrive in either order. A
 *  connection that hasn't met its mouse yet isn't shown. */
function setDevice(d: DeviceState) {
  if (!d.key) return;
  const prev = state.devices[d.key];
  if (!prev || d.rev >= prev.rev) putDevice(d);
}

export async function init() {
  await Promise.all([
    on.state(setDevice),
    on.prefs((prefs) => set({ prefs })),
    on.mice((mice) => {
      set({ mice });
      reshow();
    }),
    // A mouse never met, plugged in while hrtz runs: the onboarding's second half.
    on.mouseNew((key) => {
      if (!state.flow && state.prefs?.welcomed) startFlow("add", "found", key);
    }),
    on.status((status) => {
      set({ status });
      reshow();
    }),
    on.progress((progress) => set({ progress: progress[0] >= progress[1] ? null : progress })),
  ]);
  const [states, prefs, status, mice, supported] = await Promise.all([api.states(), api.prefs(), api.appStatus(), api.mice(), api.supportedMice()]);
  set({ prefs, status, mice, supported });
  states.forEach(setDevice);
  reshow();
  // First run: the onboarding, as a full page.
  if (!prefs.welcomed) startFlow("first");
}

/** Optimistically apply `next`, roll back and explain on failure. */
export async function commit(next: Settings) {
  const prev = state.device;
  if (!prev?.settings || !prev.key) return;
  putDevice({ ...prev, settings: next });
  set({ busy: true });
  try {
    const applied = await api.applySettings(next);
    const now = state.devices[prev.key];
    if (now) putDevice({ ...now, settings: applied });
  } catch (e) {
    toast(message(e), "error");
    putDevice(prev);
  } finally {
    set({ busy: false });
  }
}

export async function savePrefs(next: Prefs) {
  const prev = state.prefs;
  set({ prefs: next });
  try {
    await api.setPrefs(next);
  } catch (e) {
    set({ prefs: prev });
    toast(message(e), "error");
  }
}

/** The connected mouse as hrtz knows it (its key and prefs); null with none connected. The
 *  stored object itself, so it only changes when the mice do. */
export function useMouse(): KnownMouse | null {
  return useStore((s) => {
    const key = s.device?.connected ? s.device.key : null;
    return (key && s.mice.find((x) => x.key === key)) || null;
  });
}

/** Change what's kept for mouse `key`: shown at once, put back and explained on failure. */
export async function saveMousePrefs(key: string, prefs: MousePrefs) {
  const prev = state.mice;
  set({ mice: prev.map((m) => (m.key === key ? { ...m, prefs } : m)) });
  try {
    await api.setMousePrefs(key, prefs);
  } catch (e) {
    set({ mice: prev });
    toast(message(e), "error");
  }
}

export async function run<T>(work: () => Promise<T>, done?: string): Promise<T | undefined> {
  set({ busy: true });
  try {
    const r = await work();
    if (done) toast(done);
    return r;
  } catch (e) {
    toast(message(e), "error");
    return undefined;
  } finally {
    set({ busy: false, progress: null });
  }
}
