import { useEffect, useRef, useState } from "react";
import { Body1, Button, Caption1, Caption1Strong, Link, makeStyles, Title2, tokens } from "@fluentui/react-components";
import type { MeterSample } from "@/api";
import { Choice } from "@/components";
import { useRawInput } from "@/rawInput";
import { commit, useStore } from "@/store";
import { useReport, type TestProps, type TestResult } from "./result";

const DISTANCES = [5, 10, 20] as const;
/** Runs kept per axis. */
const KEEP = 6;
/** A run counts for an axis when it stays this close to it (3:1, about 18°). */
const STRAIGHT = 3;
/** Within this the sensor is accurate; past `FIX` it's worth correcting (RTINGS uses ±1%). */
const FINE = 1;
const FIX = 2;

type Axis = "x" | "y";
type Run = { dpi: number; axis: Axis; speed: number };

const useStyles = makeStyles({
  // The result drops under the steps when the pane has no room for both.
  body: { display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))", columnGap: tokens.spacingHorizontalXL, rowGap: tokens.spacingVerticalL, alignItems: "start" },
  steps: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS },
  muted: { color: tokens.colorNeutralForeground3 },
  // Every row is always present, so the result doesn't push anything around as runs come in.
  result: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS },
  value: { fontVariantNumeric: "tabular-nums" },
  axes: { display: "grid", gridTemplateColumns: "auto 1fr", columnGap: tokens.spacingHorizontalM, rowGap: "2px", fontVariantNumeric: "tabular-nums" },
  note: { color: tokens.colorNeutralForeground2, minHeight: "16px" },
  actions: { display: "flex", flexWrap: "wrap", alignItems: "center", gap: tokens.spacingHorizontalL },
});

const fmt = (n: number) => Math.round(n).toLocaleString("en-US");
const mean = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);
const pct = (n: number) => `${Math.abs(n).toFixed(1).replace(/\.0$/, "")}%`;

/**
 * Real DPI from measured drags: hold the left button, slide a known distance along a ruler, let
 * go. Raw input counts the sensor's own counts, so pointer speed and acceleration don't matter.
 * Runs across and up and down are kept apart, since sensors can differ by axis, and fast runs are
 * compared with slow ones.
 */
