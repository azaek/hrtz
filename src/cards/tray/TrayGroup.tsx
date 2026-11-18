import { useEffect, useState } from "react";
import { Switch } from "@fluentui/react-components";
import { Battery820Regular, Desktop20Regular } from "@fluentui/react-icons";
import { api, type Prefs, type TrayStyle } from "@/api";
import { Group, SettingCard } from "@/components";
import { msg, t } from "@/i18n";
import { NewBadge, TileChoice } from "@/kit";
import { useSeen } from "@/news";
import { savePrefs, useStore } from "@/store";

/** The tray icon's styles, as the tiles name them (shown with `t()`). */
const TRAY_STYLES: { value: TrayStyle; label: string }[] = [
  { value: "percent", label: msg("Percentage") },
  { value: "battery", label: msg("Battery") },
  { value: "logo", label: msg("Logo") },
  { value: "logobar", label: msg("Logo and bar") },
];

/** The tray, on the HUD & Tray page: the battery on the taskbar, and the tray icon's style, each
 *  tile showing it as the taskbar would draw it now. */
export function TrayGroup({ prefs }: { prefs: Prefs }) {
  const device = useStore((x) => x.device);
  // Windows 10's taskbar has no room for it. An older backend (the lab's) doesn't say: take it as yes.
  const taskbar = useStore((x) => x.status.taskbarSupported) !== false;
  const news = useSeen("taskbar");
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
    <Group title={t("Tray")}>
      <SettingCard
        icon={<Battery820Regular />}
        title={
          <>
            {t("Battery on the taskbar")} {news.isNew && <NewBadge ref={news.ref} />}
          </>
        }
        description={taskbar ? t("Your mouse and its battery beside the tray icons. Click it to open hrtz.") : t("Needs Windows 11.")}
      >
        <Switch
          checked={taskbar && prefs.taskbarBattery}
          disabled={!taskbar}
          onChange={(_, d) => void savePrefs({ ...prefs, taskbarBattery: d.checked })}
          aria-label={t("Battery on the taskbar")}
        />
      </SettingCard>
      <SettingCard
        stacked
        icon={<Desktop20Regular />}
        title={t("Tray icon")}
        description={t("Hover it to see each mouse's battery, polling rate, DPI and profile.")}
        below={
          <div style={{ marginTop: 12 }}>
            <TileChoice
              label={t("Tray icon style")}
              value={prefs.trayStyle}
              options={TRAY_STYLES.map((s) => ({ value: s.value, label: t(s.label), icon: trayIcon(previews?.[s.value]) }))}
              onChange={(trayStyle) => void savePrefs({ ...prefs, trayStyle })}
            />
          </div>
        }
      />
    </Group>
  );
}
