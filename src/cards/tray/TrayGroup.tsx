import { useEffect, useState } from "react";
import { Desktop20Regular } from "@fluentui/react-icons";
import { api, type Prefs, type TrayStyle } from "@/api";
import { Group, SettingCard } from "@/components";
import { TileChoice } from "@/kit";
import { savePrefs, useStore } from "@/store";

/** The tray icon's styles, as the tiles name them. */
const TRAY_STYLES: { value: TrayStyle; label: string }[] = [
  { value: "percent", label: "Percentage" },
  { value: "battery", label: "Battery" },
  { value: "logo", label: "Logo" },
  { value: "logobar", label: "Logo and bar" },
];

/** The tray, on the HUD & Tray page: its icon's style, each tile showing it as the taskbar would
 *  draw it now. */
export function TrayGroup({ prefs }: { prefs: Prefs }) {
  const device = useStore((x) => x.device);
  const [previews, setPreviews] = useState<Awaited<ReturnType<typeof api.trayPreviews>> | null>(null);
  // The previews show the battery as it is, so redraw them when it changes.
  const batteryKey = `${device?.battery?.level}-${device?.battery?.charging}-${device?.online}-${prefs.lowBatteryPercent}`;
  useEffect(() => {
    api.trayPreviews().then(setPreviews, () => setPreviews(null));
  }, [batteryKey]);

  // The icon as the taskbar draws it. A light taskbar's dark icon gets its light backing, or it
  // would vanish on the dark tile.
  const trayIcon = (src?: string) =>
    src ? (
      <span className="grid size-5 place-items-center rounded-[3px]" style={{ background: previews?.taskbar === "light" ? "#f3f3f3" : undefined }}>
        <img src={src} alt="" width={16} height={16} />
      </span>
    ) : (
      <span className="size-5" />
    );

  return (
    <Group title="Tray">
      <SettingCard
        stacked
        icon={<Desktop20Regular />}
        title="Tray icon"
        description="Hover it to see each mouse's battery, polling rate, DPI and profile."
        below={
          <div style={{ marginTop: 12 }}>
            <TileChoice
              label="Tray icon style"
              value={prefs.trayStyle}
              options={TRAY_STYLES.map((t) => ({ value: t.value, label: t.label, icon: trayIcon(previews?.[t.value]) }))}
              onChange={(trayStyle) => void savePrefs({ ...prefs, trayStyle })}
            />
          </div>
        }
      />
    </Group>
  );
}
