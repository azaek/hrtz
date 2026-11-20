import type { ButtonAction } from "./api";
import { msg, screen, t, type Speech } from "./i18n";

/** KeyboardEvent.code → HID keyboard usage. */
export const KEY_USAGE: Record<string, number> = (() => {
  const m: Record<string, number> = {};
  "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").forEach((c, i) => (m[`Key${c}`] = 0x04 + i));
  "123456789".split("").forEach((c, i) => (m[`Digit${c}`] = 0x1e + i));
  m.Digit0 = 0x27;
  Object.assign(m, {
    Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c, Minus: 0x2d, Equal: 0x2e,
    BracketLeft: 0x2f, BracketRight: 0x30, Backslash: 0x31, Semicolon: 0x33, Quote: 0x34, Backquote: 0x35,
    Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39, PrintScreen: 0x46, ScrollLock: 0x47, Pause: 0x48,
    Insert: 0x49, Home: 0x4a, PageUp: 0x4b, Delete: 0x4c, End: 0x4d, PageDown: 0x4e, ArrowRight: 0x4f,
    ArrowLeft: 0x50, ArrowDown: 0x51, ArrowUp: 0x52, NumLock: 0x53, NumpadDivide: 0x54, NumpadMultiply: 0x55,
    NumpadSubtract: 0x56, NumpadAdd: 0x57, NumpadEnter: 0x58, NumpadDecimal: 0x63, IntlBackslash: 0x64,
    ContextMenu: 0x65,
  });
  for (let i = 1; i <= 9; i++) m[`Numpad${i}`] = 0x58 + i;
  m.Numpad0 = 0x62;
  for (let i = 1; i <= 12; i++) m[`F${i}`] = 0x39 + i;
  for (let i = 13; i <= 24; i++) m[`F${i}`] = 0x68 + (i - 13);
  return m;
})();

const USAGE_NAME: Record<number, string> = (() => {
  const pretty: Record<string, string> = {
    Escape: "Esc", Backspace: "Backspace", Space: "Space", Minus: "-", Equal: "=", BracketLeft: "[",
    BracketRight: "]", Backslash: "\\", Semicolon: ";", Quote: "'", Backquote: "`", Comma: ",", Period: ".",
    Slash: "/", ArrowRight: "→", ArrowLeft: "←", ArrowDown: "↓", ArrowUp: "↑", PageUp: "Page Up",
    PageDown: "Page Down", PrintScreen: "Print Screen", ContextMenu: "Menu",
  };
  const out: Record<number, string> = {};
  for (const [code, usage] of Object.entries(KEY_USAGE)) {
    out[usage] = pretty[code] ?? code.replace(/^Key/, "").replace(/^Digit/, "").replace(/^Numpad/, "Num ");
  }
  return out;
})();

export const MODIFIER_BIT: Record<string, number> = {
  ControlLeft: 1, ShiftLeft: 2, AltLeft: 4, MetaLeft: 8,
  ControlRight: 16, ShiftRight: 32, AltRight: 64, MetaRight: 128,
};

export function modifierNames(bits: number): string[] {
  const names: string[] = [];
  if (bits & 0x11) names.push("Ctrl");
  if (bits & 0x22) names.push("Shift");
  if (bits & 0x44) names.push("Alt");
  if (bits & 0x88) names.push("Win");
  return names;
}

export function comboLabel(modifiers: number, keys: number[], s: Speech = screen): string {
  const parts = [...modifierNames(modifiers), ...keys.map((k) => USAGE_NAME[k] ?? `0x${k.toString(16)}`)];
  return parts.length ? parts.join(" + ") : s.t("No keys set");
}

/** A media key: its name in English (`key`), and read in the language shown (`name`). */
const media = (usage: number, key: string) => ({
  usage,
  key,
  get name() {
    return t(key);
  },
});

