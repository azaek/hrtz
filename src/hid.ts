import type { ButtonAction } from "./api";

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

export function comboLabel(modifiers: number, keys: number[]): string {
  const parts = [...modifierNames(modifiers), ...keys.map((k) => USAGE_NAME[k] ?? `0x${k.toString(16)}`)];
  return parts.length ? parts.join(" + ") : "No keys set";
}

/** Consumer-page usages offered by the vendor app. */
export const MEDIA: { usage: number; name: string }[] = [
  { usage: 0x00cd, name: "Play / pause" },
  { usage: 0x00b5, name: "Next track" },
  { usage: 0x00b6, name: "Previous track" },
  { usage: 0x00b7, name: "Stop" },
  { usage: 0x00e2, name: "Mute" },
  { usage: 0x00e9, name: "Volume up" },
  { usage: 0x00ea, name: "Volume down" },
  { usage: 0x0183, name: "Media player" },
  { usage: 0x018a, name: "Email" },
  { usage: 0x0192, name: "Calculator" },
  { usage: 0x0194, name: "This PC" },
  { usage: 0x0223, name: "Browser home" },
  { usage: 0x0221, name: "Search" },
  { usage: 0x0224, name: "Browser back" },
  { usage: 0x0225, name: "Browser forward" },
  { usage: 0x0227, name: "Refresh" },
  { usage: 0x022a, name: "Favourites" },
];

const MOUSE_NAME = { left: "Left click", right: "Right click", middle: "Middle click", back: "Back", forward: "Forward" };

export function actionLabel(a: ButtonAction): string {
  switch (a.kind) {
    case "disabled":
      return "Disabled";
    case "mouse":
      return MOUSE_NAME[a.button];
    case "dpi":
      return { cycle: "Cycle DPI stages", up: "DPI up", down: "DPI down", default: "Default DPI stage" }[a.action];
    case "scroll":
      return { up: "Scroll up", down: "Scroll down", left: "Scroll left", right: "Scroll right" }[a.direction];
    case "fire":
      return a.times === 0 ? `Rapid fire every ${a.intervalMs} ms while held` : `Rapid fire ×${a.times}, ${a.intervalMs} ms apart`;
    case "pollingSwitch":
      return "Cycle polling rate";
    case "dpiLock":
      return `Hold for ${a.dpi} DPI`;
    case "media":
      return MEDIA.find((m) => m.usage === a.usage)?.name ?? `Media key 0x${a.usage.toString(16)}`;
    case "keys":
      return comboLabel(a.modifiers, a.keys);
    case "macro":
      return `Macro ${a.slot + 1}`;
    case "raw":
      return `Vendor function ${a.code}`;
    case "profile":
      return { next: "Next profile", previous: "Previous profile", cycle: "Cycle profiles" }[a.action];
    case "vendor":
      return vendorLabel(a.bytes);
  }
}

/** A Logitech record hrtz keeps as it is, by what G HUB calls it. */
function vendorLabel(b: number[]): string {
  if (b[0] === 0x00) return "Macro";
  if (b[0] === 0x90) return { 0x07: "DPI Shift", 0x0b: "G-Shift", 0x0c: "Battery level" }[b[1]] ?? "Set in G HUB";
  return "Set in G HUB";
}
