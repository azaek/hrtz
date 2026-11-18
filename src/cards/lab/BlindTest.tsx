import { useEffect, useState } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import type { CaptureScreen } from "@/api";
import { Choice } from "@/components";
import { t } from "@/i18n";
import { StepCard } from "@/kit";
import { hzText } from "@/rates";
import { commit, toast, useStore } from "@/store";
import { blindSummary, FEWEST, pairsFor, ROUNDS, shareText, useBlindTest, type Pair, type Result, type View } from "./blind";
import { useCapture, useHold } from "./parts";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

/** Game mode's screen for a part of a round: the arena for a run (and before it), else the words. */
function screenFor(v: View): CaptureScreen {
  const of = t("Round {{round}} of {{rounds}}", { round: v.round + 1, rounds: ROUNDS });
  const first = v.run === 0;
  const blank = { title: "", detail: "", figure: "", figureLabel: "", progress: v.round / ROUNDS, progressLabel: "" };
  if (v.phase === "ready" || v.phase === "run") {
    const which =
      v.phase === "ready"
        ? first
          ? t("First run in a moment")
          : t("Second run in a moment")
        : first
          ? t("First run: keep the crosshair on the dot")
          : t("Second run: keep the crosshair on the dot");
    return { ...blank, arena: { running: v.phase === "run", seed: v.seed, label: `${of} · ${which}` } };
  }
  if (v.phase === "answer" || !v.reveal) {
    return { ...blank, title: v.pair.question, detail: t("Left click for the first run, right click for the second."), progressLabel: `${of} · ${t("if you can't tell, go with your gut")}` };
  }
  const r = v.reveal;
  const last = v.round + 1 >= ROUNDS;
  const runs = { asked: v.pair.asked, other: v.pair.other };
  return {
    // "Right" as in correct, apart from the button and the side of the screen.
    title: r.right ? t("Right", { context: "answer" }) : t("Not this time"),
    detail: r.askedFirst ? t("The first run had {{asked}}, the second {{other}}.", runs) : t("The second run had {{asked}}, the first {{other}}.", runs),
    figure: t("{{n}} of {{total}}", { n: r.score, total: r.done }),
    figureLabel: t("right so far"),
    progress: r.done / ROUNDS,
    progressLabel: last ? t("Click for your result") : r.done >= FEWEST ? t("Click for the next round, or Esc to finish here") : t("Click for the next round"),
  };
}

/**
 * Can you feel it? A blind test of two settings of your mouse, on game mode's screen like the
 * Lab's other timing tests (the user: in the app window the browser sets the pace, and Windows hands
 * hrtz only some of the reports): hrtz sets one setting or the other before each run without saying
 * which, you keep a crosshair it draws on a dot it moves, then click for the run that had the asked
 * one. The answer shows after each round. Ten rounds, about two minutes; your settings go back at
 * the end. The score says how likely it was by guessing, and how closely the crosshair followed the
 * dot with each says what you may not have named.
 */
export function BlindTest({ onResult }: TestProps) {
  const device = useStore((x) => x.device);
  const online = !!device?.online;
  const pairs = pairsFor(device?.capabilities, device?.settings);
  const [pick, setPick] = useState<Pair["id"]>(() => pairs[0]?.id ?? "polling");
  const pair = pairs.find((p) => p.id === pick) ?? pairs[0];
  const test = useBlindTest();
  const cap = useCapture();
  // The pointer held, as a game holds it, while the screen is up.
  useHold(test.running);
  const [stopped, setStopped] = useState(false);
  useReport(onResult, test.result ? blindSummary(test.result) : null);

  // The screen follows the test, and goes when it ends.
  useEffect(() => {
    if (test.view) cap.update(screenFor(test.view));
    else cap.close();
  }, [test.view]);
  // Esc, or another window in front, stops it: five rounds or more still make a result.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done" && test.running) {
      setStopped(true);
      test.stop();
    }
  }, [cap.closed]);

  if (!pair) {
    return <StepCard label={t("Blind test")} title={t("Nothing to compare on this mouse")} detail={t("It needs a mouse with two polling rates, or motion sync.")} />;
  }

  const begin = (p: Pair) => {
    setStopped(false);
    setPick(p.id);
    test.start(p);
  };
  // Opened first, so the screen is up before the first run is set.
  const start = (p: Pair) => {
    const label = `${t("Round {{round}} of {{rounds}}", { round: 1, rounds: ROUNDS })} · ${t("First run in a moment")}`;
    cap.start({ title: "", detail: "", figure: "", figureLabel: "", progress: 0, progressLabel: "", arena: { running: false, seed: 0, label } });
    begin(p);
  };
  // Alone on the left of the button row: beside a status line too, it had no room and wrapped
  // (the user's screenshot). The stopped note goes on top instead.
  const choose =
    pairs.length > 1 ? (
      <Choice label={t("What to test")} value={pair.id} options={pairs.map((p) => ({ value: p.id, label: p.name }))} onChange={(id) => setPick(id)} />
    ) : null;

  if (test.running) {
    return (
      <StepCard
        label={test.view ? t("Round {{round}} of {{rounds}}", { round: test.view.round + 1, rounds: ROUNDS }) : t("Blind test")}
        title={pair.id === "polling" ? t("{{asked}} or {{other}}?", { asked: pair.asked, other: pair.other }) : t("Motion sync on or off?")}
        status={<span style={soft}>{t("Running full screen. Press Esc to stop.")}</span>}
        actions={
          <Link as="button" onClick={() => (setStopped(true), test.stop())}>
            {t("Stop")}
          </Link>
        }
      />
    );
  }

  return test.result ? (
    <Outcome r={test.result} again={() => start(pair)} online={online} other={pairs.find((p) => p.id !== pair.id) ?? null} tryOther={start} />
  ) : (
    // Only what to do now: the pane above says what the test is, and its footer what it changes
    // (the user: "it looks too busy on card").
    <StepCard
      label={stopped ? t("Stopped. Five rounds or more make a result.") : t("{{count}} rounds · about 2 minutes", { count: ROUNDS })}
      title={pair.id === "polling" ? t("{{asked}} or {{other}}?", { asked: pair.asked, other: pair.other }) : t("Motion sync on or off?")}
      detail={t("Keep the crosshair on the dot, twice, then click for the run that had {{asked}}.", { asked: pair.asked })}
      status={choose}
      actions={
        <Button appearance="primary" disabled={!online} onClick={() => start(pair)}>
          {t("Start")}
        </Button>
      }
    />
  );
}

