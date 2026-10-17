import { useEffect } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import type { CaptureScreen, Lod } from "@/api";
import { StepCard } from "@/kit";
import { useStore } from "@/store";
import { LOD_NAME, RETURNS, useLiftOff } from "./liftoff";
import { LodTable, Motion, useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

const SLIDE = "Slide slowly about 10 cm to the right, then lift the mouse, carry it back and set it down. Repeat.";

/** Game mode's screen for a setting: what to do, and how far along. `before` is the setting that
 *  lost track, when this isn't the first. */
function screenFor(lod: Lod, ready: boolean, count: number, before: Lod | null): CaptureScreen {
  const name = LOD_NAME[lod];
  return {
    title: ready ? "Slide, lift and carry back" : `Setting ${name}…`,
    detail: SLIDE,
    figure: ready ? `${count} of ${RETURNS}` : "–",
    figureLabel: ready ? `returns at ${name}` : "",
    progress: ready ? count / RETURNS : 0,
    progressLabel: before ? `${LOD_NAME[before]} lost track on your pad, so ${name} now` : `Starting at ${name}, the lowest`,
  };
}

/**
 * Lift-off on your pad, in game mode: hrtz sets the lowest lift-off, and on a screen of its own
 * that takes the pointer as a game does (over hrtz's page Windows hands it about 150 reports a
 * second), you slide, lift and carry the mouse back ten times. If the sensor loses track on the
 * slides, hrtz moves up a setting and you go again; the first that holds suits the pad. Your own
 * setting goes back at the end, on Esc, or on leaving the test, and the one that suits the pad is
 * a button away. Decided in the diagnostics lab, made smarter after the user's first runs
 * (DECISIONS.md, "Lab page's new tests").
 */
export function LiftOffCheck({ onResult }: TestProps) {
  const t = useLiftOff();
  const online = useStore((x) => !!x.device?.online);
  const cap = useCapture();
  useHold(t.running);
  useReport(onResult, t.verdict ? { value: t.verdict.title, detail: t.verdict.note, tone: t.verdict.held ? "good" : "warn" } : null);
  const before = t.lod && t.index > 0 ? t.lods[t.index - 1] : null;

  // The screen follows the settings, and goes when the check ends.
  useEffect(() => {
    if (t.lod) cap.update(screenFor(t.lod, t.ready, t.count, before));
    else cap.close();
  }, [t.lod, t.ready, t.count]);
  // Esc, or another window in front, stops the check, and your setting goes back.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && t.running) t.stop();
  }, [cap.closed]);

  const start = () => {
    cap.start(screenFor(t.lods[0], false, 0, null));
    t.start();
  };

  if (t.lod)
    return (
      <StepCard
        label={before ? `${LOD_NAME[before]} lost track · trying ${LOD_NAME[t.lod]}` : `Trying ${LOD_NAME[t.lod]}, the lowest`}
        title={t.ready ? "Slide, lift and carry back" : `Setting ${LOD_NAME[t.lod]}…`}
        detail={SLIDE}
        art={<Motion kind="return" />}
        status={<span style={soft}>Running full screen. Press Esc to stop.</span>}
        actions={
          <Link as="button" onClick={t.stop}>
            Stop
          </Link>
        }
      />
    );
  if (t.verdict)
    return (
      <StepCard label="Results" title={t.verdict.title} detail={t.verdict.why}>
        <LodTable tried={t.tried} dpi={t.dpi} chosen={t.verdict.lod} />
        <div className="flex min-h-8 items-center gap-3">
          {t.current !== t.verdict.lod ? (
            <Button appearance="primary" disabled={!online} onClick={() => t.use(t.verdict!.lod)}>
              Use {LOD_NAME[t.verdict.lod]}
            </Button>
          ) : (
            <Caption1 style={muted}>In use</Caption1>
          )}
          <Link as="button" disabled={!online} onClick={start}>
            Start over
          </Link>
        </div>
      </StepCard>
    );
  const stopped = cap.closed === "esc" ? "Stopped. Your lift-off is restored." : cap.closed === "away" ? "Stopped because another window took focus. Your lift-off is restored." : null;
  return (
    <StepCard
      label="About 20 seconds"
      title="Find the lift-off that suits your pad"
      detail={`Starts at ${LOD_NAME[t.lods[0]]}. Slide, lift and carry the mouse back ${RETURNS} times. If the sensor loses track, the next setting is tried.`}
      art={<Motion kind="return" />}
      status={stopped ? <span style={muted}>{stopped}</span> : undefined}
      actions={
        <Button appearance="primary" disabled={!online} onClick={start}>
          Start
        </Button>
      }
    />
  );
}
