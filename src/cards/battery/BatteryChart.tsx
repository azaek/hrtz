import { useEffect, useMemo, useRef, useState } from "react";
import { Caption1, makeStyles, tokens } from "@fluentui/react-components";
import type { BatteryEntry, BatterySummary } from "@/api";
import { hzText } from "@/rates";
import { BatteryFooter } from "./BatteryFooter";
import { awakeIn, CHART_HEIGHT, drawChart, hoursPerBar, RIGHT, slotsOf, tints, type Mark, type Slot } from "./drawChart";
import { duration } from "./format";

const useStyles = makeStyles({
  muted: { color: tokens.colorNeutralForeground3 },
  chartWrap: { position: "relative", minWidth: 0 },
  canvas: { display: "block", borderRadius: tokens.borderRadiusMedium },
  tooltip: {
    position: "absolute",
    pointerEvents: "none",
    display: "flex",
    flexDirection: "column",
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--flyout)",
    boxShadow: tokens.shadow8,
    whiteSpace: "nowrap",
    zIndex: 1,
  },
  legend: { display: "flex", flexWrap: "wrap", columnGap: tokens.spacingHorizontalL, color: tokens.colorNeutralForeground3 },
  key: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXS },
  dot: { width: "8px", height: "8px", borderRadius: "50%" },
  dash: { width: "12px", height: 0, borderTopWidth: "1px", borderTopStyle: "dashed" },
});

