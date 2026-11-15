import { ButtonsPanel } from "@/cards/buttons/ButtonsPanel";
import { artOf, familyOf } from "@/models";
import { useStore } from "@/store";

/** ATK's makers: their mice are in the Zaopin's family, with their own app's functions. */
const ATK_BRANDS = ["ATK", "VXE", "MAD"];

export function ButtonsPage() {
  const device = useStore((x) => x.device!);
  const family = familyOf(device.key) ?? "compx";
  return (
    <ButtonsPanel
      s={device.settings!}
      caps={device.capabilities!}
      art={artOf(device.key, device.variant)}
      online={device.online}
      family={family === "compx" && ATK_BRANDS.includes(device.brand ?? "") ? "atk" : family}
    />
  );
}
