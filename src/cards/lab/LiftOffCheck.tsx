import { useEffect } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import { lodName, type CaptureScreen, type Lod } from "@/api";
import { t } from "@/i18n";
import { StepCard } from "@/kit";
import { useStore } from "@/store";
import { RETURNS, useLiftOff } from "./liftoff";
import { LodTable, Motion, useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

const slide = () => t("Slide slowly about 10 cm to the right, then lift the mouse, carry it back and set it down. Repeat.");

/** Game mode's screen for a setting: what to do, and how far along. `before` is the setting that
 *  lost track, when this isn't the first. */
function screenFor(lod: Lod, ready: boolean, count: number, before: Lod | null): CaptureScreen {
  const name = lodName(lod);
  return {
    title: ready ? t("Slide, lift and carry back") : t("Setting {{lod}}…", { lod: name }),
    detail: slide(),
    figure: ready ? t("{{n}} of {{total}}", { n: count, total: RETURNS }) : "–",
    figureLabel: ready ? t("returns at {{lod}}", { lod: name }) : "",
    progress: ready ? count / RETURNS : 0,
    progressLabel: before ? t("{{before}} lost track on your pad, so {{lod}} now", { before: lodName(before), lod: name }) : t("Starting at {{lod}}, the lowest", { lod: name }),
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
  const check = useLiftOff();
  const online = useStore((x) => !!x.device?.online);
  const cap = useCapture();
  useHold(check.running);
  useReport(onResult, check.verdict ? { value: check.verdict.title, detail: check.verdict.note, tone: check.verdict.held ? "good" : "warn" } : null);
  const before = check.lod && check.index > 0 ? check.lods[check.index - 1] : null;

  // The screen follows the settings, and goes when the check ends.
  useEffect(() => {
    if (check.lod) cap.update(screenFor(check.lod, check.ready, check.count, before));
    else cap.close();
  }, [check.lod, check.ready, check.count]);
  // Esc, or another window in front, stops the check, and your setting goes back.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && check.running) check.stop();
  }, [cap.closed]);

  const start = () => {
    cap.start(screenFor(check.lods[0], false, 0, null));
    check.start();
  };

  if (check.lod)
    return (
      <StepCard
        label={before ? t("{{before}} lost track · trying {{lod}}", { before: lodName(before), lod: lodName(check.lod) }) : t("Trying {{lod}}, the lowest", { lod: lodName(check.lod) })}
        title={check.ready ? t("Slide, lift and carry back") : t("Setting {{lod}}…", { lod: lodName(check.lod) })}
        detail={slide()}
        art={<Motion kind="return" />}
        status={<span style={soft}>{t("Running full screen. Press Esc to stop.")}</span>}
        actions={
          <Link as="button" onClick={check.stop}>
            {t("Stop")}
          </Link>
        }
      />
    );
  if (check.verdict)
    return (
      <StepCard label={t("Results")} title={check.verdict.title} detail={check.verdict.why}>
        <LodTable tried={check.tried} dpi={check.dpi} chosen={check.verdict.lod} />
        <div className="flex min-h-8 items-center gap-3">
          {check.current !== check.verdict.lod ? (
            <Button appearance="primary" disabled={!online} onClick={() => check.use(check.verdict!.lod)}>
              {t("Use {{lod}}", { lod: lodName(check.verdict.lod) })}
            </Button>
          ) : (
            <Caption1 style={muted}>{t("In use")}</Caption1>
          )}
          <Link as="button" disabled={!online} onClick={start}>
            {t("Start over")}
          </Link>
        </div>
      </StepCard>
    );
  const stopped =
    cap.closed === "esc" ? t("Stopped. Your lift-off is restored.") : cap.closed === "away" ? t("Stopped because another window took focus. Your lift-off is restored.") : null;
  return (
    <StepCard
      label={t("About 20 seconds")}
      title={t("Find the lift-off that suits your pad")}
      detail={t("Starts at {{lod}}. Slide, lift and carry the mouse back {{count}} times. If the sensor loses track, the next setting is tried.", { lod: lodName(check.lods[0]), count: RETURNS })}
      art={<Motion kind="return" />}
      status={stopped ? <span style={muted}>{stopped}</span> : undefined}
      actions={
        <Button appearance="primary" disabled={!online} onClick={start}>
          {t("Start")}
        </Button>
      }
    />
  );
}