/** What the test found: one button (Test again), and the rest as links, so it fits the Lab's pane
 *  (the user's screenshot: the choice, Copy result, Use 1,000 Hz and Test again in one row, each
 *  wrapping). */
function Outcome({ r, again, online, other, tryOther }: { r: Result; again: () => void; online: boolean; other: Pair | null; tryOther: (p: Pair) => void }) {
  const device = useStore((x) => x.device);
  const s = device?.settings;
  const pct = Math.round(Math.abs(r.closer) * 100);
  const title =
    r.right >= 8
      ? t("You can feel {{feel}}", { feel: r.pair.feel })
      : r.right >= 6
        ? t("Hard to say: {{right}} of {{rounds}} right", { right: r.right, rounds: r.rounds })
        : t("You couldn't tell {{asked}} from {{other}}", { asked: r.pair.asked, other: r.pair.other });
  // The chance only says something for more right than half: 2 of 5 is what guessing gets.
  const odds =
    r.right * 2 > r.rounds
      ? r.chance < 0.01
        ? t("Guessing gets that many or more less than 1% of the time.")
        : t("Guessing gets that many or more {{percent}}% of the time.", { percent: Math.round(r.chance * 100) })
      : t("That's about what guessing gets.");
  const tracking =
    r.consistent && pct >= 1 ? t("Followed {{percent}}% closer with {{setting}}", { percent: pct, setting: r.closer > 0 ? r.pair.asked : r.pair.other }) : t("Followed as closely with either");
  const screen =
    r.refreshHz && r.pair.id === "polling"
      ? r.refreshHz <= 75
        ? t("{{hz}} Hz, where a faster polling rate is hard to see", { hz: r.refreshHz })
        : t("{{hz}} Hz", { hz: r.refreshHz })
      : "";
  // Can't feel the faster rate on a wireless mouse: the slower one lasts longer on a charge.
  const low = r.pair.low;
  const saver = r.right <= 7 && low && s?.pollingHz && s.pollingHz > low && !device?.capabilities?.wired && device?.capabilities?.battery;
  const copy = () =>
    void navigator.clipboard.writeText(shareText(r)).then(
      () => toast(t("Result copied")),
      () => {},
    );
  return (
    <StepCard
      label={t("Result")}
      title={title}
      detail={`${t("{{right}} of {{rounds}} right.", { right: r.right, rounds: r.rounds })} ${odds}`}
      status={
        <span className="flex flex-wrap items-center gap-x-4 gap-y-1">
          <Link as="button" onClick={copy}>
            {t("Copy result")}
          </Link>
          {other && (
            <Link as="button" disabled={!online} onClick={() => tryOther(other)}>
              {other.id === "polling" ? t("Test polling rate") : t("Test motion sync")}
            </Link>
          )}
        </span>
      }
      actions={
        <Button appearance="primary" disabled={!online} onClick={again}>
          {t("Test again")}
        </Button>
      }
    >
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        <Fact name={t("The dot")}>{tracking}</Fact>
        {screen && <Fact name={t("Your screen")}>{screen}</Fact>}
        {saver && (
          <Fact name={t("Battery")}>
            {`${t("{{rate}} lasts longer on a charge.", { rate: hzText(low) })} `}
            <Link inline as="button" disabled={!online} onClick={() => s && void commit({ ...s, pollingHz: low })}>
              {t("Use {{rate}}", { rate: hzText(low) })}
            </Link>
          </Fact>
        )}
      </ul>
    </StepCard>
  );
}

/** One finding of a result: what it's about, muted, then what was found. */
function Fact({ name, children }: { name: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <Caption1 className="w-24 shrink-0" style={muted}>
        {name}
      </Caption1>
      <Caption1 style={soft}>{children}</Caption1>
    </li>
  );
}
