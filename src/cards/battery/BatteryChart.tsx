import { useEffect, useRef, useState } from "react";
import { Caption1, makeStyles, tokens } from "@fluentui/react-components";
import type { BatteryEntry } from "@/api";
import { hzText, rateColor } from "@/rates";
import { DAY } from "./format";

/** A longer gap between log rows means the app wasn't running (matches `battery_log.rs`). */
const GAP = 45 * 60;
const CHART_HEIGHT = 160;
/** Room for the level labels on the left, and the awake band and time labels below (px). */
const LEFT = 36;
const BAND = 6;
const BELOW = 26;

const useStyles = makeStyles({
  muted: { color: tokens.colorNeutralForeground3 },
  chartWrap: { position: "relative", minWidth: 0 },
  canvas: { display: "block" },
  tooltip: {
    position: "absolute",
    pointerEvents: "none",
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--flyout)",
    boxShadow: tokens.shadow8,
    whiteSpace: "nowrap",
    zIndex: 1,
  },
  legend: { display: "flex", flexWrap: "wrap", alignItems: "center", columnGap: tokens.spacingHorizontalL, rowGap: tokens.spacingVerticalXS, color: tokens.colorNeutralForeground3 },
  key: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXS },
  swatch: { width: "10px", height: "10px", borderRadius: "2px" },
});

