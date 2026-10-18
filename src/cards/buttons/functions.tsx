import type { ReactElement } from "react";
import {
  ArrowBidirectionalUpDown20Regular,
  ArrowRepeatAll20Regular,
  CursorClick20Regular,
  Flash20Regular,
  Gauge20Regular,
  Keyboard20Regular,
  Play20Regular,
  Prohibited20Regular,
  Stack20Regular,
} from "@fluentui/react-icons";
import type { ButtonAction } from "@/api";
import { actionLabel } from "@/hid";

/** Flash index of the DPI button, on the underside of every Zaopin model. */
export const DPI_BUTTON = 5;

/** What the six buttons do out of the box on a Zaopin mouse, by flash index. */
export const DEFAULT_ACTIONS: Record<number, ButtonAction> = {
  0: { kind: "mouse", button: "left" },
  1: { kind: "mouse", button: "right" },
  2: { kind: "mouse", button: "middle" },
  3: { kind: "mouse", button: "back" },
  4: { kind: "mouse", button: "forward" },
  5: { kind: "dpi", action: "cycle" },
};

/** The kinds of function a button can have: one tile each in the editor. */
export type Fn = "mouse" | "dpi" | "scroll" | "keys" | "media" | "fire" | "pollingSwitch" | "profile" | "disabled";

type DpiChoice = "cycle" | "up" | "down" | "default" | "hold";
type Direction = "up" | "down" | "left" | "right";

/** What a family's buttons can do: the tiles, the options within them, and whose app set what
 *  this one can't edit. */
export interface Catalog {
  fns: Fn[];
  dpi: DpiChoice[];
  scroll: Direction[];
  /** Keys in a combination: up to three counting modifiers (a Zaopin), or one besides them. */
  oneKey: boolean;
  maker: string;
}

export const CATALOGS: Record<"compx" | "hidpp", Catalog> = {
  compx: {
    fns: ["mouse", "dpi", "scroll", "keys", "media", "fire", "pollingSwitch", "disabled"],
    dpi: ["cycle", "up", "down", "hold"],
    scroll: ["up", "down", "left", "right"],
    oneKey: false,
    maker: "Zaopin's app",
  },
  // OpenMouse's records (`hidpp/buttons.rs`): no rapid fire, polling switch or hold-for-DPI, and the
  // wheel tilts but doesn't scroll from a button.
  hidpp: {
    fns: ["mouse", "dpi", "scroll", "keys", "media", "profile", "disabled"],
    dpi: ["cycle", "up", "down", "default"],
    scroll: ["left", "right"],
    oneKey: true,
    maker: "G HUB",
  },
};

export const FUNCTIONS: { value: Fn; label: string; icon: ReactElement }[] = [
  { value: "mouse", label: "Mouse button", icon: <CursorClick20Regular /> },
  { value: "dpi", label: "DPI", icon: <Gauge20Regular /> },
  { value: "scroll", label: "Scroll", icon: <ArrowBidirectionalUpDown20Regular /> },
  { value: "keys", label: "Keys", icon: <Keyboard20Regular /> },
  { value: "media", label: "Media", icon: <Play20Regular /> },
  { value: "fire", label: "Rapid fire", icon: <Flash20Regular /> },
  { value: "pollingSwitch", label: "Polling rate", icon: <ArrowRepeatAll20Regular /> },
  { value: "profile", label: "Profile", icon: <Stack20Regular /> },
  { value: "disabled", label: "Disabled", icon: <Prohibited20Regular /> },
];

/** The tile an action belongs to, or null for a macro or vendor function this app can't edit. */
export function functionOf(a: ButtonAction): Fn | null {
  if (a.kind === "dpiLock") return "dpi";
  if (a.kind === "macro" || a.kind === "raw" || a.kind === "vendor") return null;
  return a.kind;
}

/** What picking a tile writes: the function's first option this mouse has. */
export function firstOf(fn: Fn, catalog: Catalog): ButtonAction {
  switch (fn) {
    case "mouse":
      return { kind: "mouse", button: "left" };
    case "dpi":
      return { kind: "dpi", action: "cycle" };
    case "scroll":
      return { kind: "scroll", direction: catalog.scroll[0] };
    case "profile":
      return { kind: "profile", action: "next" };
    case "keys":
      return { kind: "keys", modifiers: 0, keys: [] };
    case "media":
      return { kind: "media", usage: 0x00cd };
    case "fire":
      return { kind: "fire", intervalMs: 30, times: 0 };
    default:
      return { kind: fn };
  }
}

export const sameAction = (a: ButtonAction | undefined, b: ButtonAction | undefined) => JSON.stringify(a) === JSON.stringify(b);
export const isLeftClick = (a: ButtonAction | undefined) => a?.kind === "mouse" && a.button === "left";

/** A short line for what a button does: "Rapid fire, 30 ms", "Hold for 400 DPI", "Ctrl + C". */
export function shortLabel(a: ButtonAction): string {
  if (a.kind === "fire") return a.times === 0 ? `Rapid fire, ${a.intervalMs} ms` : `Rapid fire ×${a.times}, ${a.intervalMs} ms`;
  if (a.kind === "dpiLock") return `Hold for ${a.dpi.toLocaleString()} DPI`;
  return actionLabel(a);
}
