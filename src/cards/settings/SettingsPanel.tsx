import { useEffect, useState } from "react";
import { Body1, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Switch } from "@fluentui/react-components";
import {
  Alert20Regular,
  ArrowRotateClockwise20Regular,
  Bug20Regular,
  DataUsage20Regular,
  Desktop20Regular,
  Info20Regular,
  PersonFeedback20Regular,
  PlugConnected20Regular,
  Power20Regular,
  Wifi120Regular,
} from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, message, on, type Prefs } from "@/api";
import { Choice, Group, SettingCard } from "@/components";
import { linkName } from "@/links";
import { familyOf, ofModel } from "@/models";
import { reportUrl, type ReportForm } from "@/report";
import { SHARING_READY } from "@/sharing";
import { run, savePrefs, toast, useStore } from "@/store";
import { BackupsPanel } from "../backups/BackupsPanel";
import { AppCard } from "./AppCard";

const muted = { color: "var(--colorNeutralForeground3)" };

/**
 * Settings: hrtz's own card on top, then App, Mouse, Backups, Privacy (hidden until sharing is
 * ready) and Help, one line of text per setting.
 * The tray style is a choice with its two real icons; the mouse's facts are its row's
 * description; the not-affiliated note is the page's small print. Decided in the Settings lab
 * (DECISIONS.md, "Settings page").
 */
