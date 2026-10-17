import { useEffect, useState, type ComponentProps, type ComponentType, type ReactElement, type ReactNode } from "react";
import {
  Badge,
  Body1,
  Body1Strong,
  Button,
  Caption1,
  makeStyles,
  mergeClasses,
  Menu,
  MenuDivider,
  MenuItem,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuTrigger,
  ProgressBar,
  Spinner,
  Subtitle1,
  Title2,
  tokens,
} from "@fluentui/react-components";
import {
  Add16Regular,
  Battery820Regular,
  Beaker20Regular,
  ChevronUpDown16Regular,
  CursorClick20Regular,
  Games20Regular,
  PlugDisconnected16Regular,
  Settings20Regular,
  Stack20Regular,
  TopSpeed20Regular,
  WeatherMoon16Filled,
} from "@fluentui/react-icons";
import type { PageId } from "@/App";
import { BatteryIcon } from "@/BatteryIcon";
import { DeviceProgress, HeaderActionsSlot, NavRow, StatusPill, ToastBridge } from "@/components";
import { BatteryPage } from "@/pages/Battery";
import { ButtonsPage } from "@/pages/Buttons";
import { HudPage } from "@/pages/Hud";
import { LabPage } from "@/pages/Lab";
import { ProfilesPage } from "@/pages/Profiles";
import { SensorPage } from "@/pages/Sensor";
import { SettingsPage } from "@/pages/SettingsPage";
import { SidebarFooter } from "@/SidebarFooter";
import { init, useStore } from "@/store";
import { TitleBar } from "@/TitleBar";
import { real } from "../fake/backend";
import { MouseArt, pickMouse, startFlow, useWorld, type Mouse } from "./mice";
import { Onboarding } from "./Onboarding";

// Throwaway, like the labs: the app's shell (App.tsx) copied, with the device card made a switcher
// (picked 2026-10-17) and the onboarding as a full page over it. The pages are the real ones for
// the Zaopin; another mouse's pages are a placeholder.

const PAGES: { id: PageId; label: string; icon: ReactElement }[] = [
  { id: "sensor", label: "Sensor", icon: <TopSpeed20Regular /> },
  { id: "buttons", label: "Buttons", icon: <CursorClick20Regular /> },
  { id: "profiles", label: "Profiles", icon: <Stack20Regular /> },
  { id: "battery", label: "Battery", icon: <Battery820Regular /> },
  { id: "hud", label: "HUD", icon: <Games20Regular /> },
  { id: "lab", label: "Lab", icon: <Beaker20Regular /> },
  { id: "settings", label: "Settings", icon: <Settings20Regular /> },
];
const REAL: Record<PageId, ComponentType> = {
  sensor: SensorPage,
  buttons: ButtonsPage,
  profiles: ProfilesPage,
  battery: BatteryPage,
  hud: HudPage,
  lab: LabPage,
  settings: SettingsPage,
};
/** Pages about one mouse; the HUD and Settings are the app's. */
const MOUSE_PAGES = new Set<PageId>(["sensor", "buttons", "profiles", "battery", "lab"]);

