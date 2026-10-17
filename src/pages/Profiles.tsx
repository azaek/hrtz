import { ProfilesPanel } from "@/cards/profiles/ProfilesPanel";
import { useStore } from "@/store";

export function ProfilesPage() {
  const prefs = useStore((x) => x.prefs);
  return prefs ? <ProfilesPanel prefs={prefs} /> : null;
}