export function SettingsPanel({ prefs }: { prefs: Prefs }) {
  const device = useStore((x) => x.device);
  const busy = useStore((x) => x.busy);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [confirmReset, setConfirmReset] = useState(false);
  const [resetting, setResetting] = useState(false);
  const progress = useStore((x) => x.progress);
  const [previews, setPreviews] = useState<{ percent?: string; battery?: string; taskbar: "light" | "dark" } | null>(null);
  // Pairing: the seconds the receiver keeps looking, and how the last try went.
  const [pairing, setPairing] = useState<number | null>(null);
  const [paired, setPaired] = useState<boolean | null>(null);
  useEffect(() => {
    const off = on.pairing(setPairing);
    return () => void off.then((f) => f());
  }, []);
  const pair = async () => {
    setPaired(null);
    setPairing(30);
    const ok = await run(() => api.pairReceiver());
    setPairing(null);
    if (ok !== undefined) setPaired(ok);
  };
  // The previews show the battery as it is, so redraw them when it changes.
  const batteryKey = `${device?.battery?.level}-${device?.battery?.charging}-${device?.online}-${prefs.lowBatteryPercent}`;
  useEffect(() => {
    api.trayPreviews().then(setPreviews, () => setPreviews(null));
  }, [batteryKey]);
  useEffect(() => {
    isEnabled().then(setAutostart, () => setAutostart(null));
  }, []);

  const set = (patch: Partial<Prefs>) => savePrefs({ ...prefs, ...patch });
  const report = (form: ReportForm) => void reportUrl(form, device).then(openUrl).catch((e) => toast(message(e), "error"));
  // A mouse hrtz recognises but hasn't been tried on: its owner's report is what makes it supported.
  const supported = useStore((x) => x.supported);
  const untested = device?.connected && supported.some((m) => ofModel(device.key, m.key) && !m.tested) ? device.model : null;
  const caps = device?.capabilities;
  const wired = !!caps?.wired;
  // The small print names the maker of the mouse shown.
  const family = familyOf(device?.key);
  const maker = family ? { compx: "Zaopin", hidpp: "Logitech" }[family] : null;
  // A row this mouse can't use is left out; with no mouse they all show, waiting for one.
  const can = (feature: "longRange" | "pairing" | "factoryReset" | "backups") => !caps || caps[feature];
  const noLongRange = !device?.connected || device.longRange === null || wired;
  const facts = device?.connected
    ? [
        `Firmware ${device.firmware ?? "unknown until it wakes"}`,
        linkName(device),
        familyOf(device.key) === "compx" ? `CID ${device.cid} · MID ${device.mid}` : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : null;
  // The icon as the taskbar draws it. A light taskbar's dark icon gets its light backing, or it
  // would vanish on the dark choice.
  const trayIcon = (src?: string) =>
    src ? (
      <span className="grid size-5 place-items-center rounded-[3px]" style={{ background: previews?.taskbar === "light" ? "#f3f3f3" : undefined }}>
        <img src={src} alt="" width={16} height={16} />
      </span>
    ) : null;

  return (
    <>
      <AppCard />

      <Group title="App">
        <SettingCard icon={<Power20Regular />} title="Start with Windows" description="Starts in the tray, so alerts and per-app profiles keep working.">
          <Switch
            checked={!!autostart}
            disabled={autostart === null}
            aria-label="Start with Windows"
            onChange={async (_, d) => {
              try {
                await (d.checked ? enable() : disable());
                setAutostart(await isEnabled());
              } catch (e) {
                toast(message(e), "error");
              }
            }}
          />
        </SettingCard>
        <SettingCard
          icon={<Alert20Regular />}
          title="DPI and profile pop-up"
          // While the HUD is on, the backend keeps this pop-up out of games in every Show mode, even
          // where the HUD wouldn't show the change itself (while held, or with that tab off).
          description={prefs.hud && prefs.hud.show !== "off" ? "Shows the new value on screen, except in games with the HUD on." : "Shows the new value near the bottom of the screen."}
        >
          <Switch checked={prefs.osd} onChange={(_, d) => set({ osd: d.checked })} aria-label="DPI and profile pop-up" />
        </SettingCard>
        <SettingCard icon={<Desktop20Regular />} title="Tray icon" description="Hover for battery, polling rate and connection.">
          <Choice
            label="Tray icon style"
            value={prefs.trayStyle}
            options={[
              {
                value: "percent",
                label: (
                  <span className="tw flex items-center gap-2">
                    {trayIcon(previews?.percent)}
                    Percentage
                  </span>
                ),
              },
              {
                value: "battery",
                label: (
                  <span className="tw flex items-center gap-2">
                    {trayIcon(previews?.battery)}
                    Battery
                  </span>
                ),
              },
            ]}
            onChange={(trayStyle) => set({ trayStyle })}
          />
        </SettingCard>
      </Group>

      <Group title="Mouse">
        {can("longRange") && (
          <SettingCard
            icon={<Wifi120Regular />}
            title="Long-range mode"
            description={
              wired
                ? "Wireless only. Unplug the cable to change it."
                : noLongRange
                  ? "Available when the mouse is awake and wireless."
                  : "A stronger signal with less interference. Uses more battery."
            }
          >
            <Switch
              checked={!!device?.longRange}
              disabled={!device?.online || noLongRange || busy}
              aria-label="Long-range mode"
              onChange={(_, d) => run(() => api.setLongRange(d.checked))}
            />
          </SettingCard>
        )}
        {can("pairing") && (
          <SettingCard
            icon={<PlugConnected20Regular />}
            title="Pair a mouse"
            description={
              wired
                ? "Wireless only. Unplug the cable first."
                : !device?.connected
                  ? "Plug in the receiver first."
                  : pairing !== null
                    ? `Hold the left, right and middle buttons together until the light flashes yellow. ${pairing} s left.`
                    : paired === true
                      ? "Paired. The mouse will connect shortly."
                      : paired === false
                        ? "No mouse found in 30 seconds. Turn the mouse on and try again."
                        : "For a new receiver or a mouse that won't connect. Searches for 30 seconds."
            }
          >
            <Button disabled={wired || !device?.connected || busy} onClick={() => void pair()}>
              {pairing !== null ? "Pairing…" : "Pair"}
            </Button>
          </SettingCard>
        )}
        <SettingCard icon={<Info20Regular />} title={device?.model ?? "No mouse connected"} description={facts ?? "Plug in the receiver to see its details."} />
        {can("factoryReset") && (
          <SettingCard
            icon={<ArrowRotateClockwise20Regular />}
            title="Restore factory settings"
            description={
              !resetting
                ? "Resets DPI, polling, buttons and macros to factory defaults."
                : progress
                  ? `Saving a backup first · ${Math.round((progress[0] / progress[1]) * 100)}%`
                  : "Restoring factory settings…"
            }
          >
            <Button disabled={!device?.online || busy} onClick={() => setConfirmReset(true)}>
              Restore
            </Button>
          </SettingCard>
        )}
      </Group>

      {can("backups") && <BackupsPanel />}

      {SHARING_READY && (
        <Group title="Privacy">
          <SettingCard
            icon={<DataUsage20Regular />}
            title="Help improve hrtz"
            description="Shares anonymous lift and battery data, never your movement or apps. Coming in a later version."
          >
            <Switch checked={prefs.shareData} onChange={(_, d) => set({ shareData: d.checked })} aria-label="Help improve hrtz" />
          </SettingCard>
        </Group>
      )}

      <Group title="Help">
        <SettingCard icon={<Bug20Regular />} title="Report a problem" description="Opens a GitHub issue with your hrtz version, mouse and Windows filled in.">
          <Button onClick={() => report("bug_report")}>Report</Button>
        </SettingCard>
        {untested && (
          <SettingCard
            icon={<PersonFeedback20Regular />}
            title={`Your ${untested} isn't tested yet`}
            description="A short report on what works helps make it supported."
          >
            <Button onClick={() => report("mouse_report")}>Send a report</Button>
          </SettingCard>
        )}
      </Group>

      <Caption1 style={{ ...muted, padding: "8px 2px 0" }}>
        hrtz isn't affiliated with {maker ? `${maker} or any other mouse maker` : "any mouse maker"}. Mouse images and model names belong to their makers and are shown only to identify your mouse.
      </Caption1>

      {/* Factory reset is device-wide, so it's the one thing that asks first. */}
      <Dialog open={confirmReset} onOpenChange={(_, d) => setConfirmReset(d.open)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>Restore factory settings?</DialogTitle>
            <DialogContent>
              <Body1>
                Everything on the mouse returns to its defaults, including buttons and macros. A backup is saved first, so you can restore this setup from Backups below.
              </Body1>
            </DialogContent>
            <DialogActions>
              <Button onClick={() => setConfirmReset(false)}>Cancel</Button>
              <Button
                appearance="primary"
                onClick={async () => {
                  setConfirmReset(false);
                  setResetting(true);
                  await run(() => api.factoryReset(), "Factory settings restored");
                  setResetting(false);
                }}
              >
                Restore factory settings
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </>
  );
}
