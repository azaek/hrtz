import { useEffect, useState } from "react";
import {
  Body1,
  Button,
  Caption1,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  makeStyles,
  Switch,
  tokens,
} from "@fluentui/react-components";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { message } from "./api";
import { SHARING_READY } from "./sharing";
import { savePrefs, toast, useStore } from "./store";

/** Dispatch on `window` to open the window again (the pop-up tester). */
export const OPEN_WELCOME = "hrtz:welcome";

const useStyles = makeStyles({
  surface: { maxWidth: "520px" },
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalL },
  options: { display: "flex", flexDirection: "column", gap: "2px" },
  // Each choice as a row like the Settings cards: what it is and why on the left, its switch on
  // the right.
  option: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) auto",
    alignItems: "center",
    gap: tokens.spacingHorizontalL,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
  },
  text: { display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 },
  muted: { color: tokens.colorNeutralForeground3 },
});

/**
 * The first-run window: start with Windows (on, since most of what the app adds needs it
 * running), the low battery alert and, once it's ready, sharing anonymous data (off). Shown once;
 * closing it any way keeps the choices as shown.
 */
export function Welcome() {
  const s = useStyles();
  const prefs = useStore((x) => x.prefs);
  const [forced, setForced] = useState(false);
  const [startup, setStartup] = useState(true);
  const [alerts, setAlerts] = useState(true);
  const [share, setShare] = useState(false);
  const open = !!prefs && (!prefs.welcomed || forced);

  // Opened again from the tester: start from what's set now.
  useEffect(() => {
    const again = () => {
      setForced(true);
      isEnabled().then(setStartup, () => {});
    };
    window.addEventListener(OPEN_WELCOME, again);
    return () => window.removeEventListener(OPEN_WELCOME, again);
  }, []);
  useEffect(() => {
    if (!prefs || prefs.welcomed) return;
    setAlerts(prefs.batteryAlerts);
    setShare(prefs.shareData);
  }, [prefs?.welcomed]);

  const finish = async () => {
    if (!prefs) return;
    setForced(false);
    try {
      await (startup ? enable() : disable());
    } catch (e) {
      toast(message(e), "error");
    }
    await savePrefs({ ...prefs, welcomed: true, batteryAlerts: alerts, shareData: SHARING_READY && share });
  };

  return (
    <Dialog open={open} modalType="alert" onOpenChange={(_, d) => !d.open && finish()}>
      <DialogSurface className={s.surface}>
        <DialogBody>
          <DialogTitle>Thanks for trying hrtz</DialogTitle>
          <DialogContent className={s.body}>
            <Body1>
              hrtz is a free, open-source app for gaming mice. Settings are stored on the mouse and work without the app, but some features need
              it running. You can change these later in Settings.
            </Body1>
            <div className={s.options}>
              <label className={s.option}>
                <span className={s.text}>
                  <Body1>Start with Windows</Body1>
                  <Caption1 className={s.muted}>
                    Runs in the tray to keep battery history and estimates, switch profiles per app, and map your mousepad use.
                  </Caption1>
                </span>
                <Switch checked={startup} onChange={(_, d) => setStartup(d.checked)} aria-label="Start with Windows" />
              </label>
              <label className={s.option}>
                <span className={s.text}>
                  <Body1>Low battery alert</Body1>
                  <Caption1 className={s.muted}>A Windows notification when the battery drops to {prefs?.lowBatteryPercent ?? 15}%.</Caption1>
                </span>
                <Switch checked={alerts} onChange={(_, d) => setAlerts(d.checked)} aria-label="Low battery alert" />
              </label>
              {SHARING_READY && (
                <label className={s.option}>
                  <span className={s.text}>
                    <Body1>Help improve it</Body1>
                    <Caption1 className={s.muted}>
                      Share anonymous data, such as battery drain per polling rate and pad size, to improve lift detection and battery estimates. Never
                      your movement, apps or anything that identifies you. Coming in a later version, and you'll see exactly what's sent first.
                    </Caption1>
                  </span>
                  <Switch checked={share} onChange={(_, d) => setShare(d.checked)} aria-label="Help improve it" />
                </label>
              )}
            </div>
            <Caption1 className={s.muted}>Closing the window keeps hrtz in the tray. Quit from the tray icon.</Caption1>
          </DialogContent>
          <DialogActions>
            <Button appearance="primary" onClick={finish}>
              Get started
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
