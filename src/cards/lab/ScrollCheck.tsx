import { useEffect, useRef, useState } from "react";
import { Caption1, Link, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import type { MeterSample } from "@/api";
import { useRawInput } from "@/rawInput";
import { useReport, type TestProps, type TestResult } from "./result";

/** A notch against the way its neighbours went, this close to each, is the encoder misreading. */
const REVERSED_MS = 150;
/** A second notch this soon after one in slow scrolling is the encoder bouncing. */
const DOUBLE_MS = 6;
const SLOW_MS = 30;
const STRIP = 48;

type Flag = "ok" | "reversed" | "double";
type Tick = { dir: 1 | -1; t: number; flag: Flag };

const useStyles = makeStyles({
  body: { display: "grid", gridTemplateColumns: "minmax(180px, 1fr) minmax(0, 1.4fr)", gap: tokens.spacingHorizontalXL },
  target: {
    display: "grid",
    placeItems: "center",
    height: "120px",
    borderRadius: tokens.borderRadiusMedium,
    border: `1px solid ${tokens.colorNeutralStroke2}`,
    backgroundColor: tokens.colorSubtleBackground,
    color: tokens.colorNeutralForeground3,
    userSelect: "none",
  },
  side: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, minWidth: 0 },
  counts: { display: "flex", gap: tokens.spacingHorizontalXL, fontVariantNumeric: "tabular-nums" },
  count: { display: "flex", flexDirection: "column" },
  muted: { color: tokens.colorNeutralForeground3 },
  flagged: { color: tokens.colorPaletteRedForeground1 },
  // Notches up above the line, down below it, newest on the right.
  strip: { display: "flex", alignItems: "center", gap: "2px", height: "28px" },
  tick: { width: "4px", height: "10px", borderRadius: "1px", backgroundColor: tokens.colorNeutralForeground3 },
  up: { transform: "translateY(-6px)" },
  down: { transform: "translateY(6px)" },
  reversed: { backgroundColor: tokens.colorPaletteRedForeground1 },
  double: { backgroundColor: tokens.colorPaletteMarigoldForeground1 },
  verdict: { minHeight: "32px", color: tokens.colorNeutralForeground2 },
  actions: { display: "flex", gap: tokens.spacingHorizontalL, minHeight: "32px", alignItems: "center" },
});

/** Add one notch, and flag it or the one before it once its neighbours are known. */
function add(ticks: Tick[], dir: 1 | -1, t: number) {
  const prev = ticks[ticks.length - 1];
  const before = ticks[ticks.length - 2];
  let flag: Flag = "ok";
  if (prev && prev.dir === dir && t - prev.t < DOUBLE_MS && (!before || prev.t - before.t > SLOW_MS)) flag = "double";
  ticks.push({ dir, t, flag });
  // The one before is wrong-way if both its neighbours agree against it, close in time.
  if (prev && before && before.dir === dir && prev.dir !== dir && prev.t - before.t < REVERSED_MS && t - prev.t < REVERSED_MS) {
    prev.flag = "reversed";
  }
}

/**
 * Scroll wheel check: an encoder with dirt or grease in it reads the odd notch backwards, or
 * twice. Raw input sees each notch with its time, so both show up here.
 */
export function ScrollCheck({ onResult }: TestProps) {
  const s = useStyles();
  const ticks = useRef<Tick[]>([]);
  const carry = useRef(0);
  const [, redraw] = useState(0);
  const target = useRef<HTMLDivElement>(null);

  const clear = () => {
    ticks.current = [];
    carry.current = 0;
    redraw((n) => n + 1);
  };
  // Scrolling over the box shouldn't scroll the page away from it.
  useEffect(() => {
    const el = target.current;
    if (!el) return;
    const stop = (e: WheelEvent) => e.preventDefault();
    el.addEventListener("wheel", stop, { passive: false });
    return () => el.removeEventListener("wheel", stop);
  }, []);

  useRawInput(true, (sample: MeterSample) => {
    if (!sample.wheel.length) return;
    for (const w of sample.wheel) {
      if (w.horizontal) continue;
      // 120 a notch; a wheel that reports finer steps adds up to whole notches.
      carry.current += w.delta;
      while (Math.abs(carry.current) >= 120) {
        const dir = carry.current > 0 ? 1 : -1;
        carry.current -= dir * 120;
        add(ticks.current, dir, w.tUs / 1000);
      }
    }
    redraw((n) => n + 1);
  });

  const all = ticks.current;
  const down = all.filter((t) => t.dir < 0).length;
  const up = all.length - down;
  const reversed = all.filter((t) => t.flag === "reversed").length;
  const doubled = all.filter((t) => t.flag === "double").length;

  let verdict: string;
  if (all.length < 10) verdict = "Scroll down 20 notches at your usual pace, then up 20.";
  else if (reversed) {
    verdict = `${reversed} of ${all.length} notches went the wrong way. The wheel's encoder is misreading, usually from dirt or grease. Cleaning it often fixes this.`;
    if (doubled) verdict += ` ${doubled} more registered twice.`;
  } else if (doubled) {
    verdict = `${doubled} of ${all.length} notches registered twice, a few milliseconds apart. The encoder is bouncing. Cleaning it often helps.`;
  } else verdict = `All ${all.length} notches registered once, in the right direction.`;

  const misread = reversed + doubled;
  const result: TestResult | null =
    all.length < 10
      ? null
      : misread
        ? { value: `${misread} ${misread === 1 ? "misread" : "misreads"}`, detail: [reversed && `${reversed} the wrong way`, doubled && `${doubled} twice`].filter(Boolean).join(", ") + ` in ${all.length} notches`, tone: "warn" }
        : { value: "No misreads", detail: `In ${all.length} notches`, tone: "good" };
  useReport(onResult, result);

  const body = (
    <div className={s.body}>
      <div ref={target} className={s.target} aria-hidden>
        <Caption1>Scroll here</Caption1>
      </div>
      <div className={s.side}>
        <div className={s.counts}>
          <span className={s.count}>
            <Caption1 className={s.muted}>Down</Caption1>
            <Caption1>{down}</Caption1>
          </span>
          <span className={s.count}>
            <Caption1 className={s.muted}>Up</Caption1>
            <Caption1>{up}</Caption1>
          </span>
          <span className={s.count}>
            <Caption1 className={s.muted}>Wrong way</Caption1>
            <Caption1 className={reversed ? s.flagged : undefined}>{reversed}</Caption1>
          </span>
          <span className={s.count}>
            <Caption1 className={s.muted}>Twice</Caption1>
            <Caption1 className={doubled ? s.flagged : undefined}>{doubled}</Caption1>
          </span>
        </div>
        <div className={s.strip} aria-hidden>
          {all.slice(-STRIP).map((t, i) => (
            <span
              key={all.length - STRIP + i}
              className={mergeClasses(s.tick, t.dir > 0 ? s.up : s.down, t.flag === "reversed" && s.reversed, t.flag === "double" && s.double)}
            />
          ))}
        </div>
        <Caption1 className={s.verdict} aria-live="polite">
          {verdict}
        </Caption1>
        <div className={s.actions}>
          <Link as="button" disabled={!all.length} onClick={clear}>
            Clear
          </Link>
        </div>
      </div>
    </div>
  );
  return body;
}
