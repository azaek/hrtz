import type { ComponentProps } from "react";
import {
  Badge,
  Body1,
  Body1Strong,
  Caption1,
  makeStyles,
  mergeClasses,
  Menu,
  MenuDivider,
  MenuItem,
  MenuItemRadio,
  MenuList,
  MenuPopover,
  MenuSplitGroup,
  MenuTrigger,
  ProgressBar,
  tokens,
} from "@fluentui/react-components";
import { Add16Regular, BatterySaverRegular, ChevronUpDown16Regular, Delete16Regular, MoreHorizontal16Regular } from "@fluentui/react-icons";
import { topRate, type DeviceState, type KnownMouse } from "./api";
import { BatteryIcon } from "./BatteryIcon";
import { MousePicture } from "./DrawnMouse";
import { seenAt } from "./format";
import { linkName } from "./links";
import { artOf, ofModel } from "./models";
import { asleepOnPort, forgetMouse, goTo, pickMouse, restoreMouse, startFlow, toast, useMouse, useStore } from "./store";
import { FLYOUT_PADDING } from "./kit";

const useStyles = makeStyles({
  // The mouse anchors the pane: a raised card on the same surface as everything else. It's the
  // switcher's button too, with the rows' faint wash on hover.
  card: {
    display: "flex",
    flexDirection: "column",
    paddingBottom: tokens.spacingVerticalM,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
    borderRadius: tokens.borderRadiusXLarge,
    textAlign: "left",
    cursor: "pointer",
    ":hover": { backgroundImage: "linear-gradient(var(--nav-hover), var(--nav-hover))" },
    ":active": { backgroundImage: "linear-gradient(var(--nav-pressed), var(--nav-pressed))" },
  },
  art: {
    alignSelf: "center",
    margin: `${tokens.spacingVerticalM} 0 ${tokens.spacingVerticalXS}`,
    color: tokens.colorNeutralForeground3,
  },
  /** The card, with the "Not tested" chip over its corner: beside the switcher's button, not in it. */
  cardWrap: { position: "relative" },
  untested: {
    position: "absolute",
    top: tokens.spacingVerticalS,
    right: tokens.spacingHorizontalS,
    padding: 0,
    border: "none",
    background: "none",
    cursor: "pointer",
    borderRadius: tokens.borderRadiusMedium,
  },
  device: {
    display: "flex",
    flexDirection: "column",
    gap: "2px",
    padding: `${tokens.spacingVerticalS} ${tokens.spacingHorizontalM} 0`,
  },
  modelRow: { display: "flex", alignItems: "center", justifyContent: "space-between", gap: tokens.spacingHorizontalS, minWidth: 0 },
  name: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalXS, minWidth: 0 },
  model: { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  chevron: { flexShrink: 0, color: tokens.colorNeutralForeground3 },
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
  linkState: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground3, minWidth: 0 },
  dot: { width: "8px", height: "8px", borderRadius: tokens.borderRadiusCircular, backgroundColor: tokens.colorNeutralForeground4, flexShrink: 0 },
  dotOk: { backgroundColor: tokens.colorPaletteGreenForeground1 },
  dotWarn: { backgroundColor: tokens.colorPaletteMarigoldForeground1 },
  battery: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, marginTop: tokens.spacingVerticalM },
  batteryLine: { display: "flex", alignItems: "center", justifyContent: "space-between" },
  batteryNumber: { fontVariantNumeric: "tabular-nums" },
  batteryIcon: { fontSize: "20px", color: tokens.colorNeutralForeground2 },
  muted: { color: tokens.colorNeutralForeground3, fontVariantNumeric: "tabular-nums" },
  menuItem: { display: "grid", gridTemplateColumns: "44px minmax(0, 1fr)", alignItems: "center", gap: tokens.spacingHorizontalS, padding: "2px 0" },
  rowText: { display: "flex", flexDirection: "column", minWidth: 0 },
  thumb: { display: "flex", alignItems: "center", justifyContent: "center", width: "44px", color: tokens.colorNeutralForeground3 },
  // Fluent's auto-size keeps the flyout below the title bar and above the window's edge; the mice
  // scroll inside it, and "Connect a new mouse" stays in view under them.
  popover: { display: "flex", flexDirection: "column", overflowY: "hidden" },
  list: { minHeight: 0, flex: 1 },
  mice: { display: "flex", flexDirection: "column", gap: "4px", minHeight: 0, overflowY: "auto" },
});

