import { useEffect, useRef, useState } from "react";
import { Body1, Button, Caption1, Spinner } from "@fluentui/react-components";
import { api, message, type TrialChange } from "@/api";
import { msg, num, t } from "@/i18n";
import { pickMouse, toast } from "@/store";
import { pollingOf, type Part, type Tried } from "./report";
import { useMeasure } from "./useMeasure";

const muted = { color: "var(--colorNeutralForeground3)" };

/** How long the new rate is counted for, and how long that's waited for at most: hrtz puts a
 *  change back by itself after 45 s, which this must stay well under. */
const COUNT_MS = 3000;
const WITHIN_MS = 20_000;

/** What's under way, said while it is. */
const doing = () => ({ changing: t("Changing its polling rate"), measuring: t("Follow the full screen"), restoring: t("Putting it back") });

/**
 * One change tried on a mouse whose model hrtz doesn't know, when its owner asks: its polling rate
 * set to another rate, counted on the full screen, and put back. The proof that a setting can be
 * changed, in the same report: without it, that takes a second report once the mouse is added.
 *
 * Only the polling rate, and only once hrtz's reading of it matched the count (`canTry`), so what's
 * written is known to be the rate. One hrtz leaves alone is changed through its reading's own
 * connection, and hrtz puts it back by itself should this page go first; one hrtz runs, through
 * its driver.
 */
export function Trial({ part, onTried }: { part: Part; onTried: (t: Tried) => void }) {
  const m = useMeasure();
  const [stage, setStage] = useState<keyof ReturnType<typeof doing> | null>(null);
  /** Puts back what was changed, while something is. */
  const undo = useRef<(() => Promise<boolean>) | null>(null);
  const alive = useRef(true);
  // Left while a change is set: it goes back, and nothing more is said of it.
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      const back = undo.current;
      undo.current = null;
      void back?.().catch(() => null);
    };
  }, []);

  /** The change, made: through the reading's connection, or the driver of a mouse hrtz runs. What
   *  puts it back is set before the change is asked for, so leaving while it's made puts it back. */
  const change = async (): Promise<TrialChange> => {
    if (part.how === "read") {
      const connection = part.connection ?? "";
      // Asked on the same connection, so it's done after the change. With nothing changed it's
      // refused, which does no harm.
      undo.current = async () => ((await api.inspectTrial(connection, true)) as { restored: boolean }).restored;
      return (await api.inspectTrial(connection, false)) as TrialChange;
    }
    if (part.key) pickMouse(part.key);
    const now = (await api.states()).find((d) => d.key === part.key);
    if (!now?.online) throw new Error(t("The mouse is asleep. Move it, then try again."));
    const was = now.settings;
    const from = was?.pollingHz;
    // 500 Hz, or 1,000 from 500: else any other rate it has.
    const rates = now.capabilities?.polling?.rates ?? [];
    const to = [from === 500 ? 1000 : 500, ...rates].find((hz) => hz !== from && rates.includes(hz));
    if (!was || !from || !to) throw new Error(t("hrtz can't change this mouse's polling rate."));
    // Only its polling rate goes back: what else changed meanwhile (its DPI button) stays.
    undo.current = async () => {
      const held = (await api.states()).find((d) => d.key === part.key)?.settings ?? was;
      return (await api.applySettings({ ...held, pollingHz: from })).pollingHz === from;
    };
    const after = await api.applySettings({ ...was, pollingHz: to });
    // English, as the report keeps it; the screen says it with `t`.
    return { what: msg("Polling rate"), from, to, kept: after.pollingHz === to };
  };

  /** Put back what was changed, once. */
  const putBack = async () => {
    const back = undo.current;
    undo.current = null;
    return back ? await back().catch(() => false) : false;
  };

  const run = async () => {
    let made: TrialChange;
    setStage("changing");
    try {
      made = await change();
    } catch (e) {
      // Nothing was changed for good: said, and it can be tried again.
      await putBack();
      if (!alive.current) return;
      setStage(null);
      toast(message(e), "error");
      return;
    }
    // The page went while it was made: back it goes (its leaving has asked already).
    if (!alive.current) return void putBack();
    setStage("measuring");
    const counted = await m.measure(
      COUNT_MS,
      { title: t("Keep moving the mouse in quick circles"), detail: t("Counting its reports at {{hz}} Hz. Then it goes back to {{from}}.", { hz: num(made.to), from: num(made.from) }) },
      WITHIN_MS,
    );
    if (!alive.current) return void putBack();
    setStage("restoring");
    const restored = await putBack();
    if (!alive.current) return;
    setStage(null);
    onTried({ ...made, measured: counted.hz, restored });
  };

  const { set } = pollingOf(part);
  const to = set === 500 ? 1000 : 500;
  return (
    <div className="flex w-full items-center justify-between gap-4 rounded-[var(--borderRadiusMedium)] border px-4 py-3 text-left">
      <div className="flex min-w-0 flex-col gap-0.5">
        <Body1>{t("Try one change?")}</Body1>
        <Caption1 style={muted}>
          {t("hrtz sets its polling rate to {{hz}} Hz, counts its reports while you move it, and puts it back. It shows a setting can be changed, so we don't have to ask you later.", { hz: num(to) })}
        </Caption1>
      </div>
      {stage ? (
        <span className="flex shrink-0 items-center gap-2" style={muted}>
          <Spinner size="extra-tiny" />
          <Caption1>{doing()[stage]}</Caption1>
        </span>
      ) : (
        <Button style={{ flexShrink: 0 }} onClick={() => void run()}>
          {t("Try a change")}
        </Button>
      )}
    </div>
  );
}
