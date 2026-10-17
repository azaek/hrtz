import { useEffect, useState } from "react";
import { Body1Strong, Button, Caption1, Link } from "@fluentui/react-components";
import type { CaptureScreen, ConnStats, MeterSample } from "@/api";
import { StepCard } from "@/kit";
import { useRawInput } from "@/rawInput";
import { commit, useStore } from "@/store";
import { RUN_MS, clearChecks, comparison, fmt, setupText, stepReason, tips, useChecks, useConnCheck, verdict, type ConnRun } from "./connection";
import { GapBars, Motion, RateTable, useCapture, useGames, useHold } from "./parts";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };
const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1);

const HOW = "If your rate isn't stable, lower rates are tried too.";

/** Game mode's screen during a check: the rate being measured, the reports a second coming in once
 *  the pointer is held, and how far along. `before` is the run that made it step down. */
function screenFor(hz: number, waiting: boolean, live: ConnStats | null, rate: number | null, before: ConnRun | null): CaptureScreen {
  const ms = live?.ms ?? 0;
  return {
    title: waiting && before ? `Setting ${fmt(hz)} Hz…` : "Move the mouse in quick circles",
    detail: before ? `${fmt(before.setup.hz)} Hz ${stepReason(before)}, so now ${fmt(hz)} Hz. Keep going.` : `Only fast movement counts. ${HOW}`,
    figure: !waiting && rate ? fmt(rate) : "–",
    figureLabel: waiting ? "" : rate ? `reports a second at ${fmt(hz)} Hz` : "Start moving",
    progress: waiting ? 0 : ms / RUN_MS,
    progressLabel: `${fmt(hz)} Hz · ${(ms / 1000).toFixed(1)} of ${RUN_MS / 1000} s counted`,
  };
}

/**
 * The connection check, in game mode: Start opens a screen of hrtz's own that takes the pointer as
 * a game does (over hrtz's page Windows hands it about 150 reports a second), and 10 seconds of
 * quick circles measure your polling rate. If it doesn't come through clean, hrtz steps down a rate
 * by itself while that helps, puts yours back, and offers the one that ran best. What only you can
 * change (the receiver's place) follows when it still stutters, and the next check compares by
 * itself. Decided in the diagnostics lab, made smarter after the user's first runs (DECISIONS.md,
 * "Lab page's new tests").
 */
export function ConnectionCheck({ onResult }: TestProps) {
  const device = useStore((x) => x.device!);
  const c = useConnCheck();
  useHold(true);
  const cap = useCapture();
  const checks = useChecks();
  const games = useGames();
  const [skipped, setSkipped] = useState<string[]>([]);
  // Reports a second while the pointer is held, for the screen.
  const [rate, setRate] = useState<number | null>(null);
  useRawInput(true, (s: MeterSample) => setRate(s.held && s.hz ? s.hz : null));
  const latest = checks[checks.length - 1] ?? null;
  const prev = checks[checks.length - 2] ?? null;
  const v = latest ? verdict(latest) : null;
  useReport(onResult, v ? { value: v.value, detail: v.note, tone: v.tone } : null);
  const before = c.tried[c.tried.length - 1] ?? null;

  // A new check brings its own things to try.
  useEffect(() => setSkipped([]), [checks.length]);
  // The screen follows the check, and closes when it has its answer.
  useEffect(() => {
    if (c.hz) cap.update(screenFor(c.hz, c.waiting, c.live, rate, before));
    else cap.close();
  }, [c.hz, c.waiting, c.live, rate]);
  // Esc, or another window in front, stops the check, and your rate goes back.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && c.running) c.stop();
  }, [cap.closed]);

  const start = () => {
    const hz = Math.min(device.settings?.pollingHz ?? 1000, device.capabilities?.maxPollingHz ?? 8000);
    cap.start(screenFor(hz, false, null, null, null));
    c.start();
  };
  const stopped = cap.closed === "esc" ? "Stopped. Your polling rate is unchanged." : cap.closed === "away" ? "Stopped because another window took focus. Your polling rate is unchanged." : null;

  if (c.hz)
    return (
      <StepCard
        label={before ? `${fmt(before.setup.hz)} Hz ${stepReason(before)}` : `Checking ${fmt(c.hz)} Hz`}
        title={c.waiting && before ? `Setting ${fmt(c.hz)} Hz…` : "Move the mouse in quick circles"}
        detail={HOW}
        art={<Motion kind="circles" />}
        status={<span style={soft}>Running full screen. Press Esc to stop.</span>}
        actions={
          <Link as="button" onClick={c.stop}>
            Stop
          </Link>
        }
      />
    );

  if (!latest || !v)
    return (
      <StepCard
        label="About 10 seconds"
        title="Move the mouse in quick circles"
        detail={`Measured the way a game reads the mouse. ${HOW}`}
        art={<Motion kind="circles" />}
        status={stopped ? <span style={muted}>{stopped}</span> : undefined}
        actions={
          <Button appearance="primary" disabled={!device.online} onClick={start}>
            Start
          </Button>
        }
      />
    );

  const first = latest.runs[0];
  const next = tips(latest, games).filter((t) => !skipped.includes(t.key))[0];
  const switchable = !!device.settings && v.hz !== device.settings.pollingHz;

  return (
    <div className="tw flex flex-col gap-4">
      <StepCard label={`Check ${latest.n} · ${setupText(first.setup)}`} title={v.title} detail={v.detail}>
        {latest.runs.length > 1 ? <RateTable runs={latest.runs} chosen={v.hz} /> : <GapBars stats={first.stats} />}
        {prev && (
          <Caption1 style={soft}>
            Against check {prev.n}: {lower(comparison(prev.runs[0], first))}
          </Caption1>
        )}
        {next && (
          <div className="flex flex-col gap-2 border-t pt-3">
            <Body1Strong>{next.game ? "Good to know" : "Next, try this"}</Body1Strong>
            <Caption1 style={soft}>{next.text}</Caption1>
            <div>
              <Link as="button" onClick={() => setSkipped((s) => [...s, next.key])}>
                Show another tip
              </Link>
            </div>
          </div>
        )}
        <div className="flex min-h-8 items-center justify-between gap-3 border-t pt-3">
          <Caption1 style={muted}>{stopped ?? ""}</Caption1>
          <div className="flex items-center gap-3">
            {switchable && (
              <Button appearance="primary" disabled={!device.online} onClick={() => void commit({ ...device.settings!, pollingHz: v.hz })}>
                Use {fmt(v.hz)} Hz
              </Button>
            )}
            <Button appearance={switchable ? "secondary" : "primary"} disabled={!device.online} onClick={start}>
              Check again
            </Button>
          </div>
        </div>
      </StepCard>
      <div className="flex items-baseline justify-between gap-3">
        <Caption1 style={muted}>
          {checks.length > 1
            ? `Earlier: ${checks
                .slice(0, -1)
                .map((k) => `check ${k.n}, ${setupText(k.runs[0].setup)}: ${verdict(k).value}`)
                .join("; ")}.`
            : ""}
        </Caption1>
        <Link as="button" onClick={clearChecks}>
          Start over
        </Link>
      </div>
    </div>
  );
}