export function DpiCheck({ dpi, onResult }: { dpi: number } & TestProps) {
  const s = useStyles();
  const device = useStore((x) => x.device);
  const [cm, setCm] = useState<(typeof DISTANCES)[number]>(10);
  const [runs, setRuns] = useState<Run[]>([]);
  const [corrected, setCorrected] = useState<{ from: number; to: number } | null>(null);
  const pressedAt = useRef<number | null>(null);

  // Runs at another DPI or distance don't average with these.
  useEffect(() => setRuns([]), [dpi, cm]);

  useRawInput(true, (sample: MeterSample) => {
    for (const b of sample.buttons) {
      if (b.button !== 0) continue;
      if (b.down) {
        pressedAt.current = b.tUs;
        continue;
      }
      const counts = Math.hypot(b.dx, b.dy);
      const measured = counts / (cm / 2.54);
      // Ignore ordinary clicks and stray drags: a real run lands near the set DPI.
      if (measured < dpi * 0.5 || measured > dpi * 1.5) continue;
      const [ax, ay] = [Math.abs(b.dx), Math.abs(b.dy)];
      const axis: Axis | null = ax >= ay * STRAIGHT ? "x" : ay >= ax * STRAIGHT ? "y" : null;
      if (!axis) continue;
      const secs = pressedAt.current === null ? 0 : (b.tUs - pressedAt.current) / 1e6;
      const run = { dpi: measured, axis, speed: secs > 0 ? cm / secs : 0 };
      setCorrected(null);
      setRuns((r) => [...r.filter((x) => x.axis !== axis), ...r.filter((x) => x.axis === axis).slice(1 - KEEP), run]);
    }
  });

  const x = runs.filter((r) => r.axis === "x");
  const y = runs.filter((r) => r.axis === "y");
  const [mx, my, all] = [mean(x.map((r) => r.dpi)), mean(y.map((r) => r.dpi)), mean(runs.map((r) => r.dpi))];
  const off = all === null ? null : ((all - dpi) / dpi) * 100;

  // Axis against axis, once both have runs.
  let axes = "";
  if (mx !== null && my !== null) {
    const d = ((my - mx) / mx) * 100;
    axes = Math.abs(d) < FINE ? "Both axes agree." : `Up and down reads ${pct(d)} ${d > 0 ? "higher" : "lower"} than across.`;
  }

  // Fast runs against slow ones, each against its own axis's average.
  let speed = "";
  const timed = runs.filter((r) => r.speed > 0);
  const speeds = timed.map((r) => r.speed).sort((a, b) => a - b);
  if (timed.length >= 4 && speeds[speeds.length - 1] >= speeds[0] * 1.8) {
    const mid = speeds[Math.floor(speeds.length / 2)];
    const dev = (r: Run) => (r.dpi / ((r.axis === "x" ? mx : my) ?? r.dpi) - 1) * 100;
    const fast = mean(timed.filter((r) => r.speed >= mid).map(dev))!;
    const slow = mean(timed.filter((r) => r.speed < mid).map(dev))!;
    const d = fast - slow;
    speed = Math.abs(d) < FINE ? "Fast and slow runs agree." : `Faster runs read ${pct(d)} ${d > 0 ? "higher" : "lower"} than slow ones.`;
  }

  // The setting that lands closest to the DPI you meant, in the mouse's own steps.
  const settings = device?.settings;
  const caps = device?.capabilities;
  const step = caps?.dpi?.step ?? 50;
  let fix: { to: number; real: number } | null = null;
  if (all !== null && runs.length >= 3 && Math.abs(off!) >= FINE && settings && caps?.dpi) {
    const ratio = all / dpi;
    const to = Math.min(caps.dpi.max, Math.max(caps.dpi.min, Math.round(dpi / ratio / step) * step));
    if (to !== dpi) fix = { to, real: to * ratio };
  }
  const apply = () => {
    if (!fix || !settings?.stages) return;
    const stages = settings.stages.map((st, i) => (i === settings.dpiStage ? { ...st, dpi: fix!.to } : st));
    setCorrected({ from: dpi, to: fix.to });
    commit({ ...settings, stages });
  };

  let verdict: string;
  if (corrected) verdict = `Set to ${fmt(corrected.to)} DPI. Measure again to confirm it reads near ${fmt(corrected.from)}.`;
  else if (off === null) verdict = `Your setting: ${fmt(dpi)} DPI.`;
  else if (Math.abs(off) < FINE) verdict = `Accurate, within ${pct(off)} of the set ${fmt(dpi)} DPI.`;
  else
    verdict = `${pct(off)} ${off > 0 ? "above" : "below"} the set ${fmt(dpi)} DPI${Math.abs(off) >= FIX ? ", enough to notice when matching sensitivity across mice" : ""}.`;

  const result: TestResult | null =
    all === null || off === null
      ? null
      : {
          value: `${fmt(all)} DPI`,
          detail: Math.abs(off) < FINE ? `Within ${pct(off)} of the set ${fmt(dpi)}` : `${pct(off)} ${off > 0 ? "above" : "below"} the set ${fmt(dpi)}`,
          tone: Math.abs(off) < FINE ? "good" : Math.abs(off) >= FIX ? "warn" : "neutral",
        };
  useReport(onResult, result);

  const body = (
    <div className={s.body}>
      <div className={s.steps}>
        <Choice label="Distance" value={cm} options={DISTANCES.map((d) => ({ value: d, label: `${d} cm` }))} onChange={setCm} />
        <Caption1>
          Line the mouse up with the ruler's zero. Hold the left button, slide straight to the {cm} cm mark and release. Repeat a few times
          across and up and down, at varied speeds.
        </Caption1>
        <Caption1 className={s.muted}>
          Every millimetre off is {(10 / cm).toFixed(1).replace(".0", "")}% on {cm} cm, so take a few runs.
        </Caption1>
      </div>
      <div className={s.result} aria-live="polite">
        <Caption1Strong>{runs.length ? `Average of ${runs.length} ${runs.length === 1 ? "run" : "runs"}` : "No runs yet"}</Caption1Strong>
        <Title2 className={s.value}>{all === null ? "—" : `${fmt(all)} DPI`}</Title2>
        <div className={s.axes}>
          <Caption1 className={s.muted}>Across</Caption1>
          <Caption1>{mx === null ? "—" : `${fmt(mx)} DPI · ${x.length} ${x.length === 1 ? "run" : "runs"}`}</Caption1>
          <Caption1 className={s.muted}>Up and down</Caption1>
          <Caption1>{my === null ? "—" : `${fmt(my)} DPI · ${y.length} ${y.length === 1 ? "run" : "runs"}`}</Caption1>
        </div>
        <Body1 className={s.note}>{verdict}</Body1>
        <Caption1 className={s.note}>{[axes, speed].filter(Boolean).join(" ")}</Caption1>
        <div className={s.actions}>
          {fix && !corrected && (
            <Button onClick={apply} disabled={!device?.online}>
              Use {fmt(fix.to)} DPI (about {fmt(fix.real)} real)
            </Button>
          )}
          <Link as="button" disabled={!runs.length} onClick={() => setRuns([])}>
            Clear runs
          </Link>
        </div>
      </div>
    </div>
  );
  return body;
}
