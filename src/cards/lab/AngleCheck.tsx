import { useEffect, useState } from "react";
import { Button, Link } from "@fluentui/react-components";
import { currentDpi, type CaptureScreen, type SensorAngle } from "@/api";
import { StepCard } from "@/kit";
import { commit, useStore } from "@/store";
import { SWIPES, SWIPE_CM, angleSummary, angleText, inEffect, useAngleCheck, verdict } from "./angle";
import { Motion, useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };
const warn = { color: "var(--colorPaletteMarigoldForeground1)" };

/** Game mode's screen: what to do, and how many swipes are in. */
const screenFor = (n: number): CaptureScreen => ({
  title: "Swipe side to side",
  detail: "Hold the mouse as you do in a game, and swipe the way that feels level to you.",
  figure: `${n} of ${SWIPES}`,
  figureLabel: "swipes",
  progress: n / SWIPES,
  progressLabel: `Sideways swipes of ${SWIPE_CM} cm or more count`,
});

// The angle a mouse had before the last one set here, for Undo. Kept for the session.
let before: { mouse: string | null; angle: SensorAngle } | null = null;

/**
 * The sensor angle for your grip, in game mode: on a screen of hrtz's own that takes the pointer
 * as a game does, you swipe side to side the way that feels level, and hrtz measures how far the
 * swipes tilt. On a mouse with a sensor angle, the angle that levels them is a button away, and
 * nothing changes until it's pressed; on any other, the tilt is said and that the mouse can't
 * level it. Run like the Lab's other game-mode checks (DECISIONS.md, "Sensor angle test").
 */
export function AngleCheck({ onResult }: TestProps) {
  const dpi = useStore((x) => currentDpi(x.device?.settings) ?? 1600);
  const mouse = useStore((x) => x.device?.key ?? null);
  const online = useStore((x) => !!x.device?.online);
  const settings = useStore((x) => x.device?.settings);
  const max = useStore((x) => x.device?.capabilities?.sensorAngleDeg);
  // The mouse's own sensor angle, where it has one and it could be read.
  const held = max !== undefined ? settings?.sensorAngle : undefined;
  const now = inEffect(held);
  const t = useAngleCheck(dpi);
  const cap = useCapture();
  useHold(t.running);
  const m = t.measured;
  // A reading made for another mouse isn't this one's.
  const v = m.mouse === mouse ? verdict(m, held ? max : undefined, now) : null;
  // Undo is offered for the angle set from this result, until the next run.
  const [undo, setUndo] = useState(false);
  useReport(onResult, v ? angleSummary(v, m.applied) : null);

  // The screen follows the swipes, and goes when the check ends.
  useEffect(() => {
    if (t.running) cap.update(screenFor(m.swipes.length));
    else cap.close();
  }, [t.running, m.swipes.length]);
  // Esc, or another window in front, stops the check.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && t.running) t.stop();
  }, [cap.closed]);

  const start = () => {
    setUndo(false);
    cap.start(screenFor(0));
    t.start(now, mouse);
  };

  if (t.running) {
    return (
      <StepCard
        label={`${m.swipes.length} of ${SWIPES} swipes`}
        title="Swipe side to side"
        detail="Hold the mouse as you do in a game, and swipe the way that feels level to you."
        art={<Motion kind="sweep" />}
        status={<span style={soft}>Running full screen. Press Esc to stop.</span>}
        actions={
          <Link as="button" onClick={t.stop}>
            Stop
          </Link>
        }
      />
    );
  }

  if (!v) {
    const stopped = cap.closed === "esc" ? "Stopped." : cap.closed === "away" ? "Stopped because another window took focus." : null;
    return (
      <StepCard
        label="About 20 seconds"
        title="Find the sensor angle for your grip"
        detail={`Hold the mouse as you do in a game and swipe side to side ${SWIPES} times, the way that feels level to you. hrtz measures how far your swipes tilt.`}
        art={<Motion kind="sweep" />}
        status={stopped ? <span style={muted}>{stopped}</span> : undefined}
        actions={
          <Button appearance="primary" disabled={!online} onClick={start}>
            Start
          </Button>
        }
      />
    );
  }

  const set = v.set;
  const can = set !== null && !!settings && !!held;
  const by = Math.abs(v.turn);
  const slope = `Swiping right, the pointer ${v.tilt > 0 ? "drops" : "climbs"} ${angleText(by)}.`;
  const at = m.applied ? ` Measured with the sensor angle at ${angleText(m.applied)}.` : "";
  const title = v.level
    ? m.applied
      ? "Your sensor angle suits your grip"
      : "Your swipes run level"
    : can
      ? v.isSet
        ? `Sensor angle set to ${angleText(set)}`
        : `Your grip needs a sensor angle of ${angleText(set)}`
      : `Your swipes tilt ${angleText(by)}`;
  const detail = v.level
    ? m.applied
      ? `Your swipes ran level with the sensor angle at ${angleText(m.applied)}.`
      : "Your grip needs no sensor angle."
    : can
      ? v.isSet
        ? `It levels swipes that tilted ${angleText(by)}.`
        : `${slope}${at}${set !== v.want ? ` It would take ${angleText(v.want)}, past what this mouse has.` : ""}`
      : `${slope} This mouse has no sensor angle setting to level it.`;
  const apply = () => {
    if (!can) return;
    before = { mouse, angle: held };
    setUndo(true);
    void commit({ ...settings, sensorAngle: { on: true, degrees: set } });
  };
  const putBack = () => {
    if (!settings || before?.mouse !== mouse) return;
    setUndo(false);
    void commit({ ...settings, sensorAngle: before.angle });
  };
  const again = (
    <Button appearance={can && !v.level && !v.isSet ? "secondary" : "primary"} disabled={!online} onClick={start}>
      Check again
    </Button>
  );

  return (
    <StepCard
      label="Result"
      title={title}
      detail={detail}
      status={
        can && v.isSet && undo && before?.mouse === mouse ? (
          <span style={soft}>
            Check again to see that your swipes run level.{" "}
            <Link as="button" onClick={putBack}>
              Undo
            </Link>
          </span>
        ) : v.rough ? (
          <span style={warn}>Your swipes varied by ±{Math.round(v.spread)}°, so this is a rough reading.</span>
        ) : undefined
      }
      actions={
        <>
          {again}
          {can && !v.level && !v.isSet && (
            <Button appearance="primary" disabled={!online} onClick={apply}>
              Set to {angleText(set)}
            </Button>
          )}
        </>
      }
    />
  );
}
