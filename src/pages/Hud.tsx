import { HudPanel } from "@/cards/hud/HudPanel";
import { TrayGroup } from "@/cards/tray/TrayGroup";
import { useStore } from "@/store";

/** HUD & Tray: the in-game HUD, then the tray icon. */
export function HudPage() {
  const prefs = useStore((x) => x.prefs);
  return prefs ? (
    <>
      <HudPanel prefs={prefs} />
      <TrayGroup prefs={prefs} />
    </>
  ) : null;
}
