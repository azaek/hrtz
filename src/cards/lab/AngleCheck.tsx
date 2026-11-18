import { useEffect, useState } from "react";
import { Button, Link } from "@fluentui/react-components";
import { currentDpi, type CaptureScreen, type SensorAngle } from "@/api";
import { t } from "@/i18n";
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
  title: t("Swipe side to side"),
  detail: t("Hold the mouse as you do in a game, and swipe the way that feels level to you."),
  figure: t("{{n}} of {{total}}", { n, total: SWIPES }),
  figureLabel: t("swipes"),
  progress: n / SWIPES,
  progressLabel: t("Sideways swipes of {{cm}} cm or more count", { cm: SWIPE_CM }),
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
  const check = useAngleCheck(dpi);
  const cap = useCapture();
  useHold(check.running);
  const m = check.measured;
  // A reading made for another mouse isn't this one's.
  const v = m.mouse === mouse ? verdict(m, held ? max : undefined, now) : null;
  // Undo is offered for the angle set from this result, until the next run.
  const [undo, setUndo] = useState(false);
  useReport(onResult, v ? angleSummary(v, m.applied) : null);

  // The screen follows the swipes, and goes when the check ends.
  useEffect(() => {
    if (check.running) cap.update(screenFor(m.swipes.length));
    else cap.close();
  }, [check.running, m.swipes.length]);
  // Esc, or another window in front, stops the check.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && check.running) check.stop();
  }, [cap.closed]);

  const start = () => {
    setUndo(false);
    cap.start(screenFor(0));
    check.start(now, mouse);
  };

  if (check.running) {
    return (
      <StepCard
        label={t("{{n}} of {{total}} swipes", { n: m.swipes.length, total: SWIPES })}
        title={t("Swipe side to side")}
        detail={t("Hold the mouse as you do in a game, and swipe the way that feels level to you.")}
        art={<Motion kind="sweep" />}
        status={<span style={soft}>{t("Running full screen. Press Esc to stop.")}</span>}
        actions={
          <Link as="button" onClick={check.stop}>
            {t("Stop")}
          </Link>
        }
      />
    );
  }

  if (!v) {
    const stopped = cap.closed === "esc" ? t("Stopped.") : cap.closed === "away" ? t("Stopped because another window took focus.") : null;
    return (
      <StepCard
        label={t("About 20 seconds")}
        title={t("Find the sensor angle for your grip")}
        detail={t("Hold the mouse as you do in a game and swipe side to side {{count}} times, the way that feels level to you. hrtz measures how far your swipes tilt.", { count: SWIPES })}
        art={<Motion kind="sweep" />}
        status={stopped ? <span style={muted}>{stopped}</span> : undefined}
        actions={
          <Button appearance="primary" disabled={!online} onClick={start}>
            {t("Start")}
          </Button>
        }
      />
    );
  }

  const set = v.set;
  const can = set !== null && !!settings && !!held;
  const by = Math.abs(v.turn);
  const slope = v.tilt > 0 ? t("Swiping right, the pointer drops {{angle}}.", { angle: angleText(by) }) : t("Swiping right, the pointer climbs {{angle}}.", { angle: angleText(by) });
  const at = m.applied ? ` ${t("Measured with the sensor angle at {{angle}}.", { angle: angleText(m.applied) })}` : "";
  const title = v.level
    ? m.applied
      ? t("Your sensor angle suits your grip")
      : t("Your swipes run level")
    : can
      ? v.isSet
        ? t("Sensor angle set to {{angle}}", { angle: angleText(set) })
        : t("Your grip needs a sensor angle of {{angle}}", { angle: angleText(set) })
      : t("Your swipes tilt {{angle}}", { angle: angleText(by) });
  const detail = v.level
    ? m.applied
      ? t("Your swipes ran level with the sensor angle at {{angle}}.", { angle: angleText(m.applied) })
      : t("Your grip needs no sensor angle.")
    : can
      ? v.isSet
        ? t("It levels swipes that tilted {{angle}}.", { angle: angleText(by) })
        : `${slope}${at}${set !== v.want ? ` ${t("It would take {{angle}}, past what this mouse has.", { angle: angleText(v.want) })}` : ""}`
      : `${slope} ${t("This mouse has no sensor angle setting to level it.")}`;
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
      {t("Check again")}
    </Button>
  );

  return (
    <StepCard
      label={t("Result")}
      title={title}
      detail={detail}
      status={
        can && v.isSet && undo && before?.mouse === mouse ? (
          <span style={soft}>
            {t("Check again to see that your swipes run level.")}{" "}
            <Link as="button" onClick={putBack}>
              {t("Undo")}
            </Link>
          </span>
        ) : v.rough ? (
          <span style={warn}>{t("Your swipes varied by ±{{degrees}}°, so this is a rough reading.", { degrees: Math.round(v.spread) })}</span>
        ) : undefined
      }
      actions={
        <>
          {again}
          {can && !v.level && !v.isSet && (
            <Button appearance="primary" disabled={!online} onClick={apply}>
              {t("Set to {{angle}}", { angle: angleText(set) })}
            </Button>
          )}
        </>
      }
    />
  );
}
