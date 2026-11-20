import type { DeviceState } from "./api";
import { msg, t } from "./i18n";
import { familyOf } from "./models";

/** By the handshake's connection type: 0 to 5 a Zaopin's, 6 to 8 ATK's too (a 2K or 4K cable, NearLink). */
export const LINK_NAME = [msg("1K receiver"), msg("4K receiver"), msg("USB cable"), msg("USB cable"), msg("2K receiver"), msg("8K receiver"), msg("USB cable"), msg("USB cable"), msg("NearLink receiver")];

/** How a mouse is connected: "8K receiver" (a Zaopin's own), "LIGHTSPEED receiver", "USB cable".
 *  In English, as the reports give it; `linkLabel` is it in the language shown. */
export function linkName(d: Pick<DeviceState, "key" | "deviceType" | "capabilities">): string {
  const family = familyOf(d.key);
  if (family === "compx") return LINK_NAME[d.deviceType] ?? msg("receiver");
  if (d.capabilities?.wired) return msg("USB cable");
  return family === "hidpp" ? msg("LIGHTSPEED receiver") : msg("receiver");
}

/** `linkName`, in the language shown. */
export const linkLabel = (d: Pick<DeviceState, "key" | "deviceType" | "capabilities">): string => t(linkName(d));
