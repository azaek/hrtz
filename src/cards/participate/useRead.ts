import { useEffect, useRef, useState } from "react";
import { api, message, on, type MouseRead } from "@/api";

/** How often a receiver is asked again whether its mouse has woken. It answers for itself, so
 *  asking costs the mouse nothing. */
const ASK_EVERY_MS = 1000;

export type ReadStage = "idle" | "asking" | "waking" | "reading" | "done";

/** The reading under way, whoever started it: one left behind (stopped, or its page left) runs to
 *  its end, and the next waits for it, so its progress isn't taken for the next one's. */
let flying: Promise<unknown> | null = null;

/**
 * The check of a mouse hrtz leaves alone, on `connection`: once started, its receiver is asked
 * until its mouse is awake, then the mouse is read (who it is, its firmware, its memory). Nothing
 * is written, and nothing is sent before `start`.
 */
export function useRead(connection: string | null) {
  const [stage, setStage] = useState<ReadStage>("idle");
  const [read, setRead] = useState<MouseRead | null>(null);
  /** The receiver's last answer while its mouse sleeps: whose mouse it holds. */
  const [asleep, setAsleep] = useState<MouseRead | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [progress, setProgress] = useState(0);
  /** Which run is live: a stopped one's answers are dropped. */
  const run = useRef(0);
  /** This run's own reading is the one under way: its progress counts. */
  const mine = useRef(false);

  useEffect(() => {
    const unlisten = on.inspectProgress(([done, total]) => {
      if (!mine.current) return;
      setProgress(total ? done / total : 0);
      setStage((s) => (s === "asking" || s === "waking" ? "reading" : s));
    });
    return () => {
      run.current++;
      mine.current = false;
      void unlisten.then((f) => f());
    };
  }, []);

  const start = async () => {
    if (!connection) return;
    const id = ++run.current;
    const live = () => run.current === id;
    mine.current = false;
    setRead(null);
    setAsleep(null);
    setError(null);
    setProgress(0);
    setStage("asking");
    for (;;) {
      if (flying) await flying.catch(() => null);
      if (!live()) return;
      let r: MouseRead;
      try {
        const call = api.inspectRead(connection);
        flying = call;
        mine.current = true;
        r = await call;
      } catch (e) {
        if (live()) {
          setError(message(e));
          setStage("idle");
        }
        return;
      } finally {
        if (live()) mine.current = false;
      }
      if (!live()) return;
      if (!r.asleep) {
        setRead(r);
        setStage("done");
        return;
      }
      setAsleep(r);
      setStage("waking");
      await new Promise((next) => setTimeout(next, ASK_EVERY_MS));
      if (!live()) return;
    }
  };

  /** Stop waiting for the mouse to wake. A reading under way finishes by itself, unused. */
  const stop = () => {
    run.current++;
    mine.current = false;
    setStage("idle");
  };

  return { stage, read, asleep, error, progress, start, stop };
}
