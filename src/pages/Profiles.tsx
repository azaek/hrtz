import { ProfilesPanel } from "@/cards/profiles/ProfilesPanel";
import { useMouse, useStore } from "@/store";

export function ProfilesPage() {
  const prefs = useStore((x) => x.prefs);
  const mouse = useMouse();
  return prefs && mouse ? <ProfilesPanel prefs={prefs} mouse={mouse} /> : null;
}
