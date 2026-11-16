import { useEffect, useRef, useState } from "react";
import { Body1, Button, Caption1, Link, Portal, Title3 } from "@fluentui/react-components";
import { Choice } from "@/components";
import { StepCard } from "@/kit";
import { hzText } from "@/rates";
import { commit, toast, useStore } from "@/store";
import { blindSummary, dotAt, FEWEST, pairsFor, READY_MS, ROUNDS, shareText, useBlindTest, type Pair, type Phase, type Result, type Reveal } from "./blind";
import { useReport, type TestProps } from "./result";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

/**
 * Can you feel it? A blind test of two settings of your mouse: hrtz sets one or the other before
 * each run without saying which, you follow a dot with the pointer on a screen that covers the
 * window, then say which run was which. Ten rounds, about two minutes; your settings go back at the
 * end. The score says how likely it was by guessing, and how closely you followed the dot with
 * each says what you may not have named.
 */
export function BlindTest({ onResult }: TestProps) {
  const device = useStore((x) => x.device);
  const online = !!device?.online;
  const pairs = pairsFor(device?.capabilities, device?.settings);
  const [pick, setPick] = useState<Pair["id"]>(() => pairs[0]?.id ?? "polling");
  const pair = pairs.find((p) => p.id === pick) ?? pairs[0];
  const t = useBlindTest();
  const [stopped, setStopped] = useState(false);
  useReport(onResult, t.result ? blindSummary(t.result) : null);

  if (!pair) {
    return <StepCard label="Blind test" title="Nothing to compare on this mouse" detail="It needs a mouse with two polling rates, or motion sync." />;
  }

  const start = () => {
    setStopped(false);
    t.start(pair);
  };
  // Alone on the left of the button row: beside a status line too, it had no room and wrapped
  // (the user's screenshot). The stopped note goes on top instead.
  const choose =
    pairs.length > 1 ? (
      <Choice label="What to test" value={pair.id} options={pairs.map((p) => ({ value: p.id, label: p.name }))} onChange={(id) => setPick(id)} />
    ) : null;

  return (
    <>
      {t.view && (
        <Arena
          view={t.view}
          onSample={t.sample}
          onAnswer={t.answer}
          onNext={t.next}
          onStop={() => {
            setStopped(true);
            t.stop();
          }}
        />
      )}
      {t.result && !t.running ? (
        <Outcome r={t.result} again={start} online={online} other={pairs.find((p) => p.id !== pair.id) ?? null} tryOther={(p) => (setPick(p.id), setStopped(false), t.start(p))} />
      ) : (
        // Only what to do now: the pane above says what the test is, and its footer what it
        // changes (the user: "it looks too busy on card").
        <StepCard
          label={stopped ? "Stopped. Five rounds or more make a result." : `${ROUNDS} rounds · about 2 minutes`}
          title={pair.id === "polling" ? `${pair.asked} or ${pair.other}?` : "Motion sync on or off?"}
          detail={`Follow the dot twice, then pick the run that had ${pair.asked}.`}
          status={choose}
          actions={
            <Button appearance="primary" disabled={!online || t.running} onClick={start}>
              Start
            </Button>
          }
        />
      )}
    </>
  );
}

/** What the test found, with the next step. */
/** What the test found: one button (Test again), and the rest as links, so it fits the Lab's pane
 *  (the user's screenshot: the choice, Copy result, Use 1,000 Hz and Test again in one row, each
 *  wrapping). */
