import { useEffect, useState, type ReactNode } from "react";
import { Body1, Body1Strong, Button, Caption1, Caption1Strong, Link, makeStyles, mergeClasses, Spinner, Switch, Title1, tokens } from "@fluentui/react-components";
import { Checkmark16Regular, Warning16Filled } from "@fluentui/react-icons";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, message, on, type BackupMeta, type DeviceState } from "./api";
import { Emblem } from "./brand";
import { Notice } from "./components";
import { DrawnMouse, MousePicture } from "./DrawnMouse";
import { linkName } from "./links";
import { artOf, familyOf, ofModel } from "./models";
import { endFlow, goStep, pickMouse, savePrefs, toast, useStore, type Step } from "./store";
import { TitleBar } from "./TitleBar";
import { REPO_URL } from "./update";

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
      <DrawnMouse width={180} outline />
    </div>
  );
}

/** What hrtz runs, by brand, and whether each has been tried on the real mouse. */
function WorksWith() {
  const s = useStyles();
  const supported = useStore((x) => x.supported);
  const brands = [...new Set(supported.map((m) => m.brand))];
  return (
    <div className={s.works}>
      {brands.map((brand) => (
        <div key={brand}>
          <Caption1Strong>{brand}</Caption1Strong>
          <div className={s.workGrid}>
            {supported
              .filter((m) => m.brand === brand)
              .map((m) => (
                <div key={m.key} className={s.workRow}>
                  <MousePicture image={artOf(m.key)?.image} width={40} />
                  <span className={s.workText}>
                    <Body1>{m.model}</Body1>
                    <Caption1 className={s.muted}>{m.tested ? "Tested" : "Not tested yet"}</Caption1>
                  </span>
                </div>
              ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/** "8K receiver · 27% battery · firmware 2.17". */
const details = (d: DeviceState) =>
  [linkName(d), d.battery ? `${d.battery.level}% battery` : null, d.firmware ? `firmware ${d.firmware}` : null].filter(Boolean).join(" · ");

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
  const vendorApp = useStore((x) => x.status.vendorAppRunning);
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
  useEffect(() => {
    isEnabled().then(setStartup, () => {});
  }, []);

  const finish = async () => {
    if (prefs) {
      try {
        await (startup ? enable() : disable());
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
  const back = (to: Step) => <Button onClick={() => goStep(to)}>Back</Button>;
  const cancel = <Button onClick={endFlow}>Cancel</Button>;
  const askForIt = () => void openUrl(`${REPO_URL}/discussions`).catch((e) => toast(message(e), "error"));

  let shown: Shown;
  switch (flow.step) {
    case "welcome":
      shown = {
        hero: <Emblem height={72} />,
        title: "Welcome to hrtz",
        body: <Body1 className={s.muted}>A free, open-source app for gaming mice. Settings are stored on the mouse, so they work without the app.</Body1>,
        primary: { label: "Get started", onClick: () => goStep(found.length ? "found" : "connect") },
        back: null,
      };
      break;
    case "connect":
      shown = {
        hero: <Searching />,
        title: first ? "Connect your mouse" : "Connect a new mouse",
        body: (
          <>
            <Body1 className={s.muted}>Plug in its receiver or cable, and turn it on. hrtz finds it automatically.</Body1>
            <div className={s.status}>
              <Spinner size="extra-tiny" />
              <Caption1>Looking for a mouse</Caption1>
            </div>
            <Link as="button" onClick={() => setShowWorks((x) => !x)}>
              {showWorks ? "Hide the list" : "See which mice work"}
            </Link>
            {showWorks && (
              <>
                <WorksWith />
                <Caption1 className={s.muted}>
                  Don't see yours?{" "}
                  <Link as="button" onClick={askForIt}>
                    Ask for it
                  </Link>
                </Caption1>
              </>
            )}
          </>
        ),
        primary: { label: "Continue", onClick: () => {}, disabled: true },
        back: first ? (
          <>
            {back("welcome")}
            <Link as="button" onClick={() => goStep("prefs")}>
              Skip for now
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
      const backedUp = (d: DeviceState) => !d.capabilities?.backups || !!backups?.some((b) => b.model === d.model);
      const waiting = found.some((d) => !backedUp(d));
      const backing = found.some((d) => d.capabilities?.backups);
      // Zaopin's app is the one that fights hrtz for a Zaopin.
      const zaopinApp = vendorApp && found.some((d) => familyOf(d.key) === "compx");
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
        title: one ? `Found your ${one.model}` : `Found ${found.length} mice`,
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
                {untested.map((d) => d.model).join(" and ")} {untested.length > 1 ? "haven't" : "hasn't"} been tested yet. If something doesn't work, send a
                report from Settings.
              </Caption1>
            )}
            {zaopinApp ? (
              <div className={s.stretch}>
                <Notice tone="warning" icon={<Warning16Filled />} title="Quit Zaopin's app">
                  It uses the same channel as hrtz and overwrites its changes. Quit it from its tray icon.
                </Notice>
              </div>
            ) : waiting ? (
              <div className={s.status}>
                <Spinner size="extra-tiny" />
                <Caption1>Backing up its current settings</Caption1>
              </div>
            ) : backing ? (
              <div className={s.status}>
                <Checkmark16Regular className={s.done} />
                <Caption1>Current settings backed up. Restore them anytime from Settings.</Caption1>
              </div>
            ) : null}
          </>
        ),
        primary: first ? { label: "Continue", onClick: () => goStep("prefs") } : { label: "Done", onClick: done },
        back: first ? null : cancel,
      };
      break;
    }
    case "prefs":
      shown = {
        hero: null,
        title: "A few preferences",
        body: (
          <>
            <Body1 className={s.muted}>You can change these later in Settings.</Body1>
            <div className={s.options}>
              <label className={s.option}>
                <span className={s.optionText}>
                  <Body1>Start with Windows</Body1>
                  <Caption1 className={s.muted}>Runs in the tray for battery history, per-app profiles and mousepad tracking.</Caption1>
                </span>
                <Switch checked={startup} onChange={(_, d) => setStartup(d.checked)} aria-label="Start with Windows" />
              </label>
              <label className={s.option}>
                <span className={s.optionText}>
                  <Body1>Low battery alert</Body1>
                  <Caption1 className={s.muted}>A Windows notification at {prefs?.lowBatteryPercent ?? 15}% battery.</Caption1>
                </span>
                <Switch checked={alerts} onChange={(_, d) => setAlerts(d.checked)} aria-label="Low battery alert" />
              </label>
            </div>
            <Caption1 className={s.muted}>Closing the window keeps hrtz in the tray. Quit from the tray icon.</Caption1>
          </>
        ),
        primary: { label: "Finish", onClick: () => void finish() },
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
            <div className={s.dots} role="img" aria-label={`Step ${now + 1} of ${steps.length}`}>
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
