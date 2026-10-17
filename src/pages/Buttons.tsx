import { ButtonsPanel } from "@/cards/buttons/ButtonsPanel";
import { MODEL_ART } from "@/models";
import { useStore } from "@/store";

export function ButtonsPage() {
  const device = useStore((x) => x.device!);
  return <ButtonsPanel s={device.settings!} caps={device.capabilities!} art={MODEL_ART[device.mid]} online={device.online} />;
}
