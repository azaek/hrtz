import { useSyncExternalStore } from "react";
import { LINK_NAME } from "@/links";
import { MODEL_ART } from "@/models";
import { real } from "../fake/backend";

// Throwaway, like the labs: the mice the shell lab plays with, and a small world around them
// (which are plugged in, which hrtz has met, whether Zaopin's app is running). The Zaopin is this
// PC's own (its pages run on the stand-in's data); the others are made up, drawn instead of a
// render.

export type Mouse = {
  id: string;
  brand: string;
  model: string;
  /** A render, or null for the drawn stand-in. */
  image: string | null;
  link: string;
  /** Plugged in now. */
  connected: boolean;
  online: boolean;
  battery: { level: number; charging: boolean } | null;
  rateHz: number | null;
  firmware: string | null;
  /** When it was last connected, for one that isn't. */
  lastSeen: string | null;
  /** The one being moved now. */
  inUse: boolean;
  /** hrtz can run it. */
  supported: boolean;
  /** Tried on the real thing. */
  tested: boolean;
  /** Its maker's app, which fights hrtz for the mouse while it runs. */
  vendorApp: string | null;
  /** Its pages run on the stand-in's data (the Zaopin); the others show a placeholder. */
  real: boolean;
  /** Pages it doesn't have, hidden from the navigation. */
  hides: string[];
};

const d = real.device;
const MICE: Record<string, Mouse> = {
  zaopin: {
    id: "zaopin",
    brand: "Zaopin",
    model: d.model ?? "Z1 PRO MAX",
    image: MODEL_ART[d.mid]?.image ?? null,
    link: LINK_NAME[d.deviceType] ?? "receiver",
    connected: true,
    online: true,
    battery: d.battery ? { level: d.battery.level, charging: d.battery.charging } : { level: 72, charging: false },
    rateHz: d.settings?.pollingHz ?? 8000,
    firmware: d.firmware,
    lastSeen: null,
    inUse: false,
    supported: true,
    tested: true,
    vendorApp: "Zaopin's app",
    real: true,
    hides: [],
  },
  z2: {
    id: "z2",
    brand: "Zaopin",
    model: "Z2 mini",
    image: MODEL_ART[3].image,
    link: "4K receiver",
    connected: true,
    online: true,
    battery: { level: 91, charging: false },
    rateHz: 4000,
    firmware: "1.06",
    lastSeen: null,
    inUse: false,
    supported: true,
    tested: false,
    vendorApp: "Zaopin's app",
    real: false,
    hides: [],
  },
  superlight: {
    id: "superlight",
    brand: "Logitech",
    model: "PRO X SUPERLIGHT",
    image: null,
    link: "LIGHTSPEED receiver",
    connected: true,
    online: true,
    battery: { level: 48, charging: false },
    rateHz: 1000,
    firmware: null,
    lastSeen: null,
    inUse: false,
    supported: true,
    tested: true,
    vendorApp: "G HUB",
    real: false,
    // An example of a page a mouse doesn't have.
    hides: ["profiles"],
  },
  atk: {
    id: "atk",
    brand: "ATK",
    model: "ATK X1 Ultimate",
    image: null,
    link: "receiver",
    connected: true,
    online: true,
    battery: null,
    rateHz: null,
    firmware: null,
    lastSeen: null,
    inUse: false,
    supported: false,
    tested: false,
    vendorApp: null,
    real: false,
    hides: [],
  },
};
export const MOUSE_IDS = Object.keys(MICE);
export const mouseName = (id: string) => MICE[id].model;

// ---- The world --------------------------------------------------------------------------------

export type Step = "welcome" | "connect" | "found" | "prefs";
export type Flow = { mode: "first" | "add"; step: Step };

type World = {
  preset: PresetId;
  /** Mice hrtz has met: they stay in the switcher when unplugged. */
  known: string[];
  plugged: string[];
  vendorApp: boolean;
  selected: string | null;
  /** The onboarding, while it's up. */
  flow: Flow | null;
};

