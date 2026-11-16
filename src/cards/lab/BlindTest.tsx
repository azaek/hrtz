import { useEffect, useRef, useState } from "react";
import { Body1, Button, Caption1, Link, Portal, Title3 } from "@fluentui/react-components";
import { Choice } from "@/components";
import { StepCard } from "@/kit";
import { hzText } from "@/rates";
import { commit, toast, useStore } from "@/store";
import { blindSummary, dotAt, pairsFor, READY_MS, ROUNDS, shareText, useBlindTest, type Pair, type Phase, type Result } from "./blind";
import { Motion } from "./parts";
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
          onStop={() => {
            setStopped(true);
            t.stop();
          }}
        />
      )}
      {t.result && !t.running ? (
        <Outcome r={t.result} again={start} online={online} choose={choose} />
      ) : (
        <StepCard
          label={`About 2 minutes · ${ROUNDS} rounds`}
          title={`Can you feel ${pair.feel}?`}
          detail={`hrtz switches your mouse between ${pair.asked} and ${pair.other} without telling you which. Follow the dot for a few seconds, twice, then say which was which. Yours goes back at the end.`}
          art={<Motion kind="circles" />}
          status={stopped ? <span style={muted}>Stopped. Five rounds or more make a result.</span> : undefined}
          actions={
            <>
              {choose}
              <Button appearance="primary" disabled={!online || t.running} onClick={start}>
                Start
              </Button>
            </>
          }
        />
      )}
    </>
  );
}

/** What the test found, with the next step. */
function Outcome({ r, again, online, choose }: { r: Result; again: () => void; online: boolean; choose: React.ReactNode }) {
  const device = useStore((x) => x.device);
  const s = device?.settings;
  const pct = Math.round(Math.abs(r.closer) * 100);
  const title =
    r.right >= 8 ? `You can feel ${r.pair.feel}` : r.right >= 6 ? `Hard to say: ${r.right} of ${r.rounds} right` : `You couldn't tell ${r.pair.asked} from ${r.pair.other}`;
  const odds = r.chance < 0.01 ? "less than 1%" : `${Math.round(r.chance * 100)}%`;
  const tracking =
    r.consistent && pct >= 1
      ? `You followed the dot ${pct}% closer with ${r.closer > 0 ? r.pair.asked : r.pair.other}.`
      : "You followed the dot about as closely with either.";
  const screen =
    r.refreshHz && r.pair.id === "polling"
      ? r.refreshHz <= 75
        ? ` Your screen runs at ${r.refreshHz} Hz, where a faster polling rate is hard to see.`
        : ` Your screen runs at ${r.refreshHz} Hz.`
      : "";
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
      detail={`${r.right} of ${r.rounds} right. Guessing gets that many right ${odds} of the time. ${tracking}${screen}`}
      status={
        <Link as="button" onClick={copy}>
          Copy result
        </Link>
      }
      actions={
        <>
          {choose}
          {saver && (
            <Button disabled={!online} onClick={() => s && void commit({ ...s, pollingHz: low })}>
              Use {hzText(low)}
            </Button>
          )}
          <Button appearance={saver ? "secondary" : "primary"} disabled={!online} onClick={again}>
            Test again
          </Button>
        </>
      }
    >
      {saver && <Caption1 style={soft}>{`${hzText(low)} lasts longer on a charge, and you didn't feel the difference.`}</Caption1>}
    </StepCard>
  );
}

/** The test's own screen over the whole window: the round, the dot to follow, then the question. */
function Arena({
  view,
  onSample,
  onAnswer,
  onStop,
}: {
  view: { round: number; run: 0 | 1; phase: Phase; seed: number; pair: Pair };
  onSample: (miss: number, frameMs: number) => void;
  onAnswer: (first: boolean) => void;
  onStop: () => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const dot = useRef<HTMLDivElement>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const handlers = useRef({ onSample, onStop });
  handlers.current = { onSample, onStop };

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && handlers.current.onStop();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, []);

  // The dot moves every frame of a run; before one, it waits where the run will start.
  useEffect(() => {
    const el = box.current;
    if (!el || view.phase === "answer") return;
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
    view.phase === "answer"
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
        {view.phase !== "answer" && (
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
