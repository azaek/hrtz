import { useEffect, useState } from "react";
import { Body1, Button, Caption1, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Link, Option, Switch } from "@fluentui/react-components";
import {
  Alert20Regular,
  ArrowRotateClockwise20Regular,
  Bug20Regular,
  SearchInfo20Regular,
  DataUsage20Regular,
  Delete20Regular,
  Info20Regular,
  LocalLanguage20Regular,
  PlugConnected20Regular,
  Power20Regular,
  Wifi120Regular,
} from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, message, on, type KnownMouse, type Prefs } from "@/api";
import { Group, SettingCard } from "@/components";
import { UndoNote } from "@/kit";
import { Trans } from "react-i18next";
import { LANGUAGES, languageFor, num, t, windowsLanguage } from "@/i18n";
import { linkLabel } from "@/links";
import { familyOf, makerOf, supportOf } from "@/models";
import { reportUrl, translationUrl } from "@/report";
import { SHARING_READY } from "@/sharing";
import { forgetMouse, restoreMouse, run, savePrefs, setInspecting, toast, useStore } from "@/store";
import { BackupsPanel } from "../backups/BackupsPanel";
import { ParticipateCard } from "../participate/ParticipateCard";
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
  // Pairing: the seconds the receiver keeps looking, and how the last try went.
  const [pairing, setPairing] = useState<number | null>(null);
  const [paired, setPaired] = useState<boolean | null>(null);
  // A mouse just removed, for Undo, which lasts 6 s as other removals' do.
  const [removed, setRemoved] = useState<KnownMouse | null>(null);
  useEffect(() => {
    if (!removed) return;
    const t = setTimeout(() => setRemoved(null), 6000);
    return () => clearTimeout(t);
  }, [removed]);
  const remove = async (key: string) => setRemoved(await forgetMouse(key));
  const undoRemove = (m: KnownMouse) => {
    setRemoved(null);
    void restoreMouse(m);
  };
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
  useEffect(() => {
    isEnabled().then(setAutostart, () => setAutostart(null));
  }, []);

  const set = (patch: Partial<Prefs>) => savePrefs({ ...prefs, ...patch });
  const report = () => void reportUrl(device).then(openUrl).catch((e) => toast(message(e), "error"));
  // A model hrtz hasn't been tried on, or doesn't know: its owner's report is what makes it supported.
  const supported = useStore((x) => x.supported);
  const support = device?.connected ? supportOf(device.key, supported) : undefined;
  const caps = device?.capabilities;
  const wired = !!caps?.wired;
  // The small print names the maker of the mouse shown.
  const family = familyOf(device?.key);
  const maker = makerOf(device) ?? null;
  // A row this mouse can't use is left out, and so is one hrtz doesn't know it has yet (a mouse met
  // but never read); with no mouse they all show, waiting for one.
  const can = (feature: "longRange" | "pairing" | "factoryReset" | "backups") => (caps ? !!caps[feature] : !device);
  const noLongRange = !device?.connected || device.longRange === null || wired;
  const facts = device?.connected
    ? [
        device.firmware != null ? t("Firmware {{version}}", { version: device.firmware }) : t("Firmware unknown until it wakes"),
        linkLabel(device),
        familyOf(device.key) === "compx" ? `CID ${device.cid} · MID ${device.mid}` : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : null;
  return (
    <>
      <AppCard />

      <Group title={t("App")}>
        <SettingCard icon={<Power20Regular />} title={t("Start with Windows")} description={t("Starts in the tray, so alerts and per-app profiles keep working.")}>
          <Switch
            checked={!!autostart}
            disabled={autostart === null}
            aria-label={t("Start with Windows")}
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
          title={t("DPI and profile pop-up")}
          // While the HUD is on, the backend keeps this pop-up out of games in every Show mode, even
          // where the HUD wouldn't show the change itself (while held, or with that tab off).
          description={prefs.hud && prefs.hud.show !== "off" ? t("Shows the new value on screen, except in games with the HUD on.") : t("Shows the new value near the bottom of the screen.")}
        >
          <Switch checked={prefs.osd} onChange={(_, d) => set({ osd: d.checked })} aria-label={t("DPI and profile pop-up")} />
        </SettingCard>
        <LanguageCard prefs={prefs} set={set} />
      </Group>

      <Group title={t("Mouse")}>
        {/* A model hrtz hasn't been tried on, or doesn't know: its owner's check is what makes it supported. */}
        {device && (support === "untested" || support === "unknown") && <ParticipateCard device={device} support={support} />}
        {removed && (
          <div className="tw px-1 py-1 text-xs">
            <UndoNote onUndo={() => undoRemove(removed)}>{t("Removed {{model}}.", { model: removed.model })}</UndoNote>
          </div>
        )}
        {can("longRange") && (
          <SettingCard
            icon={<Wifi120Regular />}
            title={t("Long-range mode")}
            description={
              wired
                ? t("Wireless only. Unplug the cable to change it.")
                : noLongRange
                  ? t("Available when the mouse is awake and wireless.")
                  : t("A stronger signal with less interference. Uses more battery.")
            }
          >
            <Switch
              checked={!!device?.longRange}
              disabled={!device?.online || noLongRange || busy}
              aria-label={t("Long-range mode")}
              onChange={(_, d) => run(() => api.setLongRange(d.checked))}
            />
          </SettingCard>
        )}
        {can("pairing") && (
          <SettingCard
            icon={<PlugConnected20Regular />}
            title={t("Pair a mouse")}
            description={
              wired
                ? t("Wireless only. Unplug the cable first.")
                : !device?.connected
                  ? t("Plug in the receiver first.")
                  : pairing !== null
                    ? t("Hold the left, right and middle buttons together until the light flashes yellow. {{n}} s left.", { n: num(pairing) })
                    : paired === true
                      ? t("Paired. The mouse will connect shortly.")
                      : paired === false
                        ? t("No mouse found in 30 seconds. Turn the mouse on and try again.")
                        : t("For a new receiver or a mouse that won't connect. Searches for 30 seconds.")
            }
          >
            <Button disabled={wired || !device?.connected || busy} onClick={() => void pair()}>
              {pairing !== null ? t("Pairing…") : t("Pair")}
            </Button>
          </SettingCard>
        )}
        <SettingCard icon={<Info20Regular />} title={device?.model ?? t("No mouse connected")} description={facts ?? t("Plug in the receiver to see its details.")} />
        {can("factoryReset") && (
          <SettingCard
            icon={<ArrowRotateClockwise20Regular />}
            title={t("Restore factory settings")}
            description={
              !resetting
                ? family === "hidpp"
                  ? t("Resets every profile's DPI, polling rate and buttons to factory defaults.")
                  : t("Resets DPI, polling, buttons and macros to factory defaults.")
                : progress
                  ? t("Saving a backup first · {{percent}}%", { percent: num(Math.round((progress[0] / progress[1]) * 100)) })
                  : t("Restoring factory settings…")
            }
          >
            <Button disabled={!device?.online || busy} onClick={() => setConfirmReset(true)}>
              {t("Restore")}
            </Button>
          </SettingCard>
        )}
        {/* Only a mouse that isn't plugged in: a connected one would be met again at once. */}
        {device?.key && !device.connected && (
          <SettingCard
            icon={<Delete20Regular />}
            title={t("Remove this mouse")}
            description={t("Takes it off the list with its profile names, app rules and battery history. Backups and the settings on the mouse stay.")}
          >
            <Button disabled={busy} onClick={() => void remove(device.key!)}>
              {t("Remove")}
            </Button>
          </SettingCard>
        )}
      </Group>

      {can("backups") && <BackupsPanel />}

      {SHARING_READY && (
        <Group title={t("Privacy")}>
          <SettingCard
            icon={<DataUsage20Regular />}
            title={t("Help improve hrtz")}
            description={t("Shares anonymous lift and battery data, never your movement or apps. Coming in a later version.")}
          >
            <Switch checked={prefs.shareData} onChange={(_, d) => set({ shareData: d.checked })} aria-label={t("Help improve hrtz")} />
          </SettingCard>
        </Group>
      )}

      <Group title={t("Help")}>
        <SettingCard icon={<Bug20Regular />} title={t("Report a problem")} description={t("Opens a GitHub issue with your hrtz version, mouse and Windows filled in.")}>
          <Button onClick={report}>{t("Report")}</Button>
        </SettingCard>
        <SettingCard icon={<SearchInfo20Regular />} title={t("Inspect a mouse")} description={t("Mouse not supported yet? Save a report and request support.")}>
          <Button onClick={() => setInspecting(true)}>{t("Inspect")}</Button>
        </SettingCard>
      </Group>

      <Caption1 style={{ ...muted, padding: "8px 2px 0" }}>
        {maker ? t("hrtz isn't affiliated with {{maker}} or any other mouse maker.", { maker }) : t("hrtz isn't affiliated with any mouse maker.")}{" "}
        {t("Mouse images and model names belong to their makers and are shown only to identify your mouse.")}
      </Caption1>

      {/* Factory reset is device-wide, so it's the one thing that asks first. */}
      <Dialog open={confirmReset} onOpenChange={(_, d) => setConfirmReset(d.open)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>{t("Restore factory settings?")}</DialogTitle>
            <DialogContent>
              <Body1>
                {family === "hidpp" ? t("Every profile on the mouse returns to its defaults, including its buttons.") : t("Everything on the mouse returns to its defaults, including buttons and macros.")}{" "}
                {t("A backup is saved first, so you can restore this setup from Backups below.")}
              </Body1>
            </DialogContent>
            <DialogActions>
              <Button onClick={() => setConfirmReset(false)}>{t("Cancel")}</Button>
              <Button
                appearance="primary"
                onClick={async () => {
                  setConfirmReset(false);
                  setResetting(true);
                  await run(() => api.factoryReset(), t("Factory settings restored"));
                  setResetting(false);
                }}
              >
                {t("Restore factory settings")}
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </>
  );
}

/** The language hrtz shows its text in: Windows' unless one is picked. Each is named in itself.
 *  Under a translation, a note that it hasn't been reviewed, with a link to the form that fixes a
 *  line (English, the code's own, needs none). */
function LanguageCard({ prefs, set }: { prefs: Prefs; set: (patch: Partial<Prefs>) => void }) {
  const picked = LANGUAGES.find((l) => l.code === prefs.language);
  const windows = LANGUAGES.find((l) => l.code === windowsLanguage())!;
  const shown = LANGUAGES.find((l) => l.code === languageFor(prefs.language))!;
  const same = t("Same as Windows ({{name}})", { name: windows.name });
  const fix = () => void translationUrl(shown.name).then(openUrl).catch((e) => toast(message(e), "error"));
  const note = shown.code !== "en" && (
    <div className="tw pl-9">
      <Caption1 style={{ color: "var(--colorNeutralForeground3)" }}>
        {/* On one line, so the link is the Trans's child 1: "…wrong, <1>open an issue on GitHub</1>." */}
        <Trans>This translation hasn't been reviewed yet. If something reads wrong, <Link inline onClick={fix}>open an issue on GitHub</Link>.</Trans>
      </Caption1>
    </div>
  );
  return (
    <SettingCard stacked={!!note} below={note} icon={<LocalLanguage20Regular />} title={t("Language")} description={t("The language hrtz shows its text in.")}>
      <Dropdown
        aria-label={t("Language")}
        style={{ minWidth: 200 }}
        value={picked?.name ?? same}
        selectedOptions={[picked?.code ?? "windows"]}
        onOptionSelect={(_, d) => d.optionValue && set({ language: d.optionValue === "windows" ? null : d.optionValue })}
      >
        <Option value="windows" text={same}>
          {same}
        </Option>
        {LANGUAGES.map((l) => (
          <Option key={l.code} value={l.code} text={l.name}>
            {l.name}
          </Option>
        ))}
      </Dropdown>
    </SettingCard>
  );
}
