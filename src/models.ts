// Generated from the vendor apps' model lists and renders (credited in assets/models/NOTICE.md).
// Hotspot centres are in frame pixels; keys are the family's button indices (a Zaopin's flash
// index, a Logitech's place in its profile).

import mid1 from "./assets/models/mid1.png";
import mid2 from "./assets/models/mid2.png";
import mid3 from "./assets/models/mid3.png";
import mid4 from "./assets/models/mid4.png";
import mid5 from "./assets/models/mid5.png";
import mid6 from "./assets/models/mid6.png";
import mid9 from "./assets/models/mid9.png";
import atkR1Black from "./assets/models/atk-r1-black.webp";
import atkU2PlusBlack from "./assets/models/atk-u2-plus-black.webp";
import g305Black from "./assets/models/g305-black.webp";
import g305Blue from "./assets/models/g305-blue.webp";
import g305Lilac from "./assets/models/g305-lilac.webp";
import g305Mint from "./assets/models/g305-mint.webp";
import g305White from "./assets/models/g305-white.webp";
import superlightBlack from "./assets/models/superlight-black.webp";
import superlightMagenta from "./assets/models/superlight-magenta.webp";
import superlightWhite from "./assets/models/superlight-white.webp";
import superlight2Black from "./assets/models/superlight2-black.webp";
import superlight2Pink from "./assets/models/superlight2-pink.webp";
import superlight2White from "./assets/models/superlight2-white.webp";
import superlight2cBlack from "./assets/models/superlight2c-black.webp";
import superlight2cPink from "./assets/models/superlight2c-pink.webp";
import superlight2cWhite from "./assets/models/superlight2c-white.webp";
import { t } from "./i18n";

export interface ModelArt {
  name: string;
  image: string;
  size: [number, number];
  hotspots: Record<number, [number, number]>;
  /** How the picture shows it, for its description: "viewed from the left" unless said. In the
   *  language shown when it's read. */
  view?: string;
  /** The DPI button is visible on top, rather than underneath the mouse. */
  dpiButtonOnTop?: boolean;
}

/** The model (MID) in a Zaopin key: `compx/7-4` (or a simulated `sim/7-4`) is 4. An ATK on the same
 *  channel (`compx/2-78`) has MIDs of its own, so it has none here. */
export const midOf = (key: string | null | undefined): number | undefined => {
  const m = key && /^(?:compx|sim)\/7-(\d+)$/.exec(key);
  return m ? Number(m[1]) : undefined;
};

/** A key's family: `compx/7-4` is a Zaopin and `compx/2-78` an ATK, both on CompX's channel;
 *  `hidpp/c094/1a2b3c4d` a Logitech (`sim/` for either, simulated, keeps the rest of its shape). */
export const familyOf = (key: string | null | undefined): "compx" | "hidpp" | undefined => {
  if (key && /^(?:compx|sim)\/\d+-\d+$/.test(key)) return "compx";
  return key && /^(?:hidpp|sim)\/[0-9a-f]{4}\//.test(key) ? "hidpp" : undefined;
};

/** Who makes a mouse: as it says, or by its family, for a state from before brands were told. */
export const makerOf = (d: { key?: string | null; brand?: string | null } | null | undefined): string | undefined => {
  if (d?.brand) return d.brand;
  const family = familyOf(d?.key);
  return family && { compx: "Zaopin", hidpp: "Logitech" }[family];
};

/** A Zaopin, which Zaopin's app gets in the way of: CID 7, or a Zaopin receiver with no mouse paired (0). */
export const isZaopin = (d: { key?: string | null; cid: number } | null | undefined): boolean =>
  familyOf(d?.key) === "compx" && (d?.cid === 7 || d?.cid === 0);

/** The key a mouse's model shares: a Zaopin's key is its model's (`compx/7-4`); a Logitech's drops
 *  its unit ID (`hidpp/c094/43c2f6b0` is `hidpp/c094`). */
export const modelKeyOf = (key: string): string => (familyOf(key) === "hidpp" ? key.replace(/\/[^/]*$/, "") : key);

/** Whether backup `b` is for mouse `d`'s model: by key, or (from before several mice, with no key)
 *  a Zaopin of the same model name. */
