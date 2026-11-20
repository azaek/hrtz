import { useEffect, useRef, useState, type ReactNode } from "react";
import { Body1, Button, Link, Spinner } from "@fluentui/react-components";
import type { DeviceState, Inspected } from "@/api";
import { num, t } from "@/i18n";
import { StepCard } from "@/kit";
import type { Support } from "@/models";
import { MouseHead } from "./parts";
import { checkedPart, idsOf, nameOf, readPart, statusOf, windowsPart, type Part } from "./report";
import { useMeasure } from "./useMeasure";
import { useParticipate } from "./useParticipate";
import { useRead } from "./useRead";

const soft = { color: "var(--colorNeutralForeground2)" };

/**
 * What the check can do, by how far hrtz is with the mouse: a model it knows (tested or not), a
 * mouse it runs without knowing the model, one it leaves alone but can read, one whose mouse
 * hasn't answered yet, or one it only knows through Windows.
 */
type Kind = Support | "reads" | "asleep" | "windows";

/** What the check does, said before it starts. */
const byKind = (): Record<Exclude<Kind, "asleep">, string> => ({
  tested: t("This model is tested. If something looks wrong, check it and send the report."),
  untested: t(
    "A 2-minute check of what works on this model. It reads the mouse, measures its polling rate, asks you to press each button, then changes each setting hrtz offers on it, one at a time, and puts each back.",
  ),
  unknown: t(
    "hrtz runs this mouse but doesn't know the model yet. The check reads its settings and memory, measures its polling rate and asks you to press each button. It changes nothing.",
  ),
  reads: t(
    "hrtz doesn't run this mouse yet, but can read it. The check reads which mouse it is and what its memory holds, the way its own app does, then measures its polling rate. It changes nothing. Close the mouse's own app first.",
  ),
  windows: t("hrtz can't read this mouse yet, so its report holds what it tells Windows: its IDs and how it connects. Nothing is sent to it."),
});

/** The same, for a model hrtz knows, by what it offers on it: where hrtz sets no buttons yet (an
 *  ATK), only the DPI button is asked for, where the mouse says when it's pressed. */
function about(kind: Exclude<Kind, "asleep">, device: DeviceState | null): string {
  const caps = device?.capabilities;
  if (kind !== "untested" || !caps || caps.buttons?.length) return byKind()[kind];
  return caps.dpiButton && caps.dpi
    ? t(
        "A 2-minute check of what works on this model. It reads the mouse, measures its polling rate, asks you to press its DPI button, then changes each setting hrtz offers on it, one at a time, and puts each back.",
      )
    : t("A 2-minute check of what works on this model. It reads the mouse, measures its polling rate, then changes each setting hrtz offers on it, one at a time, and puts each back.");
}

/** What each stage asks of you, while the full screen is up. */
const doing = (): Record<string, string> => ({
  reading: t("Reading the mouse"),
  moving: t("Move the mouse in quick circles"),
  buttons: t("Press each button when asked"),
  writing: t("Checking each setting"),
  rate: t("Move the mouse in quick circles again"),
});

/** How long a mouse is waited for before the card says what else to look at. */
const SLOW_MS = 20_000;
/** Movement counted for the polling rate of a mouse hrtz reads. */
const COUNT_MS = 5000;

/** Whether `active` has lasted `ms`. */
function useAfter(active: boolean, ms: number) {
  const [late, setLate] = useState(false);
  useEffect(() => {
    setLate(false);
    if (!active) return;
    const t = setTimeout(() => setLate(true), ms);
    return () => clearTimeout(t);
  }, [active, ms]);
  return late;
}

/**
 * Inspect a mouse's check of the mouse picked: everything hrtz can safely learn of it, handed on
 * as one part of a report (`onDone`). One hrtz runs is read, its polling measured and each button
 * pressed, on the full screen; a model it knows also has each setting hrtz offers on it changed
 * and put back, one at a time, so one check says all it keeps. One it leaves alone but can read is read, then its polling rate measured (which sends it nothing),
 * with nothing changed. A mouse that's asleep is waited for, with what to do said plainly: a
 * report from a sleeping mouse holds next to nothing.
 */
