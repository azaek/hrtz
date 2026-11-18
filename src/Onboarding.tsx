import { useEffect, useState, type ReactNode } from "react";
import { Body1, Body1Strong, Button, Caption1, Caption1Strong, Link, makeStyles, mergeClasses, Spinner, Switch, Title1, tokens } from "@fluentui/react-components";
import { Checkmark16Regular, Warning16Filled } from "@fluentui/react-icons";
import { enable, disable } from "@tauri-apps/plugin-autostart";
import { api, message, on, type BackupMeta, type DeviceState } from "./api";
import { Emblem } from "./brand";
import { Notice } from "./components";
import { MousePicture, MousePlaceholder } from "./MousePicture";
import { num, t } from "./i18n";
import { linkLabel } from "./links";
import { artOf, familyOf, isZaopin, ofModel } from "./models";
import { endFlow, goStep, pickMouse, savePrefs, setInspecting, toast, useStore, type Step } from "./store";
import { TitleBar } from "./TitleBar";

const useStyles = makeStyles({
  frame: { display: "flex", flexDirection: "column", height: "100%" },
  page: { display: "grid", gridTemplateRows: "minmax(0, 1fr) auto", flexGrow: 1, minHeight: 0 },
  // Auto margins centre the column when it fits and let it scroll from the top when it doesn't.
  scroll: { display: "flex", flexDirection: "column", overflowY: "auto", overflowX: "hidden", scrollbarGutter: "stable both-edges" },
  column: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    textAlign: "center",
    gap: tokens.spacingVerticalM,
    width: "100%",
    maxWidth: "560px",
    boxSizing: "border-box",
    margin: "auto",
    // More below than above: true centre reads low.
    padding: `${tokens.spacingVerticalXXL} ${tokens.spacingHorizontalL} 72px`,
    // Fluent's text sets its own alignment.
    "& > .fui-Body1, & > .fui-Caption1": { textAlign: "center" },
  },
  // Back on the left, the step dots in the middle, the next step on the right.
  footer: {
    display: "grid",
    gridTemplateColumns: "1fr auto 1fr",
    alignItems: "center",
    gap: tokens.spacingHorizontalM,
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalXXL}`,
  },
  footerSide: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalM },
  footerEnd: { justifyContent: "flex-end" },
  dots: { display: "flex", gap: "6px" },
  dot: { width: "6px", height: "6px", borderRadius: tokens.borderRadiusCircular, backgroundColor: tokens.colorNeutralStroke1, transitionProperty: "width, background-color", transitionDuration: "220ms" },
  dotOn: { width: "18px", backgroundColor: tokens.colorNeutralForeground2 },
  title: { margin: 0 },
  muted: { color: tokens.colorNeutralForeground3 },
  stretch: { alignSelf: "stretch", textAlign: "left" },
  status: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground3 },
  done: { color: tokens.colorPaletteGreenForeground1, flexShrink: 0 },
  heroRow: { display: "flex", alignItems: "flex-end", justifyContent: "center", gap: tokens.spacingHorizontalXL },
  heroItem: { display: "flex", flexDirection: "column", alignItems: "center", gap: tokens.spacingVerticalXS },
  searching: { position: "relative", display: "grid", placeItems: "center", width: "220px", height: "190px", color: tokens.colorNeutralForeground3 },
  ring: {
    position: "absolute",
    inset: "18px 36px",
    borderRadius: tokens.borderRadiusCircular,
    boxShadow: `0 0 0 1px ${tokens.colorNeutralStroke1}`,
    animationName: { from: { transform: "scale(0.7)", opacity: 0.9 }, to: { transform: "scale(1.35)", opacity: 0 } },
    animationDuration: "2.4s",
    animationIterationCount: "infinite",
    animationTimingFunction: "ease-out",
  },
  ring2: { animationDelay: "1.2s" },
  options: { display: "flex", flexDirection: "column", gap: "2px", alignSelf: "stretch", textAlign: "left" },
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
  optionText: { display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 },
  works: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM, alignSelf: "stretch", textAlign: "left" },
  workGrid: { display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: "2px" },
  workRow: { display: "grid", gridTemplateColumns: "40px minmax(0, 1fr)", alignItems: "center", gap: tokens.spacingHorizontalS, padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}` },
  workText: { display: "flex", flexDirection: "column", minWidth: 0 },
});