export const PRESETS = {
  first: { label: "First run", known: [], plugged: [], flow: { mode: "first", step: "welcome" } },
  one: { label: "One mouse (as today)", known: ["zaopin"], plugged: ["zaopin"], flow: null },
  two: { label: "Two mice", known: ["zaopin", "superlight"], plugged: ["zaopin", "superlight"], flow: null },
  mixed: { label: "One connected, one not", known: ["zaopin", "superlight"], plugged: ["superlight"], flow: null },
  away: { label: "Your mouse isn't connected", known: ["zaopin"], plugged: [], flow: null },
  none: { label: "Set up, but no mouse yet (skipped)", known: [], plugged: [], flow: null },
  add: { label: "Connect a new mouse", known: ["zaopin"], plugged: ["zaopin"], flow: { mode: "add", step: "connect" } },
} satisfies Record<string, { label: string; known: string[]; plugged: string[]; flow: Flow | null }>;
export type PresetId = keyof typeof PRESETS;

const params = new URLSearchParams(location.search);

function fromPreset(preset: PresetId): World {
  const p = PRESETS[preset];
  const w: World = { preset, known: [...p.known], plugged: [...p.plugged], vendorApp: false, selected: null, flow: p.flow && { ...p.flow } };
  w.selected = firstPick(w);
  return w;
}

let world: World = (() => {
  const preset = (params.get("s") as PresetId) in PRESETS ? (params.get("s") as PresetId) : "first";
  const w = fromPreset(preset);
  // For screenshots: ?plug=zaopin,atk (what's plugged in), ?step=found, ?vendor=1.
  if (params.get("plug") !== null) w.plugged = params.get("plug")!.split(",").filter((x) => x in MICE);
  if (w.flow && params.get("step")) w.flow.step = params.get("step") as Step;
  if (params.get("vendor")) w.vendorApp = true;
  return w;
})();
const listeners = new Set<() => void>();

/** The mouse the app opens on: the one in use, else one connected, else the last one seen. */
function firstPick(w: Pick<World, "known" | "plugged">): string | null {
  return w.known.find((id) => w.plugged.includes(id)) ?? w.known[0] ?? null;
}