export function MouseCheck({ mouse, support, device, onDone }: { mouse: Inspected; support: Support | undefined; device: DeviceState | null; onDone: (part: Part) => void }) {
  const p = useParticipate();
  const r = useRead(mouse.connection);
  const m = useMeasure();
  /** Still on the page: a count that ends after it's left isn't handed on. */
  const here = useRef(true);
  useEffect(() => {
    here.current = true;
    return () => {
      here.current = false;
    };
  }, []);
  const runs = mouse.reach === "runs" && !!device?.connected;
  // A mouse hrtz runs without a model it lists (a receiver's mouse that names none) is read like
  // one it doesn't know. One listed as run that the window hasn't heard of yet is waited for.
  const kind: Kind = runs ? (support ?? "unknown") : mouse.reach === "reads" || mouse.reach === "windows" ? mouse.reach : "asleep";
  const writes = kind === "untested" || kind === "tested";
  /** Start was pressed with the mouse asleep: the check of that kind starts once it wakes. */
  const [waking, setWaking] = useState<Kind | null>(null);
  /** Whether the run under way tries a change. */
  const wrote = useRef(false);
  const running = p.stage !== "idle" && p.stage !== "done";
  const reading = r.stage === "asking" || r.stage === "reading";
  const waiting = !running && !reading && !m.on && (!!waking || r.stage === "waking" || kind === "asleep");
  const slow = useAfter(waiting, SLOW_MS);

  const begin = () => {
    wrote.current = writes;
    void p.start({ write: writes });
  };
  const start = () => {
    if (kind === "reads") void r.start();
    else if (!device?.online) setWaking(kind);
    else begin();
  };

  // Awake: the check waited for starts. Should the mouse turn out to be another kind (its receiver
  // names it only now), what its check does is said first, and Start asked again.
  useEffect(() => {
    if (!waking) return;
    if (kind !== waking) setWaking(null);
    else if (device?.online) {
      setWaking(null);
      begin();
    }
  }, [waking, kind, device?.online]);

  useEffect(() => {
    if (p.stage === "done" && p.checks && device) onDone(checkedPart(mouse, device, wrote.current, p.checks, p.dump));
  }, [p.stage]);
  // Read whole: its polling rate is counted, to hold against what its memory says. A reading
  // that stopped short goes on as it is: it's checked again anyway.
  useEffect(() => {
    const read = r.read;
    if (r.stage !== "done" || !read) return;
    if ((read.memorySize ?? 0) < (read.memoryWanted ?? 1)) return onDone(readPart(mouse, read, null));
    void m
      .measure(COUNT_MS, { title: t("Move the mouse in quick circles"), detail: t("Measures how often it reports, the way a game reads it.") })
      .then((measured) => here.current && onDone(readPart(mouse, read, measured)));
  }, [r.stage]);

  const wait = (detail: string, actions: ReactNode) => (
    <div className="w-full text-left">
      <StepCard
        label={t("Waiting for the mouse")}
        title={t("Move your mouse to wake it")}
        detail={detail}
        status={
          <span className="flex items-center gap-2" style={soft}>
            <Spinner size="extra-tiny" />
            {slow ? t("Still nothing. Check that it's switched on, and paired with this receiver.") : t("Waiting for it to answer")}
          </span>
        }
        actions={actions}
      />
    </div>
  );

  return (
    <div className="tw flex w-full flex-col items-center gap-3 text-center">
      <MouseHead artKey={mouse.key} variant={device?.variant} name={nameOf(mouse)} caption={`${idsOf(mouse)} · ${statusOf(mouse, support)}`} />
      {kind !== "asleep" && (
        <Body1 style={soft}>
          {running
            ? t("Running full screen. Press Esc to stop; what it changed goes back.")
            : m.on
              ? t("Running full screen. Press Esc to skip it.")
              : p.stopped
                ? t("Stopped. Your settings are as they were.")
                : (r.error ?? about(kind, device))}
        </Body1>
      )}

      {running ? (
        <div className="w-full text-left">
          <StepCard
            label={t("Checking")}
            title={doing()[p.stage]}
            status={<span style={soft}>{t("Follow the full screen.")}</span>}
            actions={
              <Link as="button" onClick={p.stop}>
                {t("Stop")}
              </Link>
            }
          />
        </div>
      ) : m.on ? (
        <div className="w-full text-left">
          <StepCard
            label={t("Measuring")}
            title={doing().moving}
            status={<span style={soft}>{t("Follow the full screen.")}</span>}
            actions={
              <Link as="button" onClick={m.stop}>
                {t("Skip")}
              </Link>
            }
          />
        </div>
      ) : reading ? (
        <div className="w-full text-left">
          <StepCard
            label={t("Reading")}
            title={t("Reading the mouse")}
            detail={t("Which mouse it is, and what its memory holds. Keep it awake until it's done.")}
            progress={r.progress}
            status={<span style={soft}>{t("{{percent}}%", { percent: num(Math.round(r.progress * 100)) })}</span>}
          />
        </div>
      ) : waking ? (
        wait(
          t("Its check starts once it answers."),
          <Link as="button" onClick={() => setWaking(null)}>
            {t("Stop")}
          </Link>,
        )
      ) : r.stage === "waking" ? (
        wait(
          t("Its receiver answered, but the mouse is asleep. It's read once it wakes."),
          <>
            <Link as="button" onClick={() => onDone(windowsPart(mouse, true, r.asleep))}>
              {t("Use what's known instead")}
            </Link>
            <Link as="button" onClick={r.stop}>
              {t("Stop")}
            </Link>
          </>,
        )
      ) : kind === "asleep" ? (
        wait(
          t("hrtz found its receiver, but the mouse hasn't answered yet. Its check shows here once it does."),
          <Link as="button" onClick={() => onDone(windowsPart(mouse, true))}>
            {t("Use what Windows knows instead")}
          </Link>,
        )
      ) : kind === "windows" ? (
        <Button appearance="primary" onClick={() => onDone(windowsPart(mouse))}>
          {t("Continue")}
        </Button>
      ) : (
        <Button appearance="primary" onClick={start}>
          {t("Start the check")}
        </Button>
      )}
    </div>
  );
}
