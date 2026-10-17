import { HudPanel } from "@/cards/hud/HudPanel";
import { useStore } from "@/store";

export function HudPage() {
  const prefs = useStore((x) => x.prefs);
  return prefs ? <HudPanel prefs={prefs} /> : null;
}
