import { useEffect, useState, type ComponentType, type ReactElement, type ReactNode } from "react";
import {
  Badge,
  Body1,
  Body1Strong,
  Caption1,
  makeStyles,
  mergeClasses,
  ProgressBar,
  Spinner,
  Subtitle1,
  Title2,
  tokens,
} from "@fluentui/react-components";
import {
  CursorClick20Regular,
  Beaker20Regular,
  Games20Regular,
  Settings20Regular,
  Battery820Regular,
  BatterySaverRegular,
  Stack20Regular,
  TopSpeed20Regular,
  Warning16Filled,
  WeatherMoon16Filled,
} from "@fluentui/react-icons";
import { init, useMouse, useStore } from "./store";
import { HeaderActionsSlot, NavRow, Notice, StatusPill, ToastBridge } from "./components";
import { SensorPage } from "./pages/Sensor";
import { ButtonsPage } from "./pages/Buttons";
import { ProfilesPage } from "./pages/Profiles";
import { LabPage } from "./pages/Lab";
import { BatteryPage } from "./pages/Battery";
import { HudPage } from "./pages/Hud";
import { SettingsPage } from "./pages/SettingsPage";
import { topRate, type Capabilities, type DeviceState } from "./api";
import { LINK_NAME } from "./links";
import { MODEL_ART } from "./models";
import { BatteryIcon } from "./BatteryIcon";
import { WhatsNew } from "./WhatsNew";
import { Welcome } from "./Welcome";
import { TitleBar } from "./TitleBar";
import { SidebarFooter } from "./SidebarFooter";

export type PageId = "sensor" | "buttons" | "profiles" | "battery" | "hud" | "lab" | "settings";

/** What a page needs from the mouse; the navigation leaves it out for a mouse without it. The
 *  rest work for any mouse, or with none. */
const NEEDS: Partial<Record<PageId, (caps: Capabilities) => boolean>> = {
  buttons: (c) => !!c.buttons?.length,
  profiles: (c) => !!c.profiles,
};

const PAGES: { id: PageId; label: string; icon: ReactElement }[] = [
  { id: "sensor", label: "Sensor", icon: <TopSpeed20Regular /> },
  { id: "buttons", label: "Buttons", icon: <CursorClick20Regular /> },
  { id: "profiles", label: "Profiles", icon: <Stack20Regular /> },
  { id: "battery", label: "Battery", icon: <Battery820Regular /> },
  { id: "hud", label: "HUD", icon: <Games20Regular /> },
  // Tools that work but are still taking shape.
  { id: "lab", label: "Lab", icon: <Beaker20Regular /> },
  { id: "settings", label: "Settings", icon: <Settings20Regular /> },
];

