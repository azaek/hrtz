import { useEffect, useState, type ReactNode } from "react";
import {
  Body1,
  Body1Strong,
  Button,
  Caption1,
  Caption1Strong,
  Link,
  makeStyles,
  mergeClasses,
  Spinner,
  Switch,
  Title1,
  tokens,
} from "@fluentui/react-components";
import { Checkmark16Regular, PlugConnected20Regular, Warning16Filled } from "@fluentui/react-icons";
import { Emblem } from "@/brand";
import { Notice, SettingCard } from "@/components";
import { ProgressLine } from "@/kit";
import { MODEL_ART } from "@/models";
import { useStore } from "@/store";
import { TitleBar } from "@/TitleBar";
import { cancelFlow, DrawnMouse, finishFlow, goStep, MouseArt, useWorld, type Mouse, type Step } from "./mice";

// Throwaway, like the labs: the onboarding as a full page. First run goes Welcome → Connect →
// Your mouse → Preferences; "Connect a new mouse" goes Connect → Your mouse. Centred, like Windows
// Setup (picked 2026-10-17 over a split one with the steps on the left).

/** What hrtz runs, by brand, for "See which mice work". */
const WORKS_WITH: { brand: string; models: { model: string; mid?: number; tested: boolean }[] }[] = [
  {
    brand: "Zaopin",
    models: [
      { model: "Z1 PRO MAX", mid: 4, tested: true },
      { model: "Z1 PRO", mid: 1, tested: false },
      { model: "Z2", mid: 2, tested: false },
      { model: "Z2 MAX", mid: 5, tested: false },
      { model: "Z2 mini", mid: 3, tested: false },
      { model: "Z2 mini MAX", mid: 6, tested: false },
    ],
  },
  { brand: "Logitech", models: [{ model: "PRO X SUPERLIGHT", tested: true }] },
];

