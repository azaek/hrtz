// Generated from the vendor apps' model lists and renders (credited in assets/models/NOTICE.md).
// Hotspot centres are in frame pixels; keys are the family's button indices (a Zaopin's flash
// index, a Logitech's place in its profile).

import mid1 from "./assets/models/mid1.png";
import mid2 from "./assets/models/mid2.png";
import mid3 from "./assets/models/mid3.png";
import mid4 from "./assets/models/mid4.png";
import mid5 from "./assets/models/mid5.png";
import mid6 from "./assets/models/mid6.png";
import superlightBlack from "./assets/models/superlight-black.webp";
import superlightMagenta from "./assets/models/superlight-magenta.webp";
import superlightWhite from "./assets/models/superlight-white.webp";
import superlight2Black from "./assets/models/superlight2-black.webp";
import superlight2Pink from "./assets/models/superlight2-pink.webp";
import superlight2White from "./assets/models/superlight2-white.webp";

export interface ModelArt {
  name: string;
  image: string;
  size: [number, number];
  hotspots: Record<number, [number, number]>;
  /** How the picture shows it, for its description: "viewed from the left" unless said. */
  view?: string;
}

/** The model (MID) in a Zaopin key: `compx/7-4` (or a simulated `sim/7-4`) is 4. */
export const midOf = (key: string | null | undefined): number | undefined => {
  const m = key && /^(?:compx|sim)\/\d+-(\d+)$/.exec(key);
  return m ? Number(m[1]) : undefined;
};

/** A key's family: `compx/7-4` is a Zaopin, `hidpp/c094/1a2b3c4d` a Logitech (`sim/` for either,
 *  simulated, keeps the rest of its shape). */
export const familyOf = (key: string | null | undefined): "compx" | "hidpp" | undefined => {
  if (midOf(key) !== undefined) return "compx";
  return key && /^(?:hidpp|sim)\/[0-9a-f]{4}\//.test(key) ? "hidpp" : undefined;
};

/** Whether a connected mouse's key is of the model `model` names: the same key, or (Logitech's,
 *  which add the mouse's own unit ID) that key and more. */
export const ofModel = (key: string | null | undefined, model: string): boolean => !!key && (key === model || key.startsWith(`${model}/`));

/** A mouse's picture, from its key and, for a model that comes in several colours, which it is
 *  (`DeviceState.variant`). */
export const artOf = (key: string | null | undefined, variant?: number | null): ModelArt | undefined => {
  const mid = midOf(key);
  if (mid !== undefined) return MODEL_ART[mid];
  const usb = key && /^(?:hidpp|sim)\/([0-9a-f]{4})(?:\/|$)/.exec(key)?.[1];
  const looks = usb ? LOGITECH_ART[usb] : undefined;
  return looks && (looks[variant ?? 0] ?? looks[0]);
};

export const MODEL_ART: Record<number, ModelArt> = {
  1: { name: "ZI PRO", image: mid1, size: [298, 263], hotspots: {"0": [105, 203], "1": [34, 163], "2": [91, 134], "4": [216, 166], "3": [253, 131], "5": [180, 128], "13": [290, 236], "14": [246, 236], "15": [202, 236]} },
  2: { name: "Z2", image: mid2, size: [310, 276], hotspots: {"0": [99, 218], "1": [34, 183], "2": [86, 157], "4": [208, 165], "3": [240, 131], "5": [213, 210], "13": [294, 237], "14": [250, 237], "15": [206, 237]} },
  3: { name: "Z2 mini", image: mid3, size: [308, 293], hotspots: {"0": [96, 240], "1": [32, 196], "2": [88, 177], "4": [205, 186], "3": [231, 141], "5": [167, 167], "13": [291, 251], "14": [247, 251], "15": [203, 251]} },
  4: { name: "Z1 PRO MAX", image: mid4, size: [298, 263], hotspots: {"0": [105, 203], "1": [34, 163], "2": [91, 134], "4": [216, 166], "3": [253, 131], "5": [180, 128], "13": [290, 236], "14": [246, 236], "15": [202, 236]} },
  5: { name: "Z2 MAX", image: mid5, size: [310, 276], hotspots: {"0": [99, 218], "1": [34, 183], "2": [86, 157], "4": [208, 165], "3": [240, 131], "5": [213, 210], "13": [294, 237], "14": [250, 237], "15": [206, 237]} },
  6: { name: "Z2 mini MAX", image: mid6, size: [308, 293], hotspots: {"0": [96, 240], "1": [32, 196], "2": [88, 177], "4": [205, 186], "3": [231, 141], "5": [167, 167], "13": [291, 251], "14": [247, 251], "15": [203, 251]} },
};

/** Logitech's top view from G HUB, turned into the frame above, with G HUB's button markers moved
 *  along (see NOTICE.md). Keys are the profile's buttons: left, right, middle, back, forward. */
const SUPERLIGHT_HOTSPOTS: Record<number, [number, number]> = { 0: [72, 108], 1: [126, 55], 2: [90, 72], 3: [111, 190], 4: [79, 161] };
const superlight = (image: string): ModelArt => ({ name: "PRO X SUPERLIGHT", image, size: [298, 263], hotspots: SUPERLIGHT_HOTSPOTS, view: "viewed from above" });

/** Logitech's top view from its product page, turned the same way; the buttons placed where the
 *  first Superlight's are on its own shape (see NOTICE.md). */
const SUPERLIGHT_2_HOTSPOTS: Record<number, [number, number]> = { 0: [72, 109], 1: [126, 54], 2: [90, 72], 3: [111, 191], 4: [78, 162] };
const superlight2 = (image: string): ModelArt => ({ name: "PRO X SUPERLIGHT 2", image, size: [298, 263], hotspots: SUPERLIGHT_2_HOTSPOTS, view: "viewed from above" });

/** A Logitech's pictures by its USB product ID, then by its colour (the extended model ID). A
 *  colour without its own picture shows the first. */
const LOGITECH_ART: Record<string, ModelArt[]> = {
  c094: [superlight(superlightBlack), superlight(superlightWhite), superlight(superlightMagenta)],
  // Assumed in the first Superlight's order (G HUB's data for this one isn't public): to confirm
  // with an owner.
  c09b: [superlight2(superlight2Black), superlight2(superlight2White), superlight2(superlight2Pink)],
};