const useStyles = makeStyles({
  // Our own title bar on top, the navigation and the page beneath it.
  frame: { display: "flex", flexDirection: "column", height: "100%" },
  shell: {
    display: "grid",
    gridTemplateColumns: "280px 1fr",
    flexGrow: 1,
    minHeight: 0,
  },
  pane: {
    display: "flex",
    flexDirection: "column",
    gap: tokens.spacingVerticalL,
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalM}`,
    overflowY: "auto",
  },
  // The mouse anchors the pane: a raised card on the same surface as everything else.
  card: {
    display: "flex",
    flexDirection: "column",
    paddingBottom: tokens.spacingVerticalM,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
    borderRadius: tokens.borderRadiusXLarge,
  },
  art: {
    alignSelf: "center",
    width: "176px",
    height: "auto",
    margin: `${tokens.spacingVerticalM} 0 ${tokens.spacingVerticalXS}`,
    pointerEvents: "none",
    userSelect: "none",
  },
  device: {
    display: "flex",
    flexDirection: "column",
    gap: "2px",
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM} 0`,
  },
  modelRow: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: tokens.spacingHorizontalS, minWidth: 0 },
  model: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  // Soft, like the rest of the shell: a light wash, no outline.
  rate: {
    flexShrink: 0,
    fontVariantNumeric: "tabular-nums",
    borderTopColor: "transparent",
    borderRightColor: "transparent",
    borderBottomColor: "transparent",
    borderLeftColor: "transparent",
  },
  rateIdle: { backgroundColor: tokens.colorSubtleBackgroundHover, color: tokens.colorNeutralForeground2 },
  linkState: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalS,
    color: tokens.colorNeutralForeground3,
  },
  dot: {
    width: "8px",
    height: "8px",
    borderRadius: tokens.borderRadiusCircular,
    backgroundColor: tokens.colorNeutralForeground4,
    flexShrink: 0,
  },
  dotOk: { backgroundColor: tokens.colorPaletteGreenForeground1 },
  dotWarn: { backgroundColor: tokens.colorPaletteMarigoldForeground1 },
  battery: {
    display: "flex",
    flexDirection: "column",
    gap: tokens.spacingVerticalXS,
    marginTop: tokens.spacingVerticalM,
  },
  batteryLine: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
  },
  batteryNumber: {
    fontVariantNumeric: "tabular-nums",
  },
  batteryIcon: { fontSize: "20px", color: tokens.colorNeutralForeground2 },
  muted: { color: tokens.colorNeutralForeground3, fontVariantNumeric: "tabular-nums" },
  nav: { display: "flex", flexDirection: "column", gap: "2px" },
  content: {
    display: "flex",
    flexDirection: "column",
    minWidth: 0,
    minHeight: 0,
  },
  // Fixed page header, as in Windows Settings and PowerToys: the page scrolls beneath it and is
  // clipped at its bottom edge, so it needs no background over Mica. Same width and side padding as
  // the page, so the title and buttons line up with the cards. The top padding puts the title's
  // letters level with the top of the device card, as Windows Settings does with its account card.
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: tokens.spacingHorizontalL,
    flexShrink: 0,
    maxWidth: "1000px",
    // Right: the page's 16px plus the scroller's 12px gutter, so header buttons line up with the cards.
    padding: `${tokens.spacingVerticalS} 28px ${tokens.spacingVerticalL} ${tokens.spacingHorizontalL}`,
  },
  title: { margin: 0, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  headerSide: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, flexShrink: 0 },
  scroller: {
    flexGrow: 1,
    minHeight: 0,
    overflowY: "auto",
    // Never scroll sideways: a stray horizontal scrollbar paints a light corner square.
    overflowX: "hidden",
    // Reserve the scrollbar's 12px even when nothing scrolls, so the right margin never changes.
    scrollbarGutter: "stable",
  },
  // 16px each side: with the pane's 12px padding on the left and the 12px gutter on the right,
  // the cards sit 28px from the navigation and 28px from the window edge.
  page: {
    maxWidth: "1000px",
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalL} 96px ${tokens.spacingHorizontalL}`,
    display: "flex",
    flexDirection: "column",
    gap: tokens.spacingVerticalM,
  },
  center: {
    display: "grid",
    placeItems: "center",
    flexGrow: 1,
    height: "100%",
    textAlign: "center",
  },
  emptyText: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: tokens.spacingVerticalS,
    maxWidth: "420px",
  },
});

function linkText(d: DeviceState | null): [string, "ok" | "warn" | "off"] {
  if (!d?.connected) return ["No receiver connected", "off"];
  if (!d.online) return [`Asleep · move to wake`, "warn"];
  return [`Connected · ${LINK_NAME[d.deviceType] ?? "receiver"}`, "ok"];
}

/** 8000 → "8K", 500 → "500 Hz": how the rate reads on a badge. */
const rateBadge = (hz: number) => (hz >= 1000 ? `${hz / 1000}K` : `${hz} Hz`);

/** The polling rate the mouse is set to (as far as its connection carries), and whether the
 *  battery saver lowered it. */
function RateBadge({ d }: { d: DeviceState }) {
  const s = useStyles();
  const restore = useMouse()?.prefs.batterySaverRestore ?? null;
  const set = d.settings?.pollingHz;
  if (set === undefined) return null;
  const hz = Math.min(set, topRate(d.capabilities) ?? set);
  const saving = restore !== null && restore !== hz;
  const label = saving
    ? `Lowered to ${hz.toLocaleString("en-US")} Hz by battery saver. Back to ${restore.toLocaleString("en-US")} Hz when charged.`
    : `Polling at ${hz.toLocaleString("en-US")} Hz`;
  return (
    <Badge
      className={mergeClasses(s.rate, !saving && s.rateIdle)}
      appearance="tint"
      shape="rounded"
      color={saving ? "warning" : "informative"}
      icon={saving ? <BatterySaverRegular /> : undefined}
      title={label}
      aria-label={label}
    >
      {rateBadge(hz)}
    </Badge>
  );
}

function DeviceCard() {
  const s = useStyles();
  const d = useStore((st) => st.device);
  const lowAt = useStore((st) => st.prefs?.lowBatteryPercent ?? 15);
  const [text, tone] = linkText(d);
  const b = d?.battery;
  const art = d?.connected ? MODEL_ART[d.mid] : undefined;
  return (
    <div className={s.card}>
      {art && <img className={s.art} src={art.image} alt="" width={art.size[0]} height={art.size[1]} />}
      <div className={s.device}>
        <div className={s.modelRow}>
          <Body1Strong className={s.model}>{d?.model ?? "Zaopin mouse"}</Body1Strong>
          {d?.connected && <RateBadge d={d} />}
        </div>
        <div className={s.linkState}>
          <span className={`${s.dot} ${tone === "ok" ? s.dotOk : tone === "warn" ? s.dotWarn : ""}`} aria-hidden />
          <Caption1>{text}</Caption1>
        </div>
        {b && d?.connected && (
          <div className={s.battery}>
            <div className={s.batteryLine}>
              <Body1
                className={s.batteryNumber}
                title={
                  b.charging
                    ? `Estimated. The mouse reads ${b.rawLevel}% while charging, which runs high.`
                    : undefined
                }
              >
                <strong>{b.level}%</strong> battery
              </Body1>
              <BatteryIcon level={b.level} charging={b.charging} className={s.batteryIcon} />
            </div>
            <ProgressBar
              value={b.level / 100}
              color={b.charging ? "brand" : b.level <= lowAt ? "error" : b.level <= lowAt * 2 ? "warning" : "success"}
              aria-label="Battery level"
            />
            <Caption1 className={s.muted}>
              {b.charging ? "Charging · " : ""}
              {(b.millivolts / 1000).toFixed(2)} V{d.firmware ? ` · firmware ${d.firmware}` : ""}
            </Caption1>
          </div>
        )}
      </div>
    </div>
  );
}

function Empty({ title, children }: { title: string; children: ReactNode }) {
  const s = useStyles();
  return (
    <div className={s.center}>
      <div className={s.emptyText}>
        <Subtitle1 as="h1">{title}</Subtitle1>
        <Body1 className={s.muted}>{children}</Body1>
      </div>
    </div>
  );
}

/** `pages` and `start` are for the page labs, which show a page's directions inside the real app. */
export default function App({ pages, start = "sensor" }: { pages?: Partial<Record<PageId, ComponentType>>; start?: PageId } = {}) {
  const s = useStyles();
  const [page, setPage] = useState<PageId>(start);
  const [ready, setReady] = useState(false);
  const [actionsSlot, setActionsSlot] = useState<HTMLDivElement | null>(null);
  const device = useStore((st) => st.device);
  const vendor = useStore((st) => st.status.vendorAppRunning);

  useEffect(() => {
    init().finally(() => setReady(true));
  }, []);

  // The battery log can be read without the mouse, and the HUD is set up without one.
  const needsDevice = page !== "settings" && page !== "battery" && page !== "hud";
  const asleep = !!device?.connected && !device.online && needsDevice;
  let body: ReactNode;
  if (!ready) {
    body = (
      <div className={s.center}>
        <Spinner label="Looking for your mouse" />
      </div>
    );
  } else if (needsDevice && !device?.connected) {
    body = (
      <Empty title="Plug in the receiver">
        Connect the Zaopin receiver, or plug in the mouse with its USB cable.
      </Empty>
    );
  } else if (needsDevice && !device?.settings) {
    // Awake shows as soon as the receiver reports it; the settings take about a second more.
    body = device?.online ? (
      <div className={s.center}>
        <Spinner label="Reading the mouse's settings" />
      </div>
    ) : (
      <Empty title="Move your mouse">
        The receiver is connected, but the mouse is asleep.
      </Empty>
    );
  } else {
    const Page = pages?.[page] ?? { sensor: SensorPage, buttons: ButtonsPage, profiles: ProfilesPage, battery: BatteryPage, hud: HudPage, lab: LabPage, settings: SettingsPage }[page];
    body = (
      <>
        <header className={s.header}>
          <Title2 as="h1" className={s.title}>
            {PAGES.find((p) => p.id === page)?.label}
          </Title2>
          <div className={s.headerSide}>
            <div role="status">
              {asleep && (
                <StatusPill
                  tone="asleep"
                  icon={<WeatherMoon16Filled />}
                  breathing
                  title="Mouse is asleep"
                  detail="Move it to make changes"
                  hint="Showing its last reported settings."
                />
              )}
            </div>
            <div ref={setActionsSlot} className={s.headerSide} />
          </div>
        </header>
        <div className={s.scroller}>
          <div className={s.page}>
            {vendor && (
              <Notice tone="warning" icon={<Warning16Filled />} title="Zaopin's app is running">
                Quit it from its tray icon. Both apps use the same channel and overwrite each other's changes.
              </Notice>
            )}
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
        <DeviceCard />
        <div
          className={s.nav}
          onKeyDown={(e) => {
            // Up/Down move between items like WinUI's NavigationView.
            if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
            const items = [...e.currentTarget.querySelectorAll("button")];
            const i = items.indexOf(document.activeElement as HTMLButtonElement);
            const next = items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length];
            next?.focus();
            e.preventDefault();
          }}
        >
          {PAGES.filter((p) => !device?.capabilities || (NEEDS[p.id]?.(device.capabilities) ?? true)).map((p) => (
            <NavRow key={p.id} icon={p.icon} label={p.label} selected={page === p.id} onClick={() => setPage(p.id)} />
          ))}
        </div>
        <SidebarFooter />
      </nav>
      <main className={s.content}>{body}</main>
      <ToastBridge />
      <Welcome />
      <WhatsNew />
    </div>
    </div>
    </HeaderActionsSlot.Provider>
  );
}
