import { msg } from "@/i18n";
import type { Swatch } from "@/kit";

/** Hues the mouse's light shows as itself. Dark or greyish ones only read as dimmer versions.
 *  The names are shown with `t()`. */
export const LED_PALETTE: Swatch[] = [
  { name: msg("Red"), hex: "#FF2D2D" },
  { name: msg("Orange"), hex: "#FF8A00" },
  { name: msg("Yellow"), hex: "#FFE100" },
  { name: msg("Green"), hex: "#2DFF5A" },
  { name: msg("Cyan"), hex: "#00E5FF" },
  { name: msg("Blue"), hex: "#2D6BFF" },
  { name: msg("Purple"), hex: "#A63DFF" },
  { name: msg("Pink"), hex: "#FF3DD8" },
  { name: msg("White"), hex: "#FFFFFF" },
];
