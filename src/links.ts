import type { DeviceState } from "./api";
import { familyOf } from "./models";

/** By the handshake's connection type: 0 to 5 a Zaopin's, 6 to 8 ATK's too (a 2K or 4K cable, NearLink). */
export const LINK_NAME = ["1K receiver", "4K receiver", "USB cable", "USB cable", "2K receiver", "8K receiver", "USB cable", "USB cable", "NearLink receiver"];

/** How a mouse is connected: "8K receiver" (a Zaopin's own), "LIGHTSPEED receiver", "USB cable". */
export function linkName(d: Pick<DeviceState, "key" | "deviceType" | "capabilities">): string {
  const family = familyOf(d.key);
  if (family === "compx") return LINK_NAME[d.deviceType] ?? "receiver";
  if (d.capabilities?.wired) return "USB cable";
  return family === "hidpp" ? "LIGHTSPEED receiver" : "receiver";
}