/** "Connected · 8K receiver", or why it isn't. */
function linkText(d: DeviceState | null, lastSeen: number | undefined): [string, "ok" | "warn" | "off"] {
  if (!d) return ["Plug in a receiver or cable", "off"];
  if (!d.connected) return [lastSeen ? `Not connected · ${seenAt(lastSeen)}` : "Not connected", "off"];
  if (!d.online) return ["Asleep · move to wake", "warn"];
  return [`Connected · ${linkName(d)}`, "ok"];
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

/** A model hrtz lists but hasn't been tried on: the chip opens Settings at its check. */
function UntestedBadge({ d }: { d: DeviceState | null }) {
  const s = useStyles();
  const untested = useStore((st) => !!d?.connected && st.supported.some((m) => ofModel(d.key, m.key) && !m.tested));
  if (!untested) return null;
  const label = "hrtz hasn't been tried on this model yet. Help test it in Settings.";
  return (
    <button className={s.untested} title={label} aria-label={label} onClick={() => goTo("settings", "help-test")}>
      <Badge appearance="tint" shape="rounded" color="warning">
        Not tested
      </Badge>
    </button>
  );
}

/** The mouse the window shows, as the pane's card: its picture, how it's connected, its battery. */
function MouseCard({ d, ...rest }: { d: DeviceState | null } & ComponentProps<"div">) {
  const s = useStyles();
  const lowAt = useStore((st) => st.prefs?.lowBatteryPercent ?? 15);
  const lastSeen = useStore((st) => st.mice.find((m) => m.key === d?.key)?.lastSeen);
  const [text, tone] = linkText(d, lastSeen);
  const b = d?.connected ? d.battery : null;
  return (
    <div className={s.card} role="button" tabIndex={0} {...rest}>
      <div className={s.art}>
        <MousePicture image={artOf(d?.key, d?.variant)?.image} width={176} none={!d} />
      </div>
      <div className={s.device}>
        <div className={s.modelRow}>
          <span className={s.name}>
            <Body1Strong className={s.model}>{d?.model ?? "No mouse connected"}</Body1Strong>
            <ChevronUpDown16Regular className={s.chevron} />
          </span>
          {d?.connected && <RateBadge d={d} />}
        </div>
        <div className={s.linkState}>
          <span className={mergeClasses(s.dot, tone === "ok" && s.dotOk, tone === "warn" && s.dotWarn)} aria-hidden />
          <Caption1 className={s.model}>{text}</Caption1>
        </div>
        {b && d && (
          <div className={s.battery}>
            <div className={s.batteryLine}>
              <Body1
                className={s.batteryNumber}
                title={b.charging ? `Estimated. The mouse reads ${b.rawLevel}% while charging, which runs high.` : undefined}
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
              {[b.charging ? "Charging" : null, b.millivolts > 0 ? `${(b.millivolts / 1000).toFixed(2)} V` : null, d.firmware ? `firmware ${d.firmware}` : null]
                .filter(Boolean)
                .join(" · ")}
            </Caption1>
          </div>
        )}
      </div>
    </div>
  );
}

/** One line under a mouse's name in the list: battery and connection, or when it was last seen. */
function brief(m: KnownMouse, live: DeviceState | undefined, inUse: boolean): string {
  if (!live?.connected) return `Not connected · ${seenAt(m.lastSeen)}`;
  if (!live.online) return "Asleep";
  const parts = [live.battery ? `${live.battery.level}%` : null, linkName(live), inUse ? "in use" : null];
  return parts.filter(Boolean).join(" · ");
}

/**
 * The pane's card, and the switcher: it opens a list of every mouse hrtz knows (picture, name,
 * battery and connection, or when it was last seen), the one shown checked, then "Connect a new
 * mouse". Decided in the shell lab (DECISIONS.md, "Several mice").
 */
export function Switcher() {
  const s = useStyles();
  const device = useStore((st) => st.device);
  const devices = useStore((st) => st.devices);
  const mice = useStore((st) => st.mice);
  const inUse = useStore((st) => st.status.inUse);
  const quiet = useStore((st) => st.status.quietPorts);
  const remove = async (key: string) => {
    const gone = await forgetMouse(key);
    if (gone) toast(`Removed ${gone.model}.`, "info", { label: "Undo", run: () => void restoreMouse(gone) });
  };
  return (
    <Menu
      positioning={{ position: "below", align: "start", offset: 4, matchTargetSize: "width", autoSize: "height", overflowBoundaryPadding: FLYOUT_PADDING }}
      checkedValues={{ mouse: device?.key ? [device.key] : [] }}
      onCheckedValueChange={(_, d) => d.checkedItems[0] && pickMouse(d.checkedItems[0])}
    >
      <div className={s.cardWrap}>
        <MenuTrigger disableButtonEnhancement>
          <MouseCard d={device} aria-label="Switch mouse" />
        </MenuTrigger>
        <UntestedBadge d={device} />
      </div>
      <MenuPopover className={s.popover}>
        <MenuList className={s.list}>
          <div className={s.mice}>
            {mice.map((m) => {
              const asleep = asleepOnPort(m, quiet);
              const row = (
                <MenuItemRadio key={m.key} name="mouse" value={m.key}>
                  <span className={s.menuItem}>
                    <span className={s.thumb}>
                      <MousePicture image={artOf(m.key, m.last?.variant)?.image} width={44} />
                    </span>
                    <span className={s.rowText}>
                      <Body1 className={s.model}>{m.model}</Body1>
                      <Caption1 className={mergeClasses(s.muted, s.model)}>
                        {asleep ? "Asleep" : brief(m, devices[m.key], inUse === m.key && mice.length > 1)}
                      </Caption1>
                    </span>
                  </span>
                </MenuItemRadio>
              );
              // An unplugged mouse can be removed from here too, as from Settings, with Undo.
              if (devices[m.key]?.connected || asleep) return row;
              return (
                <MenuSplitGroup key={m.key}>
                  {row}
                  <Menu>
                    <MenuTrigger disableButtonEnhancement>
                      <MenuItem aria-label={`${m.model} actions`} submenuIndicator={<MoreHorizontal16Regular />} />
                    </MenuTrigger>
                    <MenuPopover>
                      <MenuList>
                        <MenuItem icon={<Delete16Regular />} onClick={() => void remove(m.key)}>
                          Remove
                        </MenuItem>
                      </MenuList>
                    </MenuPopover>
                  </Menu>
                </MenuSplitGroup>
              );
            })}
          </div>
          {mice.length > 0 && <MenuDivider />}
          <MenuItem icon={<Add16Regular />} onClick={() => startFlow("add")}>
            Connect a new mouse
          </MenuItem>
        </MenuList>
      </MenuPopover>
    </Menu>
  );
}
