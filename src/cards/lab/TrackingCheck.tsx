import { useEffect } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import type { CaptureScreen } from "@/api";
import { StepCard } from "@/kit";
import { useStore } from "@/store";
import { Motion, useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";
import { PARTS, SLOW_SWIPES, SWIPES, mps, speedFindings, speedText, stillFinding, stillTicks, trackingSummary, useTrackingCheck, type Going, type Part, type Swipe } from "./tracking";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };
const tones = { good: { color: "var(--colorPaletteGreenForeground1)" }, warn: { color: "var(--colorPaletteMarigoldForeground1)" }, neutral: soft };

const PART: Record<Part, { title: string; detail: string }> = {
  off: { title: "Let go of the mouse", detail: "Don't touch it. Any movement now comes from the sensor." },
  rest: { title: "Rest your hand on it and keep still", detail: "As you would between fights. Some sensors jitter under a resting hand." },
  swipes: { title: "Swipe between two stops, faster each time", detail: `Book to book and back. Slowly for the first ${SLOW_SWIPES}, then faster each time.` },
};

/** Game mode's screen for a part: what to do, and how far along. */
function screenFor(g: Going, swipes: Swipe[]): CaptureScreen {
  const of = `Part ${PARTS.indexOf(g.part) + 1} of ${PARTS.length}`;
  const { title, detail } = PART[g.part];
  if (g.part === "swipes") {
    const last = swipes[swipes.length - 1];
    return {
      title,
      detail,
      figure: `${swipes.length} of ${SWIPES}`,
      figureLabel: swipes.length < SLOW_SWIPES ? "swipes, slowly" : "swipes, faster each time",
      progress: swipes.length / SWIPES,
      progressLabel: last ? `${of} · last ${mps(last.speed)}` : `${of} · swipes of 5 cm or more count`,
    };
  }
  if (g.phase === "settle") {
    return {
      title,
      detail,
      figure: "–",
      figureLabel: g.restarted ? "Movement detected, restarting" : g.part === "off" ? "Waiting for stillness" : "Waiting for your hand to settle",
      progress: 0,
      progressLabel: of,
    };
  }
  const target = stillTicks(g);
  return { title, detail, figure: String(Math.ceil((target - g.ticks) / 10)), figureLabel: "seconds", progress: g.ticks / target, progressLabel: of };
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
  const dpi = useStore((x) => (x.device?.settings ? (x.device.settings.stages[x.device.settings.dpiStage]?.dpi ?? 1600) : 1600));
  const online = useStore((x) => !!x.device?.online);
  const t = useTrackingCheck(dpi);
  const cap = useCapture();
  // Swipes are timed in game mode, with the pointer held still, as in a game.
  useHold(t.running);
  const m = t.measured;
  useReport(onResult, m.done ? trackingSummary(m, m.swipes, dpi) : null);

  // The screen follows the parts, and goes when the check ends.
  useEffect(() => {
    if (t.going) cap.update(screenFor(t.going, m.swipes));
    else cap.close();
  }, [t.going, m.swipes.length]);
  // Esc, or another window in front, stops the check; what's done is kept.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && t.running) t.stop();
  }, [cap.closed]);

  const start = () => {
    cap.start(screenFor(FIRST, []));
    t.start();
  };

  if (t.going) {
    const { title, detail } = PART[t.going.part];
    return (
      <StepCard
        label={`Part ${PARTS.indexOf(t.going.part) + 1} of ${PARTS.length}`}
        title={title}
        detail={detail}
        art={<Motion kind={t.going.part === "swipes" ? "swipe" : "still"} />}
        status={<span style={soft}>Running full screen. Press Esc to stop.</span>}
        actions={
          <Link as="button" onClick={t.stop}>
            Stop
          </Link>
        }
      />
    );
  }

  if (!m.done) {
    const stopped = cap.closed === "esc" ? "Stopped." : cap.closed === "away" ? "Stopped because another window took focus." : null;
    return (
      <StepCard
        label="About 40 seconds"
        title="Test your sensor's tracking"
        detail="Place two books about 20 cm apart on the pad. You'll let go of the mouse, rest your hand on it, then swipe between the books, faster each time."
        art={<Motion kind="swipe" />}
        status={stopped ? <span style={muted}>{stopped}</span> : undefined}
        actions={
          <Button appearance="primary" disabled={!online} onClick={start}>
            Start
          </Button>
        }
      />
    );
  }

  const off = m.off && stillFinding("off", m.off, dpi);
  const rest = m.rest && stillFinding("rest", m.rest, dpi);
  const n = m.swipes.length;
  const speed = n >= 4 ? speedText(speedFindings(m.swipes), n) : n ? { tone: "neutral" as const, text: `Stopped after ${n} ${n === 1 ? "swipe" : "swipes"}, too few to judge.` } : null;
  const lines: [string, { tone: "good" | "warn" | "neutral"; text: string } | null | undefined][] = [
    ["Hands off", off],
    ["Hand resting", rest],
    ["At speed", speed],
  ];
  return (
    <StepCard label="Results" title={lines.some(([, r]) => r?.tone === "warn") ? "Needs attention" : lines.some(([, r]) => !r) ? "Clean so far" : "Your sensor tracks cleanly"}>
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        {lines.map(([name, r]) => (
          <li key={name} className="flex gap-3">
            <Caption1 className="w-24 shrink-0" style={muted}>
              {name}
            </Caption1>
            <Caption1 style={r ? tones[r.tone] : muted}>{r ? r.text : "Not checked"}</Caption1>
          </li>
        ))}
      </ul>
      <div className="flex min-h-8 items-center justify-end">
        <Button appearance="primary" disabled={!online} onClick={start}>
          Check again
        </Button>
      </div>
    </StepCard>
  );
}
