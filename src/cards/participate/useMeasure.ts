import { useEffect, useRef, useState } from "react";
import type { CaptureScreen, MeterSample } from "@/api";
import { useCapture, useHold } from "@/cards/lab/parts";
import { num, t } from "@/i18n";
import { useRawInput } from "@/rawInput";
import type { Measured } from "./report";

const median = (xs: number[]) => (xs.length ? [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)] : null);

/** `title` and `detail` come in the language shown. */
type Run = { hz: number[]; ms: number; stutters: number; rate: number | null; want: number; title: string; detail: string; done: ((m: Measured) => void) | null };

function screenOf(r: Run): CaptureScreen {
  return {
    title: r.title,
    detail: r.detail,
    figure: r.rate ? num(r.rate) : "–",
    figureLabel: r.rate ? t("reports a second") : t("Start moving"),
    progress: r.want ? r.ms / r.want : 0,
    progressLabel: t("{{counted}} of {{total}} s counted", { counted: num(r.ms / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 }), total: num(r.want / 1000) }),
  };
}

/**
 * A mouse's polling rate, measured: quick circles on game mode's screen, its reports counted the
 * way a game reads them. Nothing is sent to the mouse, so it measures one hrtz leaves alone too.
 * Esc gives what was counted so far, and so does running out of time, where there's a limit.
 */
export function useMeasure() {
  const cap = useCapture();
  const [on, setOn] = useState(false);
  const run = useRef<Run>({ hz: [], ms: 0, stutters: 0, rate: null, want: 0, title: "", detail: "", done: null });
  const [, tick] = useState(0);

  const finish = () => {
    const r = run.current;
    const done = r.done;
    if (!done) return;
    r.done = null;
    setOn(false);
    cap.close();
    done({ hz: median(r.hz), stutters: r.stutters });
  };

  useHold(on);
  useRawInput(on, (s: MeterSample) => {
    const r = run.current;
    r.rate = s.held && s.hz ? s.hz : null;
    if (s.held && s.hz) {
      r.hz.push(s.hz);
      r.ms += 100;
      r.stutters += s.stutters;
    }
    if (r.ms >= r.want) finish();
    else tick((n) => n + 1);
  });

  // The screen follows the count.
  useEffect(() => {
    if (on) cap.update(screenOf(run.current));
  });

  // Esc, or another window in front: what was counted so far.
  useEffect(() => {
    if (cap.closed && cap.closed !== "done") finish();
  }, [cap.closed]);

  // Out of time: a mouse that isn't moved isn't waited for past this.
  const [limit, setLimit] = useState<number | null>(null);
  useEffect(() => {
    if (!on || limit === null) return;
    const t = setTimeout(finish, limit);
    return () => clearTimeout(t);
  }, [on, limit]);

  // Left while counting: whoever waits is answered.
  useEffect(() => () => run.current.done?.({ hz: median(run.current.hz), stutters: run.current.stutters }), []);

  return {
    on,
    /** Count `ms` of movement, for `within` ms at most where it's given. */
    measure: (ms: number, copy: { title: string; detail: string }, within: number | null = null) =>
      new Promise<Measured>((resolve) => {
        run.current = { hz: [], ms: 0, stutters: 0, rate: null, want: ms, ...copy, done: resolve };
        setLimit(within);
        setOn(true);
        cap.start(screenOf(run.current));
      }),
    stop: finish,
  };
}