/** Consumer-page usages offered by the vendor app. */
export const MEDIA: { usage: number; key: string; name: string }[] = [
  media(0x00cd, msg("Play / pause")),
  media(0x00b5, msg("Next track")),
  media(0x00b6, msg("Previous track")),
  media(0x00b7, msg("Stop")),
  media(0x00e2, msg("Mute")),
  media(0x00e9, msg("Volume up")),
  media(0x00ea, msg("Volume down")),
  media(0x0183, msg("Media player")),
  media(0x018a, msg("Email")),
  media(0x0192, msg("Calculator")),
  media(0x0194, msg("This PC")),
  media(0x0223, msg("Browser home")),
  media(0x0221, msg("Search")),
  media(0x0224, msg("Browser back")),
  media(0x0225, msg("Browser forward")),
  media(0x0227, msg("Refresh")),
  media(0x022a, msg("Favourites")),
];

/** The mouse buttons, to read: "Left" as the button (not the direction). `t(name, { context:
 *  "button" })` finds these for a name kept in English. */
export const buttonNames = (s: Speech = screen) => ({
  left: s.t("Left", { context: "button" }),
  right: s.t("Right", { context: "button" }),
  middle: s.t("Middle", { context: "button" }),
  back: s.t("Back", { context: "button" }),
  forward: s.t("Forward", { context: "button" }),
});

/** A button's name as the mouse code gives it ("Left", "Wheel click", "Button 7"), to read in `s`.
 *  The code keeps them in English for the report; a name it doesn't know stays as it is. */
export function physicalName(name: string, s: Speech = screen): string {
  if (["Left", "Right", "Back", "Forward"].includes(name)) return s.t(name, { context: "button" });
  if (name === "Wheel click") return s.t("Wheel click");
  if (name === "DPI button") return s.t("DPI button");
  const n = /^Button (\d+)$/.exec(name)?.[1];
  return n ? s.t("Button {{n}}", { n: s.num(Number(n)) }) : name;
}

const mouseName = (s: Speech) => ({ left: s.t("Left click"), right: s.t("Right click"), middle: s.t("Middle click"), back: buttonNames(s).back, forward: buttonNames(s).forward });

/** What a button does, to read (in English with `english`, for the mouse check's report). */
export function actionLabel(a: ButtonAction, s: Speech = screen): string {
  const { t, num } = s;
  switch (a.kind) {
    case "disabled":
      return t("Disabled");
    case "mouse":
      return mouseName(s)[a.button];
    case "dpi":
      return { cycle: t("Cycle DPI stages"), up: t("DPI up"), down: t("DPI down"), default: t("Default DPI stage") }[a.action];
    case "scroll":
      return { up: t("Scroll up"), down: t("Scroll down"), left: t("Scroll left"), right: t("Scroll right") }[a.direction];
    case "fire":
      return a.times === 0
        ? t("Rapid fire every {{ms}} ms while held", { ms: num(a.intervalMs) })
        : t("Rapid fire ×{{times}}, {{ms}} ms apart", { times: num(a.times), ms: num(a.intervalMs) });
    case "pollingSwitch":
      return t("Cycle polling rate");
    case "dpiLock":
      return t("Hold for {{dpi}} DPI", { dpi: a.dpi });
    case "media": {
      const media = MEDIA.find((m) => m.usage === a.usage);
      return media ? t(media.key) : t("Media key {{code}}", { code: `0x${a.usage.toString(16)}` });
    }
    case "keys":
      return comboLabel(a.modifiers, a.keys, s);
    case "macro":
      return t("Macro {{number}}", { number: a.slot + 1 });
    case "raw":
      return t("Vendor function {{code}}", { code: a.code });
    case "profile":
      return { next: t("Next profile"), previous: t("Previous profile"), cycle: t("Cycle profiles") }[a.action];
    case "vendor":
      return vendorLabel(a.bytes, s);
  }
}

/** A Logitech record hrtz keeps as it is, by what G HUB calls it. */
function vendorLabel(b: number[], { t }: Speech): string {
  if (b[0] === 0x00) return t("Macro");
  if (b[0] === 0x90) return { 0x07: t("DPI Shift"), 0x0b: "G-Shift", 0x0c: t("Battery level") }[b[1]] ?? t("Set in G HUB");
  return t("Set in G HUB");
}