function Outcome({ r, again, online, other, tryOther }: { r: Result; again: () => void; online: boolean; other: Pair | null; tryOther: (p: Pair) => void }) {
  const device = useStore((x) => x.device);
  const s = device?.settings;
  const pct = Math.round(Math.abs(r.closer) * 100);
  const title =
    r.right >= 8 ? `You can feel ${r.pair.feel}` : r.right >= 6 ? `Hard to say: ${r.right} of ${r.rounds} right` : `You couldn't tell ${r.pair.asked} from ${r.pair.other}`;
  // The chance only says something for more right than half: 2 of 5 is what guessing gets.
  const odds =
    r.right * 2 > r.rounds
      ? `Guessing gets that many or more ${r.chance < 0.01 ? "less than 1%" : `${Math.round(r.chance * 100)}%`} of the time.`
      : "That's about what guessing gets.";
  const tracking = r.consistent && pct >= 1 ? `Followed ${pct}% closer with ${r.closer > 0 ? r.pair.asked : r.pair.other}` : "Followed as closely with either";
  const screen =
    r.refreshHz && r.pair.id === "polling" ? (r.refreshHz <= 75 ? `${r.refreshHz} Hz, where a faster polling rate is hard to see` : `${r.refreshHz} Hz`) : "";
  // Can't feel the faster rate on a wireless mouse: the slower one lasts longer on a charge.
  const low = r.pair.low;
  const saver = r.right <= 7 && low && s?.pollingHz && s.pollingHz > low && !device?.capabilities?.wired && device?.capabilities?.battery;
  const copy = () =>
    void navigator.clipboard.writeText(shareText(r)).then(
      () => toast("Result copied"),
      () => {},
    );
  return (
    <StepCard
      label="Result"
      title={title}
      detail={`${r.right} of ${r.rounds} right. ${odds}`}
      status={
        <span className="flex flex-wrap items-center gap-x-4 gap-y-1">
          <Link as="button" onClick={copy}>
            Copy result
          </Link>
          {other && (
            <Link as="button" disabled={!online} onClick={() => tryOther(other)}>
              Test {other.name.toLowerCase()}
            </Link>
          )}
        </span>
      }
      actions={
        <Button appearance="primary" disabled={!online} onClick={again}>
          Test again
        </Button>
      }
    >
      <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
        <Fact name="The dot">{tracking}</Fact>
        {screen && <Fact name="Your screen">{screen}</Fact>}
        {saver && (
          <Fact name="Battery">
            {`${hzText(low)} lasts longer on a charge. `}
            <Link inline as="button" disabled={!online} onClick={() => s && void commit({ ...s, pollingHz: low })}>
              Use {hzText(low)}
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

/** The test's own screen over the whole window: the round, the dot to follow, then the question. */
function Arena({
  view,
  onSample,
  onAnswer,
  onNext,
  onStop,
}: {
  view: { round: number; run: 0 | 1; phase: Phase; seed: number; pair: Pair; reveal: Reveal | null };
  onSample: (miss: number, frameMs: number) => void;
  onAnswer: (first: boolean) => void;
  onNext: () => void;
  onStop: () => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const dot = useRef<HTMLDivElement>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const handlers = useRef({ onSample, onStop });
  handlers.current = { onSample, onStop };

  const handlersNext = useRef(onNext);
  handlersNext.current = onNext;
  const revealing = view.phase === "reveal";
  useEffect(() => {
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") handlers.current.onStop();
      // Enter or Space for the next round, once the answer is up.
      else if (revealing && (e.key === "Enter" || e.key === " ")) {
        e.preventDefault();
        handlersNext.current();
      }
    };
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, [revealing]);

  // The dot moves every frame of a run; before one, it waits where the run will start.
  useEffect(() => {
    const el = box.current;
    if (!el || view.phase === "answer" || view.phase === "reveal") return;
    const start = performance.now();
    let last = start;
    let frame = 0;
    const draw = (now: number) => {
      const { width: w, height: h } = el.getBoundingClientRect();
      const t = view.phase === "run" ? now - start : 0;
      const p = dotAt(t, view.seed, w, h);
      if (dot.current) dot.current.style.transform = `translate(${p.x - 14}px, ${p.y - 14}px)`;
      const at = pointer.current;
      if (view.phase === "run" && at) handlers.current.onSample(Math.hypot(at.x - p.x, at.y - p.y), now - last);
      last = now;
      frame = requestAnimationFrame(draw);
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, [view.phase, view.seed, view.run]);

  const where = `Round ${view.round + 1} of ${ROUNDS}`;
  const line =
    view.phase === "answer" || view.phase === "reveal"
      ? "Both runs done"
      : view.phase === "ready"
        ? `${view.run === 0 ? "First" : "Second"} run in a moment`
        : `${view.run === 0 ? "First" : "Second"} run: follow the dot`;

  return (
    <Portal>
      <div
        ref={box}
        className="tw"
        onPointerMove={(e) => {
          const r = box.current?.getBoundingClientRect();
          if (r) pointer.current = { x: e.clientX - r.left, y: e.clientY - r.top };
        }}
        style={{ position: "fixed", inset: 0, zIndex: 1_000_000, background: "var(--colorNeutralBackground1)", userSelect: "none" }}
      >
        <div className="flex items-start justify-between" style={{ position: "absolute", top: 16, left: 20, right: 20 }}>
          <div className="flex flex-col gap-0.5">
            <Caption1 style={muted}>{where}</Caption1>
            <Body1>{line}</Body1>
          </div>
          <Caption1 style={muted}>Esc to stop</Caption1>
        </div>
        {(view.phase === "ready" || view.phase === "run") && (
          <div
            ref={dot}
            aria-hidden
            style={{
              position: "absolute",
              left: 0,
              top: 0,
              width: 28,
              height: 28,
              borderRadius: 14,
              border: "2px solid var(--colorNeutralForeground2)",
              opacity: view.phase === "ready" ? 0.35 : 1,
              transition: "opacity 200ms ease-out",
              boxSizing: "border-box",
            }}
          >
            <div style={{ position: "absolute", left: 10, top: 10, width: 4, height: 4, borderRadius: 2, background: "var(--colorNeutralForeground2)" }} />
          </div>
        )}
        {view.phase === "ready" && <ReadyBar key={`${view.round}-${view.run}`} />}
        {view.phase === "answer" && (
          <div className="flex flex-col items-center justify-center gap-5" style={{ position: "absolute", inset: 0 }}>
            <Title3>{view.pair.question}</Title3>
            <div className="flex gap-3">
              <Button onClick={() => onAnswer(true)}>The first</Button>
              <Button onClick={() => onAnswer(false)}>The second</Button>
            </div>
            <Body1 style={muted}>If you can't tell, go with your gut.</Body1>
          </div>
        )}
        {view.phase === "reveal" && view.reveal && (
          <div className="flex flex-col items-center justify-center gap-3" style={{ position: "absolute", inset: 0 }}>
            <Title3>{view.reveal.right ? "Right" : "Not this time"}</Title3>
            <Body1>
              The {view.reveal.askedFirst ? "first" : "second"} run had {view.pair.asked}, the {view.reveal.askedFirst ? "second" : "first"} {view.pair.other}.
            </Body1>
            <Body1 className="tabular-nums" style={muted}>
              {view.reveal.score} of {view.reveal.done} right so far
            </Body1>
            <div className="mt-2 flex items-center gap-4">
              <Button appearance="primary" onClick={onNext}>
                {view.round + 1 >= ROUNDS ? "See your result" : "Next round"}
              </Button>
              {view.reveal.done >= FEWEST && view.round + 1 < ROUNDS && (
                <Link as="button" onClick={onStop}>
                  Finish here
                </Link>
              )}
            </div>
          </div>
        )}
      </div>
    </Portal>
  );
}

/** A thin line filling while the mouse is set up for the next run. */
function ReadyBar() {
  const [w, setW] = useState(0);
  useEffect(() => {
    const id = requestAnimationFrame(() => setW(100));
    return () => cancelAnimationFrame(id);
  }, []);
  return (
    <div style={{ position: "absolute", left: "50%", bottom: 40, width: 240, marginLeft: -120, height: 4, borderRadius: 2, background: "var(--colorNeutralStroke2)", overflow: "hidden" }}>
      <div style={{ width: `${w}%`, height: "100%", background: "var(--colorNeutralForeground3)", transition: `width ${READY_MS}ms linear` }} />
    </div>
  );
}
