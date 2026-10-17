import shippedJson from "./states.json";

/**
 * How focus and selection look, as one plain config. `states.json` holds the values. Continuous
 * values become CSS variables on :root and modes become `data-*` attributes there, read by
 * states.css.
 */
export type StateConfig = {
  /** Seed the config was first drawn from. Not used for rendering. */
  seed: number;
  /** Accent as a dark-theme colour; light theme uses a deeper mix of it. */
  accent: string;

  focusStyle: "ring" | "halo" | "ringHalo";
  focusColor: "neutral" | "accent";
  focusOpacity: number;
  focusWidth: number;
  focusGap: number;
  haloSize: number;
  haloBlur: number;
  haloOpacity: number;
  /** Overlay on focused controls, Material's state layer. */
  focusLayer: number;

  listStyle: "neutralPill" | "tonalPill" | "tonal";
  choiceStyle: "solid" | "tonal" | "raised";
  tileStyle: "edge" | "tonal" | "tonalEdge" | "glow";
  /** Percent of accent mixed into selected fills. */
  tint: number;
  edgeOpacity: number;
  edgeWidth: number;
  glowSize: number;
  accentText: boolean;
  pillWidth: number;
  pillHeight: number;
  duration: number;
};

export type Rgb = [number, number, number];

export function hexToRgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  const n = parseInt(h.length === 3 ? h.replace(/./g, "$&$&") : h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** `a` over `b` at `t` (0–1 of a), in sRGB like CSS color-mix(in srgb, …). */
export const mixRgb = (a: Rgb, b: Rgb, t: number): Rgb => [0, 1, 2].map((i) => Math.round(a[i] * t + b[i] * (1 - t))) as Rgb;

const rgba = ([r, g, b]: Rgb, a = 1) => `rgb(${r} ${g} ${b} / ${+a.toFixed(3)})`;
const px = (n: number) => `${+n.toFixed(2)}px`;

/** The accent and its derived shades for one theme. */
export function accentFor(c: StateConfig, isDark: boolean) {
  const base = hexToRgb(c.accent);
  const accent = isDark ? base : mixRgb(base, [0, 0, 0], 0.72);
  return {
    accent,
    // Filled controls with white text (the solid choice style).
    strong: mixRgb(base, [0, 0, 0], isDark ? 0.62 : 0.72),
    text: isDark ? base : mixRgb(base, [0, 0, 0], 0.62),
  };
}

/** Base colour the focus ring and halo are drawn in. */
export function focusBase(c: StateConfig, isDark: boolean): Rgb {
  if (c.focusColor === "accent") return accentFor(c, isDark).accent;
  return isDark ? [255, 255, 255] : [0, 0, 0];
}

/** Fluent's colorNeutralBackground1, the fill of a resting button. */
export const buttonFill = (isDark: boolean): Rgb => (isDark ? [41, 41, 41] : [255, 255, 255]);

export function stateVars(c: StateConfig, isDark: boolean): Record<string, string> {
  const { accent, strong, text } = accentFor(c, isDark);
  const ring = focusBase(c, isDark);
  const neutral = (a: number) => rgba(isDark ? [255, 255, 255] : [0, 0, 0], a);
  const hasRing = c.focusStyle !== "halo";
  const hasHalo = c.focusStyle !== "ring";
  const tinted = (t: number) => rgba(accent, t / 100);

  return {
    "--accent": rgba(accent),
    "--accent-strong": rgba(strong),
    "--accent-fg": rgba(text),

    "--focus-ring": rgba(ring, c.focusOpacity),
    "--focus-width": px(hasRing ? c.focusWidth : 0),
    "--focus-offset": px(c.focusGap),
    "--focus-glow": hasHalo ? `0 0 ${px(c.haloBlur)} ${px(c.haloSize)} ${rgba(ring, c.haloOpacity)}` : "0 0 #0000",
    "--focus-layer": neutral(c.focusLayer),

    // Rows: navigation and dropdown options.
    "--row-sel-bg": c.listStyle === "neutralPill" ? neutral(isDark ? 0.0605 : 0.0373) : tinted(c.tint),
    "--pill-w": px(c.pillWidth),
    "--pill-h": px(c.listStyle === "tonal" ? 0 : c.pillHeight),

    // Choice buttons.
    "--seg-tonal-bg": rgba(mixRgb(accent, buttonFill(isDark), c.tint / 100)),
    "--seg-edge": tinted(c.edgeOpacity * 100),
    "--seg-track": isDark ? "rgb(0 0 0 / 0.2)" : "rgb(0 0 0 / 0.045)",
    "--seg-puck": isDark ? "rgb(255 255 255 / 0.13)" : "rgb(255 255 255)",
    "--seg-puck-shadow": isDark
      ? "0 1px 2px rgb(0 0 0 / 0.35), inset 0 0 0 1px rgb(255 255 255 / 0.05)"
      : "0 1px 2px rgb(0 0 0 / 0.12), 0 1px 4px rgb(0 0 0 / 0.06)",

    // Tiles and cards. Percentages feed color-mix() against each tile's own resting colours.
    "--tile-tint": `${c.tileStyle === "tonal" || c.tileStyle === "tonalEdge" ? c.tint : 0}%`,
    "--tile-edge-a": `${c.tileStyle === "tonal" ? 0 : Math.round(c.edgeOpacity * 100)}%`,
    "--tile-edge-extra": px(c.tileStyle === "tonal" ? 0 : c.edgeWidth - 1),
    "--tile-glow": c.tileStyle === "glow" ? `0 0 ${px(c.glowSize)} ${tinted(isDark ? 35 : 28)}` : "0 0 #0000",

    "--state-dur": `${Math.round(c.duration)}ms`,
  };
}

let shipped = shippedJson as StateConfig;
let config = shipped;
let dark = true;
const listeners = new Set<() => void>();

function write() {
  const root = document.documentElement;
  for (const [k, v] of Object.entries(stateVars(config, dark))) root.style.setProperty(k, v);
  root.dataset.choiceStyle = config.choiceStyle;
  root.dataset.accentText = config.accentText ? "on" : "off";
}

export const getStates = () => config;
export const getShippedStates = () => shipped;

export function setStates(next: StateConfig) {
  config = next;
  write();
  listeners.forEach((f) => f());
}

export function setStatesTheme(isDark: boolean) {
  if (isDark === dark && document.documentElement.style.getPropertyValue("--accent")) return;
  dark = isDark;
  write();
}

export function subscribeStates(f: () => void) {
  listeners.add(f);
  return () => {
    listeners.delete(f);
  };
}

write();

// An edit to states.json during development applies without reloading the page.
if (import.meta.hot) {
  import.meta.hot.accept("./states.json", (mod) => {
    if (mod) shipped = mod.default as StateConfig;
  });
}
