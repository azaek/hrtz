import { SettingsPanel } from "@/cards/settings/SettingsPanel";
import { useStore } from "@/store";

export function SettingsPage() {
  const prefs = useStore((x) => x.prefs);
  return prefs ? <SettingsPanel prefs={prefs} /> : null;
}
