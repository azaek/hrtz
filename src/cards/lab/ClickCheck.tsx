import { useRef, useState } from "react";
import { Button, Caption1, Caption1Strong, Link, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import type { MeterSample } from "@/api";
import { useRawInput } from "@/rawInput";
import { commit, useStore } from "@/store";
import { useClickWeek, weekText } from "./clicks";
import { useReport, type TestProps, type TestResult } from "./result";

const NAMES = ["Left", "Right", "Middle", "Back", "Forward"];
/** A release followed by a press sooner than this is the switch bouncing, not a finger. */
const DOUBLE_MS = 30;
/** The mouse's longest click debounce. */
const MAX_DEBOUNCE = 20;

type Stat = { presses: number; doubles: number; worst: number; lastUp: number | null };
const fresh = (): Stat[] => NAMES.map(() => ({ presses: 0, doubles: 0, worst: 0, lastUp: null }));

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
    cursor: "pointer",
    ":active": { backgroundColor: tokens.colorSubtleBackgroundPressed },
  },
  side: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, minWidth: 0 },
  table: {
    display: "grid",
    gridTemplateColumns: "1fr auto auto",
    columnGap: tokens.spacingHorizontalL,
    rowGap: "2px",
    fontVariantNumeric: "tabular-nums",
  },
  head: { color: tokens.colorNeutralForeground3 },
  num: { textAlign: "right" },
  flagged: { color: tokens.colorPaletteRedForeground1 },
  verdict: { minHeight: "32px", color: tokens.colorNeutralForeground2 },
  week: { color: tokens.colorNeutralForeground3 },
  weekWarn: { color: tokens.colorPaletteMarigoldForeground1 },
  actions: { display: "flex", gap: tokens.spacingHorizontalL, minHeight: "32px", alignItems: "center" },
});

/**
 * Double-click check: a worn switch can register one press as two, a release and a press a few
 * milliseconds apart. Raw input timestamps every button change, so those show up here, and click
 * debounce can filter the short ones.
 */
export function ClickCheck({ onResult }: TestProps) {
  const s = useStyles();
  const settings = useStore((x) => x.device?.settings);
  const online = useStore((x) => !!x.device?.online);
  const stats = useRef(fresh());
  const [view, setView] = useState(fresh);
  const week = useClickWeek();
  const counting = useStore((x) => x.prefs?.clickHistory ?? true);

  useRawInput(
    true,
    (sample: MeterSample) => {
      if (!sample.buttons.length) return;
      for (const b of sample.buttons) {
        const st = stats.current[b.button];
        if (!st) continue;
        if (b.down) {
          st.presses += 1;
          if (st.lastUp !== null) {
            const gap = (b.tUs - st.lastUp) / 1000;
            if (gap < DOUBLE_MS) {
              st.doubles += 1;
              st.worst = Math.max(st.worst, gap);
            }
          }
        } else {
          st.lastUp = b.tUs;
        }
      }
      setView(stats.current.map((x) => ({ ...x })));
    },
  );

  const presses = view.reduce((a, b) => a + b.presses, 0);
  const bad = view.map((v, i) => ({ ...v, name: NAMES[i] })).filter((v) => v.doubles > 0);
  const worst = Math.max(0, ...bad.map((b) => b.worst));
  // The week counted in the background, while you used the PC.
  const weekBad = week?.filter((b) => b.doubles > 0) ?? [];
  const weekWorst = Math.max(0, ...weekBad.map((b) => b.worstMs));
  const w = week && weekText(week);
  const weekLine = !counting
    ? "Background counting is off."
    : !w || !w.presses
      ? "This week: nothing counted yet."
      : `This week: ${w.text}${weekBad.length ? `, up to ${Math.round(weekWorst)} ms apart` : ""}.`;
  const suggest = Math.ceil(Math.max(worst, weekWorst)) + 2;
  const current = settings?.debounceMs ?? 0;

  let verdict: string;
  if (!presses) verdict = "Click each button a few times in the box.";
  else if (!bad.length) verdict = `No double clicks in ${presses} ${presses === 1 ? "press" : "presses"}.`;
  else {
    const who = bad.map((b) => `${b.name} ${b.doubles}×`).join(", ");
    verdict =
      suggest > MAX_DEBOUNCE
        ? `Double clicks: ${who}, up to ${Math.round(worst)} ms apart. Too far apart for debounce to filter, so the switch is likely worn.`
        : suggest <= current
          ? `Double clicks: ${who}, up to ${Math.round(worst)} ms apart, even with debounce at ${current} ms.`
          : `Double clicks: ${who}, up to ${Math.round(worst)} ms apart. Click debounce at ${suggest} ms should filter them.`;
  }
  const canFix = !!settings && online && bad.length + weekBad.length > 0 && suggest > current && suggest <= MAX_DEBOUNCE;
  const doubles = bad.reduce((a, b) => a + b.doubles, 0);
  const result: TestResult | null = !presses
    ? null
    : !bad.length
      ? { value: "No doubles", detail: `In ${presses} ${presses === 1 ? "press" : "presses"}`, tone: "good" }
      : { value: `${doubles} ${doubles === 1 ? "double" : "doubles"}`, detail: `${bad.map((b) => `${b.name} ${b.doubles}×`).join(", ")}, up to ${Math.round(worst)} ms apart`, tone: "warn" };
  useReport(onResult, result);

  const body = (
    <div className={s.body}>
      <div
        className={s.target}
        onContextMenu={(e) => e.preventDefault()}
        onMouseDown={(e) => e.preventDefault()}
        aria-hidden
      >
        <Caption1>Click here</Caption1>
      </div>
      <div className={s.side}>
        <div className={s.table}>
          <Caption1 className={s.head}>Button</Caption1>
          <Caption1 className={mergeClasses(s.head, s.num)}>Presses</Caption1>
          <Caption1 className={mergeClasses(s.head, s.num)}>Doubles</Caption1>
          {view.map((v, i) => (
            <Row key={i} name={NAMES[i]} stat={v} />
          ))}
        </div>
        <Caption1 className={s.verdict} aria-live="polite">
          {verdict}
        </Caption1>
        <Caption1 className={mergeClasses(s.week, weekBad.length > 0 && s.weekWarn)}>{weekLine}</Caption1>
        <div className={s.actions}>
          {canFix && (
            <Button onClick={() => settings && commit({ ...settings, debounceMs: suggest })}>
              Set debounce to {suggest} ms
            </Button>
          )}
          <Link
            as="button"
            disabled={!presses}
            onClick={() => {
              stats.current = fresh();
              setView(fresh());
            }}
          >
            Clear
          </Link>
        </div>
      </div>
    </div>
  );
  return body;
}

function Row({ name, stat }: { name: string; stat: Stat }) {
  const s = useStyles();
  return (
    <>
      <Caption1>{name}</Caption1>
      <Caption1 className={s.num}>{stat.presses}</Caption1>
      {stat.doubles ? (
        <Caption1Strong className={mergeClasses(s.num, s.flagged)}>{stat.doubles}</Caption1Strong>
      ) : (
        <Caption1 className={s.num}>0</Caption1>
      )}
    </>
  );
}