function set(next: World) {
  world = next;
  const url = new URL(location.href);
  url.searchParams.set("s", world.preset);
  for (const k of ["plug", "step", "vendor"]) url.searchParams.delete(k);
  history.replaceState(null, "", url);
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

/** A known or plugged-in mouse as the app sees it now. */
function seen(w: World, id: string): Mouse {
  const on = w.plugged.includes(id);
  const inUse = w.plugged.find((x) => w.known.includes(x)) === id;
  return { ...MICE[id], connected: on, online: on, lastSeen: on ? null : "today at 9:41", inUse };
}

export function useWorld() {
  const w = useSyncExternalStore(subscribe, () => world);
  const known = w.known.map((id) => seen(w, id));
  // A plugged-in mouse hrtz can't run shows in the switcher too, greyed out.
  const strangers = w.plugged.filter((id) => !w.known.includes(id) && !MICE[id].supported).map((id) => seen(w, id));
  return {
    ...w,
    /** The switcher's list. */
    mice: [...known, ...strangers],
    selectedMouse: known.find((m) => m.id === w.selected) ?? null,
    /** Plugged in, runnable, and not met yet: what the onboarding is waiting for. */
    found: w.plugged.filter((id) => !w.known.includes(id) && MICE[id].supported).map((id) => seen(w, id)),
    /** Plugged in, but hrtz can't run it. */
    unsupported: w.plugged.filter((id) => !MICE[id].supported).map((id) => seen(w, id)),
  };
}

export const pickPreset = (preset: PresetId) => set(fromPreset(preset));
export const pickMouse = (id: string) => set({ ...world, selected: id });
export const setPlugged = (id: string, on: boolean) => {
  const plugged = on ? [...world.plugged.filter((x) => x !== id), id] : world.plugged.filter((x) => x !== id);
  set({ ...world, plugged });
};
export const setVendorApp = (on: boolean) => set({ ...world, vendorApp: on });
export const startFlow = (mode: Flow["mode"], step?: Step) => set({ ...world, flow: { mode, step: step ?? (mode === "first" ? "welcome" : "connect") } });
export const goStep = (step: Step) => world.flow && set({ ...world, flow: { ...world.flow, step } });
export const cancelFlow = () => set({ ...world, flow: null });
/** The onboarding's end: the mice it found are met, and the first of them is shown. */
export function finishFlow() {
  const found = world.plugged.filter((id) => !world.known.includes(id) && MICE[id].supported);
  const known = [...world.known, ...found];
  set({ ...world, known, flow: null, selected: found[0] ?? world.selected ?? firstPick({ known, plugged: world.plugged }) });
}

// ---- Art -----------------------------------------------------------------------------------

/** A mouse drawn from above at an angle, in the renders' frame (298 × 263): a stand-in until a mouse
 *  has its own render. `outline` for one hrtz can't run, or no mouse at all. */
export function DrawnMouse({ width, outline }: { width: number; outline?: boolean }) {
  const body =
    "M 0 -100 C 34 -100 46 -80 48 -50 C 50 -20 52 20 50 50 C 47 85 28 100 0 100 C -28 100 -47 85 -50 50 C -52 20 -50 -20 -48 -50 C -46 -80 -34 -100 0 -100 Z";
  const id = outline ? "lab-mouse-o" : "lab-mouse";
  return (
    <svg width={width} height={(width * 263) / 298} viewBox="0 0 298 263" aria-hidden style={{ display: "block" }}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fbfbfc" />
          <stop offset="0.55" stopColor="#e4e5e9" />
          <stop offset="1" stopColor="#a9abb3" />
        </linearGradient>
        <radialGradient id={`${id}-shadow`}>
          <stop offset="0" stopColor="#000" stopOpacity="0.35" />
          <stop offset="1" stopColor="#000" stopOpacity="0" />
        </radialGradient>
      </defs>
      {!outline && <ellipse cx="156" cy="150" rx="128" ry="92" fill={`url(#${id}-shadow)`} transform="rotate(-40 156 150)" />}
      <g transform="translate(149 128) rotate(-40)">
        {outline ? (
          <g fill="none" stroke="currentColor" strokeOpacity="0.45" strokeWidth="3" strokeLinecap="round">
            <path d={body} strokeDasharray="10 9" />
            <path d="M 0 -100 L 0 -40 M -48 -38 Q 0 -28 48 -38" />
            <rect x="-6" y="-88" width="12" height="28" rx="6" />
          </g>
        ) : (
          <>
            <path d={body} fill={`url(#${id})`} stroke="rgb(0 0 0 / 0.18)" strokeWidth="1.5" />
            <path d="M 0 -100 L 0 -40 M -48 -38 Q 0 -28 48 -38" fill="none" stroke="rgb(0 0 0 / 0.28)" strokeWidth="2" />
            <path d="M -40 -70 C -36 -88 -20 -97 -6 -98" fill="none" stroke="rgb(255 255 255 / 0.9)" strokeWidth="3" strokeLinecap="round" />
            <rect x="-6" y="-88" width="12" height="28" rx="6" fill="#2a2b30" stroke="rgb(0 0 0 / 0.3)" />
          </>
        )}
      </g>
    </svg>
  );
}

/** The mouse's picture at `width`: its render, else the drawn stand-in. */
export function MouseArt({ m, width }: { m: Mouse | null; width: number }) {
  if (m?.image) return <img src={m.image} alt="" width={width} style={{ display: "block", height: "auto", pointerEvents: "none", userSelect: "none" }} />;
  return <DrawnMouse width={width} outline={!m?.supported} />;
}
