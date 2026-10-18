import { ButtonsPanel } from "@/cards/buttons/ButtonsPanel";
import { artOf, familyOf } from "@/models";
import { useStore } from "@/store";

export function ButtonsPage() {
  const device = useStore((x) => x.device!);
  return (
    <ButtonsPanel
      s={device.settings!}
      caps={device.capabilities!}
      art={artOf(device.key, device.variant)}
      online={device.online}
      family={familyOf(device.key) ?? "compx"}
    />
  );
}
