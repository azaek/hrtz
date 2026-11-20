import { useEffect } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import { currentDpi, type CaptureScreen } from "@/api";
import { t } from "@/i18n";
import { StepCard } from "@/kit";
import { useStore } from "@/store";
import { Motion, useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";
import { PARTS, SLOW_SWIPES, SWIPES, mps, speedFindings, speedText, stillFinding, stillTicks, trackingSummary, useTrackingCheck, type Going, type Part, type Swipe } from "./tracking";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };
const tones = { good: { color: "var(--colorPaletteGreenForeground1)" }, warn: { color: "var(--colorPaletteMarigoldForeground1)" }, neutral: soft };

/** What a part asks for, in the language shown. */
function partText(part: Part): { title: string; detail: string } {
  if (part === "off") return { title: t("Let go of the mouse"), detail: t("Don't touch it. Any movement now comes from the sensor.") };
  if (part === "rest") return { title: t("Rest your hand on it and keep still"), detail: t("As you would between fights. Some sensors jitter under a resting hand.") };
  return { title: t("Swipe between two stops, faster each time"), detail: t("Book to book and back. Slowly for the first {{count}}, then faster each time.", { count: SLOW_SWIPES }) };
}

/** "Part 1 of 3". */
const partOf = (part: Part) => t("Part {{part}} of {{parts}}", { part: PARTS.indexOf(part) + 1, parts: PARTS.length });

/** Game mode's screen for a part: what to do, and how far along. */
function screenFor(g: Going, swipes: Swipe[]): CaptureScreen {
  const of = partOf(g.part);
  const { title, detail } = partText(g.part);
  if (g.part === "swipes") {
    const last = swipes[swipes.length - 1];
    return {
      title,
      detail,
      figure: t("{{n}} of {{total}}", { n: swipes.length, total: SWIPES }),
      figureLabel: swipes.length < SLOW_SWIPES ? t("swipes, slowly") : t("swipes, faster each time"),
      progress: swipes.length / SWIPES,
      progressLabel: last ? `${of} · ${t("last {{speed}}", { speed: mps(last.speed) })}` : `${of} · ${t("swipes of 5 cm or more count")}`,
    };
  }
  if (g.phase === "settle") {
    return {
      title,
      detail,
      figure: "–",
      figureLabel: g.restarted ? t("Movement detected, restarting") : g.part === "off" ? t("Waiting for stillness") : t("Waiting for your hand to settle"),
      progress: 0,
      progressLabel: of,
    };
  }
  const target = stillTicks(g);
  const left = Math.ceil((target - g.ticks) / 10);
  return { title, detail, figure: String(left), figureLabel: t("seconds", { count: left }), progress: g.ticks / target, progressLabel: of };
}

const FIRST: Going = { part: "off", phase: "settle", ticks: 0, travel: 0, x: 0, y: 0, reports: 0, restarted: false };

/**
 * Tracking on your pad, in game mode: Start opens a screen of hrtz's own that takes the pointer as
 * a game does (over hrtz's page Windows hands it about 150 reports a second), and it guides three
 * parts that move on by themselves: let go of the mouse, rest a hand on it, then swipe between two
 * stops, faster each time. A still part with nothing to show is done in 5 seconds. The results come
 * together at the end; Esc keeps what's done. Decided in the diagnostics lab, run like the other
 * game-mode checks since 2026-10-17 (DECISIONS.md, "Lab page's new tests").
 */
export function TrackingCheck({ onResult }: TestProps) {
  const dpi = useStore((x) => currentDpi(x.device?.settings) ?? 1600);
  const online = useStore((x) => !!x.device?.online);
  const check = useTrackingCheck(dpi);
  const cap = useCapture();
  // Swipes are timed in game mode, with the pointer held still, as in a game.
  useHold(check.running);
  const m = check.measured;
  useReport(onResult, m.done ? trackingSummary(m, m.swipes, dpi) : null);

  // The screen follows the parts, and goes when the check ends.
  useEffect(() => {
    if (check.going) cap.update(screenFor(check.going, m.swipes));
    else cap.close();
  }, [check.going, m.swipes.length]);
  // Esc, or another window in front, stops the check; what's done is kept.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && check.running) check.stop();
  }, [cap.closed]);

  const start = () => {
    cap.start(screenFor(FIRST, []));
    check.start();
  };

  if (check.going) {
    const { title, detail } = partText(check.going.part);
    return (
      <StepCard
        label={partOf(check.going.part)}
        title={title}
        detail={detail}
        art={<Motion kind={check.going.part === "swipes" ? "swipe" : "still"} />}
        status={<span style={soft}>{t("Running full screen. Press Esc to stop.")}</span>}
        actions={
          <Link as="button" onClick={check.stop}>
            {t("Stop")}
          </Link>
        }
      />
    );
  }

  if (!m.done) {
    const stopped = cap.closed === "esc" ? t("Stopped.") : cap.closed === "away" ? t("Stopped because another window took focus.") : null;
    return (
      <StepCard
        label={t("About 40 seconds")}
        title={t("Test your sensor's tracking")}
        detail={t("Place two books about 20 cm apart on the pad. You'll let go of the mouse, rest your hand on it, then swipe between the books, faster each time.")}
        art={<Motion kind="swipe" />}
        status={stopped ? <span style={muted}>{stopped}</span> : undefined}
        actions={
          <Button appearance="primary" disabled={!online} onClick={start}>
            {t("Start")}
          </Button>
        }
      />
    );
  }

  const off = m.off && stillFinding("off", m.off, dpi);
  const rest = m.rest && stillFinding("rest", m.rest, dpi);
  const n = m.swipes.length;
  const speed =
    n >= 4
      ? speedText(speedFindings(m.swipes), n)
      : n
        ? { tone: "neutral" as const, text: t("Stopped after {{count}} swipes, too few to judge.", { count: n, defaultValue_one: "Stopped after {{count}} swipe, too few to judge." }) }
        : null;
  const lines: [string, { tone: "good" | "warn" | "neutral"; text: string } | null | undefined][] = [
    [t("Hands off"), off],
    [t("Hand resting"), rest],
    [t("At speed"), speed],
  ];
  return (
    <StepCard label={t("Results")} title={lines.some(([, r]) => r?.tone === "warn") ? t("Needs attention") : lines.some(([, r]) => !r) ? t("Clean so far") : t("Your sensor tracks cleanly")}>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        {lines.map(([name, r]) => (
          <li key={name} className="flex gap-3">
            <Caption1 className="w-24 shrink-0" style={muted}>
              {name}
            </Caption1>
            <Caption1 style={r ? tones[r.tone] : muted}>{r ? r.text : t("Not checked")}</Caption1>
          </li>
        ))}
      </ul>
      <div className="flex min-h-8 items-center justify-end">
        <Button appearance="primary" disabled={!online} onClick={start}>
          {t("Check again")}
        </Button>
      </div>
    </StepCard>
  );
}