const useStyles = makeStyles({
  frame: { display: "flex", flexDirection: "column", height: "100%" },

  // Centred: one column in the middle, the buttons along the bottom.
  centered: { display: "grid", gridTemplateRows: "minmax(0, 1fr) auto", flexGrow: 1, minHeight: 0 },
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
  // Back on the left, the step dots in the middle, Continue on the right.
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


  // Shared.
  title: { margin: 0 },
  muted: { color: tokens.colorNeutralForeground3 },
  left: { textAlign: "left", alignSelf: "stretch" },
  status: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground3 },
  done: { color: tokens.colorPaletteGreenForeground1, flexShrink: 0 },
  backup: { display: "flex", flexDirection: "column", alignItems: "center", gap: tokens.spacingVerticalXS, alignSelf: "stretch" },
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

function WorksWith() {
  const s = useStyles();
  return (
    <div className={s.works}>
      {WORKS_WITH.map((g) => (
        <div key={g.brand}>
          <Caption1Strong>{g.brand}</Caption1Strong>
          <div className={s.workGrid}>
            {g.models.map((x) => (
              <div key={x.model} className={s.workRow}>
                {x.mid ? <img src={MODEL_ART[x.mid].image} alt="" width={40} style={{ display: "block", height: "auto" }} /> : <DrawnMouse width={40} />}
                <span className={s.workText}>
                  <Body1>{x.model}</Body1>
                  <Caption1 className={s.muted}>{x.tested ? "Tested" : "Not tested yet"}</Caption1>
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
const details = (m: Mouse) => [m.link, m.battery ? `${m.battery.level}% battery` : null, m.firmware ? `firmware ${m.firmware}` : null].filter(Boolean).join(" · ");

type Shown = { hero: ReactNode; title: string; body: ReactNode; primary: { label: string; onClick: () => void; disabled?: boolean } | null; back: ReactNode };

export function Onboarding() {
  const s = useStyles();
  const w = useWorld();
  const flow = w.flow!;
  const first = flow.mode === "first";
  const steps: Step[] = first ? ["welcome", "connect", "found", "prefs"] : ["connect", "found"];
  const lowAt = useStore((st) => st.prefs?.lowBatteryPercent ?? 15);
  const [startup, setStartup] = useState(true);
  const [alerts, setAlerts] = useState(true);
  const [showWorks, setShowWorks] = useState(false);
  const [backup, setBackup] = useState(0);
  const blocker = w.vendorApp ? w.found.find((m) => m.vendorApp) : undefined;
  const foundKey = w.found.map((m) => m.id).join();

  // A new mouse moves the wait on; losing it goes back to waiting.
  useEffect(() => {
    if (flow.step === "connect" && w.found.length) {
      const t = setTimeout(() => goStep("found"), 600);
      return () => clearTimeout(t);
    }
    if (flow.step === "found" && !w.found.length) goStep("connect");
  }, [flow.step, foundKey]);
  // The first backup, as hrtz takes it when it meets a model: it waits while the maker's app runs.
  useEffect(() => {
    setBackup(0);
  }, [foundKey]);
  useEffect(() => {
    if (flow.step !== "found" || blocker || backup >= 1) return;
    const t = setTimeout(() => setBackup((b) => Math.min(1, b + 0.08)), 120);
    return () => clearTimeout(t);
  }, [flow.step, blocker, backup]);

  const backButton = (to: Step) => <Button onClick={() => goStep(to)}>Back</Button>;
  const cancel = <Button onClick={cancelFlow}>Cancel</Button>;

  let shown: Shown;
  switch (flow.step) {
    case "welcome":
      shown = {
        hero: <Emblem height={72} />,
        title: "Welcome to hrtz",
        body: (
          <Body1 className={s.muted}>
            A free, open-source app for gaming mice. Settings are stored on the mouse, so they work without the app.
          </Body1>
        ),
        primary: { label: "Get started", onClick: () => goStep(w.found.length ? "found" : "connect") },
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
            {w.unsupported.map((m) => (
              <div key={m.id} className={s.left}>
                <SettingCard icon={<PlugConnected20Regular />} title={`${m.model} isn't supported yet`} description="Support is added one mouse at a time, with help from its owners.">
                  <Button>Ask for it</Button>
                </SettingCard>
              </div>
            ))}
            <Link as="button" onClick={() => setShowWorks((x) => !x)}>
              {showWorks ? "Hide the list" : "See which mice work"}
            </Link>
            {showWorks && <WorksWith />}
          </>
        ),
        primary: { label: "Continue", onClick: () => {}, disabled: true },
        back: first ? (
          <>
            {backButton("welcome")}
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
      const one = w.found.length === 1 ? w.found[0] : null;
      const untested = w.found.filter((m) => !m.tested);
      shown = {
        hero: (
          <div className={s.heroRow}>
            {w.found.map((m) => (
              <div key={m.id} className={s.heroItem}>
                <MouseArt m={m} width={one ? 240 : 170} />
                {!one && <Body1Strong>{m.model}</Body1Strong>}
              </div>
            ))}
          </div>
        ),
        title: one ? `Found your ${one.model}` : `Found ${w.found.length} mice`,
        body: (
          <>
            {one ? (
              <Body1 className={s.muted}>{details(one)}</Body1>
            ) : (
              w.found.map((m) => (
                <Body1 key={m.id} className={s.muted}>
                  {m.model}: {details(m)}
                </Body1>
              ))
            )}
            {untested.length > 0 && (
              <Caption1 className={s.muted}>
                {untested.map((m) => m.model).join(" and ")} {untested.length > 1 ? "haven't" : "hasn't"} been tested yet. If something doesn't work, send a
                report from Settings.
              </Caption1>
            )}
            {blocker ? (
              <div className={s.left}>
                <Notice tone="warning" icon={<Warning16Filled />} title={`Quit ${blocker.vendorApp}`}>
                  It uses the same channel as hrtz and overwrites its changes. Quit it from its tray icon to continue.
                </Notice>
              </div>
            ) : (
              <div className={s.backup}>
                {backup < 1 ? (
                  <>
                    <ProgressLine value={backup} label="Backing up" />
                    <Caption1 className={s.muted}>Backing up its current settings</Caption1>
                  </>
                ) : (
                  <div className={s.status}>
                    <Checkmark16Regular className={s.done} />
                    <Caption1>Current settings backed up. Restore them anytime from Settings.</Caption1>
                  </div>
                )}
              </div>
            )}
          </>
        ),
        primary: first
          ? { label: "Continue", onClick: () => goStep("prefs"), disabled: !!blocker || backup < 1 }
          : { label: "Done", onClick: finishFlow, disabled: !!blocker || backup < 1 },
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
                  <Caption1 className={s.muted}>A Windows notification at {lowAt}% battery.</Caption1>
                </span>
                <Switch checked={alerts} onChange={(_, d) => setAlerts(d.checked)} aria-label="Low battery alert" />
              </label>
            </div>
            <Caption1 className={s.muted}>Closing the window keeps hrtz in the tray. Quit from the tray icon.</Caption1>
          </>
        ),
        primary: { label: "Finish", onClick: finishFlow },
        back: backButton(w.found.length ? "found" : "connect"),
      };
      break;
  }

  const now = steps.indexOf(flow.step);
  const primary = shown.primary && (
    <Button appearance="primary" disabled={shown.primary.disabled} onClick={shown.primary.onClick}>
      {shown.primary.label}
    </Button>
  );

  return (
    <div className={s.frame}>
      <TitleBar />
      <div className={s.centered}>
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
          <div className={mergeClasses(s.footerSide, s.footerEnd)}>{primary}</div>
        </div>
      </div>
    </div>
  );
}