const useStyles = makeStyles({
  frame: { display: "flex", flexDirection: "column", height: "100%" },
  shell: { display: "grid", gridTemplateColumns: "280px 1fr", flexGrow: 1, minHeight: 0 },
  pane: {
    display: "flex",
    flexDirection: "column",
    gap: tokens.spacingVerticalL,
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalM}`,
    overflowY: "auto",
  },
  card: {
    display: "flex",
    flexDirection: "column",
    paddingBottom: tokens.spacingVerticalM,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
    borderRadius: tokens.borderRadiusXLarge,
    textAlign: "left",
    cursor: "pointer",
    // The rows' faint wash over the card's fill.
    ":hover": { backgroundImage: "linear-gradient(var(--nav-hover), var(--nav-hover))" },
    ":active": { backgroundImage: "linear-gradient(var(--nav-pressed), var(--nav-pressed))" },
  },
  art: { alignSelf: "center", margin: `${tokens.spacingVerticalM} 0 ${tokens.spacingVerticalXS}`, color: tokens.colorNeutralForeground3 },
  device: { display: "flex", flexDirection: "column", gap: "2px", padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM} 0` },
  modelRow: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: tokens.spacingHorizontalS, minWidth: 0 },
  name: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, minWidth: 0 },
  model: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  chevron: { flexShrink: 0, color: tokens.colorNeutralForeground3 },
  rate: {
    flexShrink: 0,
    fontVariantNumeric: "tabular-nums",
    borderTopColor: "transparent",
    borderRightColor: "transparent",
    borderBottomColor: "transparent",
    borderLeftColor: "transparent",
    backgroundColor: tokens.colorSubtleBackgroundHover,
    color: tokens.colorNeutralForeground2,
  },
  linkState: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground3, minWidth: 0 },
  dot: { width: "8px", height: "8px", borderRadius: tokens.borderRadiusCircular, backgroundColor: tokens.colorNeutralForeground4, flexShrink: 0 },
  dotOk: { backgroundColor: tokens.colorPaletteGreenForeground1 },
  dotWarn: { backgroundColor: tokens.colorPaletteMarigoldForeground1 },
  battery: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, marginTop: tokens.spacingVerticalM },
  batteryLine: { display: "flex", alignItems: "center", justifyContent: "space-between" },
  batteryIcon: { fontSize: "20px", color: tokens.colorNeutralForeground2 },
  muted: { color: tokens.colorNeutralForeground3, fontVariantNumeric: "tabular-nums" },
  nav: { display: "flex", flexDirection: "column", gap: "2px" },
  content: { display: "flex", flexDirection: "column", minWidth: 0, minHeight: 0 },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: tokens.spacingHorizontalL,
    flexShrink: 0,
    maxWidth: "1000px",
    padding: `${tokens.spacingVerticalS} 28px ${tokens.spacingVerticalL} ${tokens.spacingHorizontalL}`,
  },
  title: { margin: 0, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  headerSide: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, flexShrink: 0 },
  scroller: { flexGrow: 1, minHeight: 0, overflowY: "auto", overflowX: "hidden", scrollbarGutter: "stable" },
  page: {
    maxWidth: "1000px",
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalL} 96px ${tokens.spacingHorizontalL}`,
    display: "flex",
    flexDirection: "column",
    gap: tokens.spacingVerticalM,
  },
  center: { display: "grid", placeItems: "center", flexGrow: 1, height: "100%", textAlign: "center" },
  emptyText: { display: "flex", flexDirection: "column", alignItems: "center", gap: tokens.spacingVerticalS, maxWidth: "420px" },
  menuItem: { display: "grid", gridTemplateColumns: "44px minmax(0, 1fr)", alignItems: "center", gap: tokens.spacingHorizontalS, padding: "2px 0" },
  rowText: { display: "flex", flexDirection: "column", minWidth: 0 },
  thumb: { display: "flex", alignItems: "center", justifyContent: "center", width: "44px", color: tokens.colorNeutralForeground3 },
});

/** "Connected · 8K receiver", or why it isn't. */
function stateLine(m: Mouse | null): [string, "ok" | "warn" | "off"] {
  if (!m) return ["Plug in a receiver or cable", "off"];
  if (!m.connected) return [`Not connected · ${m.lastSeen}`, "off"];
  if (!m.online) return ["Asleep · move to wake", "warn"];
  return [`Connected · ${m.link}`, "ok"];
}

/** One line under a mouse's name in the list: battery and link, or why there's none. */
function brief(m: Mouse): string {
  if (!m.supported) return "Not supported yet";
  if (!m.connected) return `Not connected · ${m.lastSeen}`;
  return [m.battery ? `${m.battery.level}%` : null, m.link, m.inUse ? "in use" : null].filter(Boolean).join(" · ");
}

const rateBadge = (hz: number) => (hz >= 1000 ? `${hz / 1000}K` : `${hz} Hz`);

/** The pane's card, as App's DeviceCard, and the switcher's button: the picked mouse, or none. */
function MouseCard({ m, ...rest }: { m: Mouse | null } & ComponentProps<"div">) {
  const s = useStyles();
  const lowAt = useStore((st) => st.prefs?.lowBatteryPercent ?? 15);
  const [text, tone] = stateLine(m);
  const b = m?.connected ? m.battery : null;
  const rd = real.device;
  return (
    <div className={s.card} role="button" tabIndex={0} {...rest}>
      <div className={s.art}>
        <MouseArt m={m} width={176} />
      </div>
      <div className={s.device}>
        <div className={s.modelRow}>
          <span className={s.name}>
            <Body1Strong className={s.model}>{m?.model ?? "No mouse connected"}</Body1Strong>
            <ChevronUpDown16Regular className={s.chevron} />
          </span>
          {m?.connected && m.rateHz && (
            <Badge className={s.rate} appearance="tint" shape="rounded" color="informative" title={`Polling at ${m.rateHz.toLocaleString("en-US")} Hz`}>
              {rateBadge(m.rateHz)}
            </Badge>
          )}
        </div>
        <div className={s.linkState}>
          <span className={mergeClasses(s.dot, tone === "ok" && s.dotOk, tone === "warn" && s.dotWarn)} aria-hidden />
          <Caption1 className={s.model}>{text}</Caption1>
        </div>
        {b && (
          <div className={s.battery}>
            <div className={s.batteryLine}>
              <Body1>
                <strong>{b.level}%</strong> battery
              </Body1>
              <BatteryIcon level={b.level} charging={b.charging} className={s.batteryIcon} />
            </div>
            <ProgressBar
              value={b.level / 100}
              color={b.charging ? "brand" : b.level <= lowAt ? "error" : b.level <= lowAt * 2 ? "warning" : "success"}
              aria-label="Battery level"
            />
            {m?.real && rd.battery && (
              <Caption1 className={s.muted}>
                {b.charging ? "Charging · " : ""}
                {(rd.battery.millivolts / 1000).toFixed(2)} V{rd.firmware ? ` · firmware ${rd.firmware}` : ""}
              </Caption1>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** The card, always a switcher: every mouse hrtz knows, then "Connect a new mouse". */
function Switcher() {
  const s = useStyles();
  const { mice, selectedMouse: sel } = useWorld();
  return (
    <Menu
      positioning={{ position: "below", align: "start", offset: 4, matchTargetSize: "width" }}
      checkedValues={{ mouse: sel ? [sel.id] : [] }}
      onCheckedValueChange={(_, d) => d.checkedItems[0] && pickMouse(d.checkedItems[0])}
    >
      <MenuTrigger disableButtonEnhancement>
        <MouseCard m={sel} aria-label="Switch mouse" />
      </MenuTrigger>
      <MenuPopover>
        <MenuList>
          {mice.map((m) => (
            <MenuItemRadio key={m.id} name="mouse" value={m.id} disabled={!m.supported}>
              <span className={s.menuItem}>
                <span className={s.thumb}>
                  <MouseArt m={m} width={44} />
                </span>
                <span className={s.rowText}>
                  <Body1 className={s.model}>{m.model}</Body1>
                  <Caption1 className={mergeClasses(s.muted, s.model)}>{brief(m)}</Caption1>
                </span>
              </span>
            </MenuItemRadio>
          ))}
          {mice.length > 0 && <MenuDivider />}
          <MenuItem icon={<Add16Regular />} onClick={() => startFlow("add")}>
            Connect a new mouse
          </MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}

function Header({ title, children }: { title: string; children?: ReactNode }) {
  const s = useStyles();
  return (
    <header className={s.header}>
      <Title2 as="h1" className={s.title}>
        {title}
      </Title2>
      <div className={s.headerSide}>{children}</div>
    </header>
  );
}

/** A mouse page with no mouse to show: set one up. */
function NoMouse({ label }: { label: string }) {
  const s = useStyles();
  return (
    <>
      <Header title={label} />
      <div className={s.center}>
        <div className={s.emptyText}>
          <Subtitle1 as="h2">No mouse connected</Subtitle1>
          <Body1 className={s.muted}>Plug in its receiver or cable to set it up.</Body1>
          <Button appearance="primary" onClick={() => startFlow("add")}>
            Connect a mouse
          </Button>
        </div>
      </div>
    </>
  );
}

/** Another mouse's page: designed when hrtz adds that mouse. */
function Placeholder({ m, label }: { m: Mouse; label: string }) {
  const s = useStyles();
  return (
    <>
      <Header title={label} />
      <div className={s.center}>
        <div className={s.emptyText}>
          <Subtitle1 as="h2">Comes with its support</Subtitle1>
          <Body1 className={s.muted}>
            The {m.model}'s {label} page is designed when hrtz adds it. This lab is for the shell.
          </Body1>
        </div>
      </div>
    </>
  );
}

export function Shell() {
  const s = useStyles();
  const w = useWorld();
  const sel = w.selectedMouse;
  const [page, setPage] = useState<PageId>("sensor");
  const [ready, setReady] = useState(false);
  const [actionsSlot, setActionsSlot] = useState<HTMLDivElement | null>(null);

  useEffect(() => {
    init().finally(() => setReady(true));
  }, []);
  // A mouse hrtz hasn't met, plugged in while the app is open, gets the onboarding's second half.
  useEffect(() => {
    if (!w.flow && w.found.length) startFlow("add", "found");
  }, [w.found.length, !!w.flow]);
  // A page the picked mouse doesn't have goes back to the first.
  useEffect(() => {
    if (sel?.hides.includes(page)) setPage("sensor");
  }, [sel?.id]);

  if (w.flow) return <Onboarding />;

  const nav = PAGES.filter((p) => !sel?.hides.includes(p.id));
  const label = nav.find((p) => p.id === page)?.label ?? "";

  let body: ReactNode;
  if (!ready) {
    body = (
      <div className={s.center}>
        <Spinner label="Looking for your mouse" />
      </div>
    );
  } else if (MOUSE_PAGES.has(page) && !sel) {
    body = <NoMouse label={label} />;
  } else if (MOUSE_PAGES.has(page) && sel && !sel.real) {
    body = <Placeholder m={sel} label={label} />;
  } else {
    const Page = REAL[page];
    const mousePage = MOUSE_PAGES.has(page);
    body = (
      <>
        <header className={s.header}>
          <Title2 as="h1" className={s.title}>
            {label}
          </Title2>
          <div className={s.headerSide}>
            <div role="status">
              {mousePage && sel && !sel.connected && (
                <StatusPill tone="asleep" icon={<PlugDisconnected16Regular />} title="Not connected" detail="Plug it in to make changes" hint="Showing its last settings." />
              )}
              {mousePage && sel?.connected && !sel.online && (
                <StatusPill tone="asleep" icon={<WeatherMoon16Filled />} breathing title="Mouse is asleep" detail="Move it to make changes" />
              )}
            </div>
            <div ref={setActionsSlot} className={s.headerSide} />
          </div>
        </header>
        <div className={s.scroller}>
          <div className={s.page}>
            <DeviceProgress />
            <Page />
          </div>
        </div>
      </>
    );
  }

  return (
    <HeaderActionsSlot.Provider value={actionsSlot}>
      <div className={s.frame}>
        <TitleBar />
        <div className={s.shell}>
          <nav className={s.pane} aria-label="Sections">
            <Switcher />
            <div className={s.nav}>
              {nav.map((p) => (
                <NavRow key={p.id} icon={p.icon} label={p.label} selected={page === p.id} onClick={() => setPage(p.id)} />
              ))}
            </div>
            <SidebarFooter />
          </nav>
          <main className={s.content}>{body}</main>
          <ToastBridge />
        </div>
      </div>
    </HeaderActionsSlot.Provider>
  );
}