export const backupFits = (b: { key: string | null; model: string }, d: { key?: string | null; model: string | null }): boolean =>
  b.key ? !!d.key && modelKeyOf(b.key) === modelKeyOf(d.key) : familyOf(d.key) === "compx" && b.model === d.model;

/** Whether a connected mouse's key is of the model `model` names: the same key, or (Logitech's,
 *  which add the mouse's own unit ID) that key and more. */
export const ofModel = (key: string | null | undefined, model: string): boolean => !!key && (key === model || key.startsWith(`${model}/`));

/** How far hrtz is with a mouse it runs: tried on the real model, listed but not tried yet, or a
 *  model it doesn't know (a Zaopin with another MID: not supported yet, run without its DPI and
 *  what depends on its sensor).
 *  A simulated mouse (`sim/…`) counts as the real model. Nothing for a receiver with no mouse. */
export type Support = "tested" | "untested" | "unknown";
export const supportOf = (key: string | null | undefined, supported: { key: string; tested: boolean }[]): Support | undefined => {
  const family = familyOf(key);
  if (!key || !family || /^(?:compx|sim)\/0-0$/.test(key)) return undefined;
  const real = key.replace(/^sim\//, `${family}/`);
  const m = supported.find((x) => ofModel(real, x.key));
  return m ? (m.tested ? "tested" : "untested") : "unknown";
};

/** What Inspect a mouse and the mouse card call each, in the language shown when it's read. */
export const SUPPORT_TEXT: Record<Support, string> = { get tested() { return t("Tested"); }, get untested() { return t("Not tested yet"); }, get unknown() { return t("Not supported yet"); } };

/** A mouse's picture, from its key and, for a model that comes in several colours, which it is
 *  (`DeviceState.variant`). */
export const artOf = (key: string | null | undefined, variant?: number | null): ModelArt | undefined => {
  const mid = midOf(key);
  if (mid !== undefined) return MODEL_ART[mid];
  const atk = key && /^(?:compx|sim)\/(\d+-\d+)$/.exec(key)?.[1];
  if (atk && ATK_ART[atk]) return ATK_ART[atk];
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
  // A Z2 that answers MID 11 (issue #19): the Z2's picture. Zaopin's web app places its buttons a
  // little differently on its own render, which hrtz doesn't have.
  11: { name: "Z2", image: mid2, size: [310, 276], hotspots: {"0": [99, 218], "1": [34, 183], "2": [86, 157], "4": [208, 165], "3": [240, 131], "5": [213, 210], "13": [294, 237], "14": [250, 237], "15": [206, 237]} },
  6: { name: "Z2 mini MAX", image: mid6, size: [308, 293], hotspots: {"0": [96, 240], "1": [32, 196], "2": [88, 177], "4": [205, 186], "3": [231, 141], "5": [167, 167], "13": [291, 251], "14": [247, 251], "15": [203, 251]} },
  // Zaopin's web app's render (Control HUB WEB, img/devices/mouse/0709.png), scaled into the frame;
  // the positions measured on it. Its DPI button is underneath.
  9: { name: "Z11", image: mid9, size: [298, 263], hotspots: {"0": [135, 160], "1": [40, 140], "2": [82, 152], "4": [190, 170], "3": [224, 142]} },
};

/** An ATK's picture by its CID and MID: ATK's own top view from its app (ATK HUB), turned into the
 *  frame above (see NOTICE.md). No buttons are placed: hrtz doesn't offer an ATK's yet. */
const ATK_ART: Record<string, ModelArt> = {
  "2-78": { name: "U2 Plus", image: atkU2PlusBlack, size: [298, 263], hotspots: {}, get view() { return t("viewed from above"); } },
  // VXE's R1 line shares one shell: ATK's app's R1 picture for each of its models in ATK's list.
  ...Object.fromEntries(
    ["1-20", "2-7", "2-8", "2-11", "2-12", "2-27", "2-28", "2-29", "2-30", "2-31", "2-32", "2-34", "2-57", "2-63", "2-67", "2-114", "2-116", "2-117", "2-125", "2-126", "2-127", "2-130", "2-136", "2-137"].map((k) => [
      k,
      { name: "R1", image: atkR1Black, size: [298, 263] as [number, number], hotspots: {}, get view() { return t("viewed from above"); } },
    ]),
  ),
};

/** Logitech's top view from G HUB, turned into the frame above, with G HUB's button markers moved
 *  along (see NOTICE.md). Keys are the profile's buttons: left, right, middle, back, forward. */
const SUPERLIGHT_HOTSPOTS: Record<number, [number, number]> = { 0: [72, 108], 1: [126, 55], 2: [90, 72], 3: [111, 190], 4: [79, 161] };
const superlight = (image: string): ModelArt => ({ name: "PRO X SUPERLIGHT", image, size: [298, 263], hotspots: SUPERLIGHT_HOTSPOTS, get view() { return t("viewed from above"); } });

/** Logitech's top view from its product page, turned the same way; the buttons placed where the
 *  first Superlight's are on its own shape (see NOTICE.md). */
const SUPERLIGHT_2_HOTSPOTS: Record<number, [number, number]> = { 0: [72, 109], 1: [126, 54], 2: [90, 72], 3: [111, 191], 4: [78, 162] };
const superlight2 = (image: string): ModelArt => ({ name: "PRO X SUPERLIGHT 2", image, size: [298, 263], hotspots: SUPERLIGHT_2_HOTSPOTS, get view() { return t("viewed from above"); } });

/** Logitech's top view of the 2c from its product page, turned the same way, with the buttons
 *  measured on it (see NOTICE.md). */
const SUPERLIGHT_2C_HOTSPOTS: Record<number, [number, number]> = { 0: [78, 109], 1: [127, 60], 2: [95, 77], 3: [114, 191], 4: [78, 162] };
const superlight2c = (image: string): ModelArt => ({ name: "PRO X SUPERLIGHT 2c", image, size: [298, 263], hotspots: SUPERLIGHT_2C_HOTSPOTS, get view() { return t("viewed from above"); } });

/** G HUB's top view of the G305 (the G304 is the same mouse), upright in the frame above, with
 *  the buttons measured on it (see NOTICE.md): left, right, middle, back, forward, and the DPI
 *  button, on top behind the wheel. */
const G305_HOTSPOTS: Record<number, [number, number]> = { 0: [112, 64], 1: [185, 64], 2: [150, 55], 3: [82, 143], 4: [83, 106], 5: [149, 92] };
const g305 = (name: string, image: string): ModelArt => ({ name, image, size: [298, 263], hotspots: G305_HOTSPOTS, get view() { return t("viewed from above"); }, dpiButtonOnTop: true });

/** A Logitech's pictures by its USB product ID, then by its colour (the extended model ID). A
 *  colour without its own picture shows the first. */
const LOGITECH_ART: Record<string, ModelArt[]> = {
  // G HUB's colours of 0x4074, by extended model ID, and whether it calls each a G304 or a G305.
  // The K/DA and the miHoYo editions have no picture here, so they show the black one; the SE
  // shows the white.
  "4074": [
    g305("G305", g305Black), g305("G304", g305Black),
    g305("G305", g305White), g305("G304", g305White),
    g305("G305", g305Blue), g305("G304", g305Blue),
    g305("G305", g305Lilac), g305("G304", g305Lilac),
    g305("G304", g305Black), g305("G305", g305Black),
    g305("G305", g305Mint), g305("G304", g305Mint),
    g305("G305", g305White), g305("G304", g305White),
    g305("G305", g305Black), g305("G304", g305Black),
    g305("G305", g305Black), g305("G304", g305Black),
    g305("G305", g305Black), g305("G304", g305Black),
  ],
  c094:[superlight(superlightBlack), superlight(superlightWhite), superlight(superlightMagenta)],
  // Assumed in the first Superlight's order (G HUB's data for this one isn't public): to confirm
  // with an owner.
  c09b: [superlight2(superlight2Black), superlight2(superlight2White), superlight2(superlight2Pink)],
  // Black, white and pink, assumed in the Superlight 2's order: to confirm with a 2c's colour.
  c09f: [superlight2c(superlight2cBlack), superlight2c(superlight2cWhite), superlight2c(superlight2cPink)],
};
