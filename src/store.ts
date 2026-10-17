import { useSyncExternalStore } from "react";
import { api, message, on, type AppStatus, type DeviceState, type KnownMouse, type MousePrefs, type Prefs, type Settings } from "./api";

interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

interface Store {
  device: DeviceState | null;
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
  prefs: null,
  mice: [],
  status: { foregroundExe: null, vendorAppRunning: false, hudShowing: false },
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

/** Device state from a reply or an event, whichever is newer: they can arrive in either order. */
function setDevice(device: DeviceState) {
  if (!state.device || device.rev >= state.device.rev) set({ device });
}

export async function init() {
  await Promise.all([
    on.state(setDevice),
    on.prefs((prefs) => set({ prefs })),
    on.mice((mice) => set({ mice })),
    on.status((status) => set({ status })),
    on.progress((progress) => set({ progress: progress[0] >= progress[1] ? null : progress })),
  ]);
  const [device, prefs, status, mice] = await Promise.all([api.state(), api.prefs(), api.appStatus(), api.mice()]);
  setDevice(device);
  set({ prefs, status, mice });
}

/** Optimistically apply `next`, roll back and explain on failure. */
export async function commit(next: Settings) {
  const prev = state.device;
  if (!prev?.settings) return;
  set({ device: { ...prev, settings: next }, busy: true });
  try {
    const applied = await api.applySettings(next);
    if (state.device) set({ device: { ...state.device, settings: applied } });
  } catch (e) {
    toast(message(e), "error");
    set({ device: prev });
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