/** What a bar covers and what happened in it: the time, then the level and state. */
function describe(slot: Slot, days: number): [string, string] {
  // A day's bar is its date; a shorter one, its hours.
  const when =
    hoursPerBar(days) >= 24
      ? new Date(slot.start * 1000).toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })
      : `${new Date(slot.start * 1000).toLocaleString(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" })} – ${new Date(slot.end * 1000).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
  if (slot.level == null) return [when, "No readings"];
  const awake = awakeIn(slot);
  // The rate it spent longest at.
  const hz = [...slot.awake].sort((a, b) => b[1] - a[1])[0]?.[0];
  const state = [
    `${slot.level}%`,
    slot.charging && "Charging",
    slot.saver && "Battery saver",
    awake > 0 ? `Awake ${duration(awake / 3600)}${hz ? ` at ${hzText(hz)}` : ""}` : "Asleep",
  ];
  return [when, state.filter(Boolean).join(" · ")];
}

/**
 * The level as bars, an hour each over a day (`drawChart`), with dashed lines at the low battery
 * alert's level (`low`) and battery saver's (`saverAt`) when they're on. Point at a bar, or focus
 * the chart and use the arrow keys, for what happened in it; the polling rate is told there. The
 * figures under it are `BatteryFooter`.
 */
export function BatteryChart({ rows, days, now, summary, hz, low, saverAt }: { rows: BatteryEntry[]; days: number; now: number; summary: BatterySummary | null; hz?: number; low?: number; saverAt?: number }) {
  const s = useStyles();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<number | null>(null);
  const slots = useMemo(() => slotsOf(rows, days, now), [rows, days, now]);
  const drawn = slots.some((x) => x.level != null);
  // Read each render, so a change of theme redraws in its colours.
  const dark = document.documentElement.dataset.theme === "dark";
  const tint = tints(dark);

  // The chart mounts once there's something to draw, so watch its size from then.
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, [drawn]);

  const plotW = Math.max(0, width - RIGHT);
  const slotW = plotW / slots.length;

  useEffect(() => {
    const c = canvas.current;
    if (!c || !width) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(width * dpr);
    c.height = Math.round(CHART_HEIGHT * dpr);
    c.style.width = `${width}px`;
    c.style.height = `${CHART_HEIGHT}px`;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(c);
    const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
    const look = {
      ...tints(dark),
      stroke: v("--colorNeutralStroke2", "#555"),
      muted: v("--colorNeutralForeground3", "#aaa"),
      ink: v("--colorNeutralForeground1", "#fff"),
      font: v("--fontFamilyBase", "sans-serif"),
    };
    const marks: Mark[] = [];
    if (low != null) marks.push({ level: low, color: look.muted });
    if (saverAt != null) marks.push({ level: saverAt, color: look.saver });
    drawChart(g, { width, slots, days, hover, marks, look });
  }, [slots, width, days, hover, dark, low, saverAt]);

  if (!drawn) {
    return (
      <>
        <Caption1 className={s.muted}>No history yet. The level appears here as you use the mouse.</Caption1>
        <BatteryFooter totals={null} summary={summary} hz={hz} />
      </>
    );
  }

  const last = slots.length - 1;
  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const i = Math.floor((e.clientX - e.currentTarget.getBoundingClientRect().left) / slotW);
    setHover(i >= 0 && i <= last ? i : null);
  };
  // The arrow keys step through the bars from the latest; Home and End go to either end.
  const onKey = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    const from = hover ?? last;
    const next = e.key === "ArrowLeft" ? Math.max(0, from - 1) : e.key === "ArrowRight" ? Math.min(last, from + 1) : e.key === "Home" ? 0 : e.key === "End" ? last : null;
    if (next == null) return;
    e.preventDefault();
    setHover(next);
  };

  const awake = slots.reduce((sum, x) => sum + awakeIn(x), 0);
  const asleep = slots.reduce((sum, x) => sum + x.asleep, 0);
  const charged = slots.some((x) => x.charging);
  const saved = slots.some((x) => x.saver);
  const lowest = Math.min(...slots.flatMap((x) => (x.level == null ? [] : [x.level])));
  const tip = hover != null ? describe(slots[hover], days) : null;

  return (
    <>
      {(charged || saved || low != null || saverAt != null) && (
        <div className={s.legend}>
          {charged && (
            <span className={s.key}>
              <span className={s.dot} style={{ background: tint.charging }} aria-hidden />
              <Caption1>Charging</Caption1>
            </span>
          )}
          {saved && (
            <span className={s.key}>
              <span className={s.dot} style={{ background: tint.saver }} aria-hidden />
              <Caption1>Battery saver</Caption1>
            </span>
          )}
          {low != null && (
            <span className={s.key}>
              <span className={s.dash} style={{ borderTopColor: tokens.colorNeutralForeground3 }} aria-hidden />
              <Caption1>Low battery alert</Caption1>
            </span>
          )}
          {saverAt != null && (
            <span className={s.key}>
              <span className={s.dash} style={{ borderTopColor: tint.saver }} aria-hidden />
              <Caption1>Battery saver starts</Caption1>
            </span>
          )}
        </div>
      )}
      <div ref={wrap} className={s.chartWrap}>
        <canvas
          ref={canvas}
          className={s.canvas}
          tabIndex={0}
          role="slider"
          aria-label={`Battery level over the period shown, lowest ${lowest}%${charged ? ", with charging" : ""}`}
          aria-valuemin={0}
          aria-valuemax={last}
          aria-valuenow={hover ?? last}
          aria-valuetext={tip ? tip.join(", ") : undefined}
          onMouseMove={onMove}
          onMouseLeave={(e) => document.activeElement !== e.currentTarget && setHover(null)}
          onFocus={() => setHover((h) => h ?? last)}
          onBlur={() => setHover(null)}
          onKeyDown={onKey}
        />
        {tip && hover != null && (
          <div className={s.tooltip} style={{ left: Math.min(hover * slotW + slotW + 8, Math.max(0, width - 240)), top: 0 }}>
            <Caption1 className={s.muted}>{tip[0]}</Caption1>
            <Caption1>{tip[1]}</Caption1>
          </div>
        )}
      </div>
      <BatteryFooter totals={{ awake, asleep }} summary={summary} hz={hz} />
    </>
  );
}