/** An empty mouse with rings going out: waiting for one to turn up. */
function Searching() {
  const s = useStyles();
  return (
    <div className={s.searching} aria-hidden>
      <span className={s.ring} />
      <span className={mergeClasses(s.ring, s.ring2)} />
      <MousePlaceholder width={180} />
    </div>
  );
}

/** A brand with more models than this shows them as one line. */
const MANY = 6;

/** What hrtz runs, by brand, and whether each has been tried on the real mouse. A brand with many
 *  models (ATK's) is one line: how many, and how many are tested. */
function WorksWith() {
  const s = useStyles();
  const supported = useStore((x) => x.supported);
  const brands = [...new Set(supported.map((m) => m.brand))];
  return (
    <div className={s.works}>
      {brands.map((brand) => {
        // One per name: ATK gives some one name for several model IDs, and a Z2 answers 2 or 11.
        // It's tested where a check on one of them has passed, as the README lists it.
        const all = supported.filter((m) => m.brand === brand);
        const models = all
          .filter((m, i) => all.findIndex((x) => x.model === m.model) === i)
          .map((m) => ({ ...m, tested: all.some((x) => x.model === m.model && x.tested) }));
        const tested = models.filter((m) => m.tested).length;
        return (
          <div key={brand}>
            <Caption1Strong>{brand}</Caption1Strong>
            <div className={s.workGrid}>
              {models.length > MANY ? (
                <div className={s.workRow}>
                  <MousePicture width={40} />
                  <span className={s.workText}>
                    <Body1>{t("{{count}} models", { count: models.length, defaultValue_one: "{{count}} model" })}</Body1>
                    <Caption1 className={s.muted}>{tested ? t("{{count}} tested", { count: tested }) : t("Not tested yet")}</Caption1>
                  </span>
                </div>
              ) : (
                models.map((m) => (
                  <div key={m.key} className={s.workRow}>
                    <MousePicture image={artOf(m.key)?.image} width={40} />
                    <span className={s.workText}>
                      <Body1>{m.model}</Body1>
                      <Caption1 className={s.muted}>{m.tested ? t("Tested") : t("Not tested yet")}</Caption1>
                    </span>
                  </div>
                ))
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

/** "8K receiver · 27% battery · firmware 2.17". */
const details = (d: DeviceState) =>
  [linkLabel(d), d.battery ? t("{{percent}}% battery", { percent: num(d.battery.level) }) : null, d.firmware ? t("firmware {{version}}", { version: d.firmware }) : null].filter(Boolean).join(" · ");

type Shown = { hero: ReactNode; title: string; body: ReactNode; primary: { label: string; onClick: () => void; disabled?: boolean } | null; back: ReactNode };

/**
 * The onboarding, as a full page in place of the app. First run goes Welcome, Connect, Your mouse,
 * Preferences; "Connect a new mouse" goes Connect, Your mouse. Centred, like Windows Setup.
 * Decided in the shell lab (DECISIONS.md, "Several mice").
 */
export function Onboarding() {
  const s = useStyles();
  const flow = useStore((x) => x.flow)!;
  const prefs = useStore((x) => x.prefs);
  const devices = useStore((x) => x.devices);
  const supported = useStore((x) => x.supported);
  const vendorApps = useStore((x) => x.status.vendorApps);
  const first = flow.mode === "first";
  const steps: Step[] = first ? ["welcome", "connect", "found", "prefs"] : ["connect", "found"];
  const [startup, setStartup] = useState(true);
  const [alerts, setAlerts] = useState(prefs?.batteryAlerts ?? true);
  const [showWorks, setShowWorks] = useState(false);
  const [backups, setBackups] = useState<BackupMeta[] | null>(null);

  // What it's waiting for: a mouse connected that wasn't known when it opened (on first run, any).
  const connected = Object.values(devices).filter((d) => d.connected && d.key);
  const found = first ? connected : connected.filter((d) => !flow.known.includes(d.key!));
  const foundKey = found.map((d) => d.key).join();

  useEffect(() => {
    if (flow.step === "connect" && found.length) {
      const t = setTimeout(() => goStep("found"), 600);
      return () => clearTimeout(t);
    }
    if (flow.step === "found" && !found.length) goStep("connect");
  }, [flow.step, foundKey]);
  // hrtz saves the first backup of a model by itself; this shows when it's there.
  useEffect(() => {
    const read = () => api.listBackups().then(setBackups, () => setBackups([]));
    read();
    const off = on.backups(read);
    return () => void off.then((f) => f());
  }, []);
  const finish = async () => {
    if (prefs) {
      // Start with Windows is on unless turned off here (DECISIONS.md), even on a PC that has no
      // entry yet. A dev build doesn't have it: its entry would point at the dev build.
      try {
        if (!import.meta.env.DEV) await (startup ? enable() : disable());
      } catch (e) {
        toast(message(e), "error");
      }
      await savePrefs({ ...prefs, welcomed: true, batteryAlerts: alerts });
    }
    if (found[0]?.key) pickMouse(found[0].key);
    endFlow();
  };
  const done = () => {
    if (found[0]?.key) pickMouse(found[0].key);
    endFlow();
  };
  const back = (to: Step) => <Button onClick={() => goStep(to)}>{t("Back")}</Button>;
  const cancel = <Button onClick={endFlow}>{t("Cancel")}</Button>;
  const askForIt = () => setInspecting(true);

  let shown: Shown;
  switch (flow.step) {
    case "welcome":
      shown = {
        hero: <Emblem height={72} />,
        title: t("Welcome to hrtz"),
        body: <Body1 className={s.muted}>{t("A free, open-source app for gaming mice. Settings are stored on the mouse, so they work without the app.")}</Body1>,
        primary: { label: t("Get started"), onClick: () => goStep(found.length ? "found" : "connect") },
        back: null,
      };
      break;
    case "connect":
      shown = {
        hero: <Searching />,
        title: first ? t("Connect your mouse") : t("Connect a new mouse"),
        body: (
          <>
            <Body1 className={s.muted}>{t("Plug in its receiver or cable, and turn it on. hrtz finds it automatically.")}</Body1>
            <div className={s.status}>
              <Spinner size="extra-tiny" />
              <Caption1>{t("Looking for a mouse")}</Caption1>
            </div>
            <Link as="button" onClick={() => setShowWorks((x) => !x)}>
              {showWorks ? t("Hide the list") : t("See which mice work")}
            </Link>
            {showWorks && (
              <>
                <WorksWith />
                <Caption1 className={s.muted}>
                  {t("Don't see yours?")}{" "}
                  <Link as="button" onClick={askForIt}>
                    {t("Request support")}
                  </Link>
                </Caption1>
              </>
            )}
          </>
        ),
        primary: { label: t("Continue"), onClick: () => {}, disabled: true },
        back: first ? (
          <>
            {back("welcome")}
            <Link as="button" onClick={() => goStep("prefs")}>
              {t("Skip for now")}
            </Link>
          </>
        ) : (
          cancel
        ),
      };
      break;
    case "found": {
      const one = found.length === 1 ? found[0] : null;
      const untested = found.filter((d) => supported.some((m) => ofModel(d.key, m.key) && !m.tested));
      // Only a mouse whose memory can be backed up waits for its first backup.
      const backedUp = (d: DeviceState) => !d.capabilities?.backups || !!backups?.some((b) => (b.key ? b.key === d.key : b.model === d.model));
      const waiting = found.some((d) => !backedUp(d));
      const backing = found.some((d) => d.capabilities?.backups);
      // A maker's app in the way of a mouse found: Zaopin's app fights hrtz for a Zaopin; G HUB edits
      // a Logitech's profiles too, or drives it itself (host mode).
      const makerApp = (family: string) => vendorApps.includes(family) && found.some((d) => familyOf(d.key) === family);
      const zaopinApp = vendorApps.includes("compx") && found.some(isZaopin);
      const ghub = makerApp("hidpp");
      const ghubDrives = ghub && found.some((d) => familyOf(d.key) === "hidpp" && d.hostMode);
      shown = {
        hero: (
          <div className={s.heroRow}>
            {found.map((d) => (
              <div key={d.key} className={s.heroItem}>
                <MousePicture image={artOf(d.key, d.variant)?.image} width={one ? 240 : 170} />
                {!one && <Body1Strong>{d.model}</Body1Strong>}
              </div>
            ))}
          </div>
        ),
        title: one ? t("Found your {{model}}", { model: one.model }) : t("Found {{count}} mice", { count: found.length, defaultValue_one: "Found {{count}} mouse" }),
        body: (
          <>
            {found.map((d) => (
              <Body1 key={d.key} className={s.muted}>
                {one ? "" : `${d.model}: `}
                {details(d)}
              </Body1>
            ))}
            {untested.length > 0 && (
              <Caption1 className={s.muted}>
                {t("{{models}} haven't been tested yet. You can help: its Not tested badge opens a 2-minute check that sends what works and what doesn't.", {
                  count: untested.length,
                  models: untested.map((d) => d.model).join(` ${t("and")} `),
                  defaultValue_one: "{{models}} hasn't been tested yet. You can help: its Not tested badge opens a 2-minute check that sends what works and what doesn't.",
                })}
              </Caption1>
            )}
            {zaopinApp ? (
              <div className={s.stretch}>
                <Notice tone="warning" icon={<Warning16Filled />} title={t("Quit Zaopin's app")}>
                  {t("It uses the same channel as hrtz and overwrites its changes. Quit it from its tray icon.")}
                </Notice>
              </div>
            ) : ghub ? (
              <div className={s.stretch}>
                <Notice tone="warning" icon={<Warning16Filled />} title={t("Quit G HUB")}>
                  {ghubDrives
                    ? t("It's controlling the mouse, so changes made in hrtz wait until it lets go. Quit it from its tray icon.")
                    : t("It changes the same profiles on the mouse as hrtz, and the last change wins. Quit it from its tray icon.")}
                </Notice>
              </div>
            ) : waiting ? (
              <div className={s.status}>
                <Spinner size="extra-tiny" />
                <Caption1>{t("Backing up its current settings")}</Caption1>
              </div>
            ) : backing ? (
              <div className={s.status}>
                <Checkmark16Regular className={s.done} />
                <Caption1>{t("Current settings backed up. Restore them anytime from Settings.")}</Caption1>
              </div>
            ) : null}
          </>
        ),
        primary: first ? { label: t("Continue"), onClick: () => goStep("prefs") } : { label: t("Done"), onClick: done },
        back: first ? null : cancel,
      };
      break;
    }
    case "prefs":
      shown = {
        hero: null,
        title: t("A few preferences"),
        body: (
          <>
            <Body1 className={s.muted}>{t("You can change these later in Settings.")}</Body1>
            <div className={s.options}>
              <label className={s.option}>
                <span className={s.optionText}>
                  <Body1>{t("Start with Windows")}</Body1>
                  <Caption1 className={s.muted}>{t("Runs in the tray for battery history, per-app profiles and mousepad tracking.")}</Caption1>
                </span>
                <Switch checked={startup} onChange={(_, d) => setStartup(d.checked)} aria-label={t("Start with Windows")} />
              </label>
              <label className={s.option}>
                <span className={s.optionText}>
                  <Body1>{t("Low battery alert")}</Body1>
                  <Caption1 className={s.muted}>{t("A Windows notification at {{percent}}% battery.", { percent: num(prefs?.lowBatteryPercent ?? 15) })}</Caption1>
                </span>
                <Switch checked={alerts} onChange={(_, d) => setAlerts(d.checked)} aria-label={t("Low battery alert")} />
              </label>
            </div>
            <Caption1 className={s.muted}>{t("Closing the window keeps hrtz in the tray. Quit from the tray icon.")}</Caption1>
          </>
        ),
        primary: { label: t("Finish"), onClick: () => void finish() },
        back: back(found.length ? "found" : "connect"),
      };
      break;
  }

  const now = steps.indexOf(flow.step);
  return (
    <div className={s.frame}>
      <TitleBar />
      <div className={s.page}>
        <div className={s.scroll}>
          <div className={s.column}>
            {shown.hero}
            <Title1 as="h1" className={s.title}>
              {shown.title}
            </Title1>
            {shown.body}
          </div>
        </div>
        <div className={s.footer}>
          <div className={s.footerSide}>{shown.back}</div>
          {first ? (
            <div className={s.dots} role="img" aria-label={t("Step {{step}} of {{total}}", { step: now + 1, total: steps.length })}>
              {steps.map((x, i) => (
                <span key={x} className={mergeClasses(s.dot, i === now && s.dotOn)} />
              ))}
            </div>
          ) : (
            <span />
          )}
          <div className={mergeClasses(s.footerSide, s.footerEnd)}>
            {shown.primary && (
              <Button appearance="primary" disabled={shown.primary.disabled} onClick={shown.primary.onClick}>
                {shown.primary.label}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
