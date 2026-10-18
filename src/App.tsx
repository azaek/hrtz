import { useEffect, useState, type ComponentType, type ReactElement, type ReactNode } from "react";
import { Body1, Button, makeStyles, Spinner, Subtitle1, Title2, tokens } from "@fluentui/react-components";
import {
  CursorClick20Regular,
  Beaker20Regular,
  Games20Regular,
  Settings20Regular,
  Battery820Regular,
  PlugDisconnected16Regular,
  Stack20Regular,
  TopSpeed20Regular,
  Warning16Filled,
  WeatherMoon16Filled,
} from "@fluentui/react-icons";
import { init, startFlow, useStore } from "./store";
import { HeaderActionsSlot, NavRow, Notice, StatusPill, ToastBridge } from "./components";
import { SensorPage } from "./pages/Sensor";
import { ButtonsPage } from "./pages/Buttons";
import { ProfilesPage } from "./pages/Profiles";
import { LabPage } from "./pages/Lab";
import { BatteryPage } from "./pages/Battery";
import { HudPage } from "./pages/Hud";
import { SettingsPage } from "./pages/SettingsPage";
import type { Capabilities, DeviceState } from "./api";
import { familyOf } from "./models";
import { Onboarding } from "./Onboarding";
import { Switcher } from "./Switcher";
import { WhatsNew } from "./WhatsNew";
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
  nav: { display: "flex", flexDirection: "column", gap: "2px" },
  muted: { color: tokens.colorNeutralForeground3 },
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

function Empty({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  const s = useStyles();
  return (
    <div className={s.center}>
      <div className={s.emptyText}>
        <Subtitle1 as="h1">{title}</Subtitle1>
        <Body1 className={s.muted}>{children}</Body1>
        {action}
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
  const vendorApps = useStore((st) => st.status.vendorApps);
  const flow = useStore((st) => st.flow);

  useEffect(() => {
    init().finally(() => setReady(true));
  }, []);

  // The battery log can be read without the mouse, and the HUD is set up without one.
  const needsDevice = page !== "settings" && page !== "battery" && page !== "hud";
  const asleep = !!device?.connected && !device.online && needsDevice;
  // A mouse that isn't connected shows what it held last time, to read, not change.
  const away = !!device && !device.connected && needsDevice;
  let body: ReactNode;
  if (!ready) {
    body = (
      <div className={s.center}>
        <Spinner label="Looking for your mouse" />
      </div>
    );
  } else if (needsDevice && !device) {
    body = (
      <Empty
        title="No mouse connected"
        action={
          <Button appearance="primary" onClick={() => startFlow("add")}>
            Connect a mouse
          </Button>
        }
      >
        Plug in its receiver or cable to set it up.
      </Empty>
    );
  } else if (away && !device?.settings) {
    body = <Empty title="Not connected">Plug in its receiver or cable to see its settings.</Empty>;
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
              {away && <StatusPill tone="asleep" icon={<PlugDisconnected16Regular />} title="Not connected" detail="Plug it in to make changes" hint="Showing its last settings." />}
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
            <MakerAppNotice device={device} running={vendorApps} />
            <Page />
          </div>
        </div>
      </>
    );
  }

  // Setting up a mouse takes the whole window.
  if (flow) {
    return (
      <>
        <Onboarding />
        <ToastBridge />
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
      <WhatsNew />
    </div>
    </div>
    </HeaderActionsSlot.Provider>
  );
}

/** The maker's app, or other software, in the way of the mouse shown: Zaopin's app fights hrtz over
 *  the same channel; G HUB edits the same onboard profiles, or drives the mouse itself (host mode,
 *  with its On-board memory mode off). */
function MakerAppNotice({ device, running }: { device: DeviceState | null; running: string[] }) {
  const family = familyOf(device?.key);
  if (!device?.connected || !family) return null;
  const app = running.includes(family);
  if (family === "compx" && app)
    return (
      <Notice tone="warning" icon={<Warning16Filled />} title="Zaopin's app is running">
        Quit it from its tray icon. Both apps use the same channel and overwrite each other's changes.
      </Notice>
    );
  if (family === "hidpp" && device.hostMode)
    return app ? (
      <Notice tone="warning" icon={<Warning16Filled />} title="G HUB is controlling this mouse">
        Changes here apply after you quit G HUB, or turn on On-board memory mode in G HUB.
      </Notice>
    ) : (
      <Notice tone="warning" icon={<Warning16Filled />} title="Another app is controlling this mouse">
        Changes here apply after it lets go of the mouse.
      </Notice>
    );
  if (family === "hidpp" && app)
    return (
      <Notice tone="warning" icon={<Warning16Filled />} title="G HUB is running">
        Quit it from its tray icon. Both apps change the profiles saved on the mouse, and the last change wins.
      </Notice>
    );
  return null;
}