/** The level over time: charging shaded, a band below for time awake coloured by polling rate. */
export function BatteryChart({ rows, from, now }: { rows: BatteryEntry[]; from: number; now: number }) {
  const s = useStyles();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<{ x: number; row: BatteryEntry } | null>(null);
  const shown = rows.filter((r) => r.t >= from);
  const drawn = shown.length >= 2;

  // The chart mounts once there's something to draw, so watch its size from then.
  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, [drawn]);

  const plotW = Math.max(0, width - LEFT - 8);
  const x = (t: number) => LEFT + ((t - from) / Math.max(1, now - from)) * plotW;
  const y = (level: number) => 6 + (1 - level / 100) * CHART_HEIGHT;

  useEffect(() => {
    const c = canvas.current;
    if (!c || !width) return;
    const dpr = window.devicePixelRatio || 1;
    const h = 6 + CHART_HEIGHT + 8 + BAND + BELOW;
    c.width = Math.round(width * dpr);
    c.height = Math.round(h * dpr);
    c.style.width = `${width}px`;
    c.style.height = `${h}px`;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(c);
    const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
    const stroke = v("--colorNeutralStroke2", "#555");
    const muted = v("--colorNeutralForeground3", "#aaa");
    const accent = v("--colorBrandForeground1", "#479ef5");
    const green = v("--colorPaletteGreenForeground1", "#54b054");
    const font = v("--fontFamilyBase", "sans-serif");
    const bandY = 6 + CHART_HEIGHT + 8;

    // Level grid, labelled on the left.
    g.font = `11px ${font}`;
    g.textBaseline = "middle";
    g.textAlign = "right";
    for (const level of [0, 25, 50, 75, 100]) {
      g.strokeStyle = stroke;
      g.globalAlpha = level % 50 ? 0.5 : 1;
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(LEFT, Math.round(y(level)) + 0.5);
      g.lineTo(LEFT + plotW, Math.round(y(level)) + 0.5);
      g.stroke();
      g.globalAlpha = 1;
      if (level % 50 === 0) {
        g.fillStyle = muted;
        g.fillText(`${level}%`, LEFT - 6, y(level));
      }
    }

    // Charging, and time awake by rate, both between consecutive rows the app saw.
    for (let i = 0; i < shown.length; i++) {
      const a = shown[i];
      const b = shown[i + 1];
      const end = b && b.t - a.t <= GAP ? b.t : Math.min(now, a.t + 60);
      if (a.charging) {
        g.fillStyle = green;
        g.globalAlpha = 0.16;
        g.fillRect(x(a.t), y(100), Math.max(1, x(end) - x(a.t)), CHART_HEIGHT);
        g.globalAlpha = 1;
      }
      if (a.awake) {
        g.fillStyle = rateColor(a.hz);
        g.fillRect(x(a.t), bandY, Math.max(1, x(end) - x(a.t)), BAND);
      }
    }

    // The level, broken where the app wasn't running.
    g.strokeStyle = accent;
    g.lineWidth = 2;
    g.lineJoin = "round";
    g.beginPath();
    shown.forEach((r, i) => {
      const gap = i === 0 || r.t - shown[i - 1].t > GAP;
      if (gap) g.moveTo(x(r.t), y(r.level));
      else g.lineTo(x(r.t), y(r.level));
    });
    g.stroke();

    // Time labels.
    g.fillStyle = muted;
    g.textAlign = "center";
    g.textBaseline = "top";
    const span = now - from;
    const step = span <= DAY ? 6 * 3600 : span <= 7 * DAY ? DAY : 7 * DAY;
    const start = new Date(from * 1000);
    start.setMinutes(0, 0, 0);
    if (step >= DAY) start.setHours(0);
    else start.setHours(Math.ceil(start.getHours() / 6) * 6);
    for (let t = start.getTime() / 1000; t <= now; t += step) {
      if (t < from) continue;
      const d = new Date(t * 1000);
      const text =
        step < DAY
          ? d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
          : step === DAY
            ? d.toLocaleDateString(undefined, { weekday: "short" })
            : d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
      g.fillText(text, x(t), bandY + BAND + 6);
    }

    if (hover) {
      g.strokeStyle = muted;
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(Math.round(hover.x) + 0.5, y(100));
      g.lineTo(Math.round(hover.x) + 0.5, bandY + BAND);
      g.stroke();
      g.fillStyle = accent;
      g.beginPath();
      g.arc(x(hover.row.t), y(hover.row.level), 3.5, 0, Math.PI * 2);
      g.fill();
    }
  }, [shown, width, from, now, hover]);

  if (!drawn) {
    return <Caption1 className={s.muted}>No history yet. The level appears here as you use the mouse.</Caption1>;
  }

  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const px = e.clientX - e.currentTarget.getBoundingClientRect().left;
    if (px < LEFT || px > LEFT + plotW) return setHover(null);
    const t = from + ((px - LEFT) / plotW) * (now - from);
    // The row in force at that moment.
    let row = shown[0];
    for (const r of shown) {
      if (r.t > t) break;
      row = r;
    }
    setHover({ x: px, row });
  };

  const rates = [...new Set(shown.filter((r) => r.awake).map((r) => r.hz))].sort((a, b) => a - b);
  const tip = hover
    ? [
        new Date(hover.row.t * 1000).toLocaleString(undefined, { weekday: "short", hour: "2-digit", minute: "2-digit" }),
        `${hover.row.level}%`,
        hover.row.charging ? "charging" : hover.row.awake ? hzText(hover.row.hz) : "asleep",
      ].join(" · ")
    : "";
  const lowest = Math.min(...shown.map((r) => r.level));
  const charged = shown.some((r) => r.charging);

  return (
    <>
      <div ref={wrap} className={s.chartWrap}>
        <canvas
          ref={canvas}
          className={s.canvas}
          role="img"
          aria-label={`Battery level over the period shown, lowest ${lowest}%${charged ? ", with charging" : ""}`}
          onMouseMove={onMove}
          onMouseLeave={() => setHover(null)}
        />
        {hover && (
          <div className={s.tooltip} style={{ left: Math.min(hover.x + 12, Math.max(0, width - 220)), top: 8 }}>
            <Caption1>{tip}</Caption1>
          </div>
        )}
      </div>
      <div className={s.legend}>
        <span className={s.key}>
          <span className={s.swatch} style={{ background: tokens.colorPaletteGreenForeground1, opacity: 0.4 }} aria-hidden />
          <Caption1>Charging</Caption1>
        </span>
        {rates.map((hz) => (
          <span key={hz} className={s.key}>
            <span className={s.swatch} style={{ background: rateColor(hz) }} aria-hidden />
            <Caption1>Awake at {hzText(hz)}</Caption1>
          </span>
        ))}
      </div>
    </>
  );
}
