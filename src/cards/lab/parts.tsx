import { Fragment, useEffect, useRef, useState } from "react";
import { Caption1, Caption1Strong } from "@fluentui/react-components";
import { api, on, type CaptureClosed, type CaptureScreen, type ConnStats, type Lod, type MeterSample } from "@/api";
import { useRawInput } from "@/rawInput";
import { GAP_BINS, fill, fmt, got, perMinute, type ConnRun } from "./connection";
import { LOD_NAME, leakPerLift, stallsPerMinute, type LodResult } from "./liftoff";
import { mm } from "./tracking";

// Pieces the Lab page's guided tests share.

const muted = { color: "var(--colorNeutralForeground3)" };
const warn = { color: "var(--colorPaletteMarigoldForeground1)" };
const good = { color: "var(--colorPaletteGreenForeground1)" };

/** Gaps by size: how many polls in a row went by without a report. The long ones in amber. */
export function GapBars({ stats }: { stats: ConnStats }) {
  const top = Math.max(1, ...stats.gaps);
  const h = (n: number) => (n ? 4 + (Math.log10(n + 1) / Math.log10(top + 1)) * 36 : 0);
  return (
    <figure className="tw m-0 flex flex-col gap-1" aria-label="Gaps by polls missed in a row">
      <div className="grid grid-cols-6 items-end gap-1.5" style={{ height: 56 }}>
        {stats.gaps.map((n, i) => (
          <div key={i} className="flex flex-col items-center justify-end gap-0.5">
            <span className="text-[10px] leading-3 tabular-nums" style={i === 5 && n ? warn : muted}>
              {n ? fmt(n) : ""}
            </span>
            <div
              className="w-3/5 rounded-[3px]"
              style={{ height: h(n), background: i === 5 && n ? "var(--colorPaletteMarigoldForeground1)" : "var(--colorNeutralForeground3)", opacity: n ? 0.55 : 0 }}
            />
          </div>
        ))}
      </div>
      <div className="grid grid-cols-6 gap-1.5 border-t pt-1">
        {GAP_BINS.map((b) => (
          <span key={b} className="text-center text-[10px] leading-3 tabular-nums" style={muted}>
            {b}
          </span>
        ))}
      </div>
      <figcaption className="text-[11px]" style={muted}>
        Gaps by polls missed in a row
      </figcaption>
    </figure>
  );
}

/** A row per rate a check tried, yours first: reports a second, stutters, and polls filled. The
 *  one that ran best in green. */
export function RateTable({ runs, chosen }: { runs: ConnRun[]; chosen: number }) {
  const right = { textAlign: "right" } as const;
  return (
    <div className="tw grid items-center gap-x-4 gap-y-1.5 tabular-nums" style={{ gridTemplateColumns: "auto repeat(3, minmax(0, 1fr))" }}>
      {["Rate", "Reports a second", "Stutters", "Polls filled"].map((h, i) => (
        <Caption1 key={h} style={{ ...muted, ...(i ? right : {}) }}>
          {h}
        </Caption1>
      ))}
      {runs.map((r) => {
        const stutters = perMinute(r.stats);
        return (
          <Fragment key={r.setup.hz}>
            <Caption1Strong style={r.setup.hz === chosen ? good : undefined}>{fmt(r.setup.hz)} Hz</Caption1Strong>
            <Caption1 style={right}>{fmt(got(r))}</Caption1>
            <Caption1 style={{ ...right, ...(stutters >= 2 ? warn : {}) }}>{stutters < 0.05 ? "none" : `${stutters >= 10 ? fmt(stutters) : stutters.toFixed(1)} a minute`}</Caption1>
            <Caption1 style={right}>{Math.round(fill(r.stats) * 100)}%</Caption1>
          </Fragment>
        );
      })}
    </div>
  );
}

/** The motion a step asks for, drawn: quick circles, a swipe between two stops, a slide lifted
 *  and carried back, a straight lift, or holding still. */
export function Motion({ kind }: { kind: "circles" | "swipe" | "return" | "lift" | "still" }) {
  const path =
    kind === "return"
      ? "M20,42 L90,42 Q100,42 96,26 Q55,6 24,26 Q16,42 20,42"
      : kind === "lift"
        ? "M60,42 L60,20 L60,42"
        : kind === "swipe"
          ? "M20,42 L100,42 L20,42"
          : kind === "circles"
            ? "M60,26 a14,12 0 1,1 0.1,0"
            : "M60,42 L60,42";
  const dur = { return: "2.4s", swipe: "1.2s", circles: "0.9s", lift: "1.6s", still: "1s" }[kind];
  return (
    <svg viewBox="0 0 120 56" className="h-14 w-[120px] shrink-0" aria-hidden>
      <line x1={6} x2={114} y1={50} y2={50} stroke="var(--colorNeutralStroke1)" strokeWidth={1} />
      {kind === "swipe" && (
        <>
          <rect x={6} y={30} width={6} height={20} rx={1} fill="var(--colorNeutralStroke1)" />
          <rect x={108} y={30} width={6} height={20} rx={1} fill="var(--colorNeutralStroke1)" />
        </>
      )}
      <g>
        <rect x={-5} y={-8} width={10} height={16} rx={5} fill="var(--colorNeutralForeground2)" />
        <animateMotion dur={dur} repeatCount="indefinite" path={path} />
      </g>
    </svg>
  );
}

/** Installed games' exe names in lower case, for the per-game notes. Read once a session. */
let games: string[] | null = null;
export function useGames() {
  const [list, setList] = useState(games ?? []);
  useEffect(() => {
    if (games) return;
    api.installedGames().then(
      (g) => setList((games = g.map((x) => x.exe.toLowerCase()))),
      () => {},
    );
  }, []);
  return list;
}

/**
 * Game mode, for the timing tests: a screen of hrtz's own takes the pointer as a game does. Over
 * hrtz's page, Windows hands hrtz about 150 reports a second, all marked as for an app in the
 * background; on this screen, with the pointer hidden and held, it gets every one (the user's
 * logs, 2026-10-17). `start` opens it, `update` changes what it says, and it closes on Esc, on
 * switching away, on `close`, or when the test goes.
 */
export function useCapture() {
  const [open, setOpen] = useState(false);
  const [closed, setClosed] = useState<CaptureClosed | null>(null);
  const live = useRef(false);
  const said = useRef("");
  useEffect(() => {
    const unlisten = on.captureClosed((why) => {
      live.current = false;
      setOpen(false);
      setClosed(why);
    });
    // Should Windows keep the screen from the front, Esc reaches the page instead.
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape" && live.current) void api.captureClose();
    };
    window.addEventListener("keydown", esc);
    return () => {
      void unlisten.then((f) => f());
      window.removeEventListener("keydown", esc);
      if (live.current) void api.captureClose();
    };
  }, []);
  return {
    open,
    /** Why it last closed, until it opens again. */
    closed,
    start: (screen: CaptureScreen) => {
      live.current = true;
      said.current = JSON.stringify(screen);
      setOpen(true);
      setClosed(null);
      void api.captureOpen(screen);
    },
    update: (screen: CaptureScreen) => {
      const key = JSON.stringify(screen);
      if (!live.current || key === said.current) return;
      said.current = key;
      void api.captureUpdate(screen);
    },
    close: () => {
      if (live.current) void api.captureClose();
    },
  };
}

/** The tests asking for the hold now: the backend hears whether any is. */
const holders = new Set<object>();
const syncHold = () => void api.meterHold(holders.size > 0);

/**
 * While `on`, a timing test's timing counts only in game mode, with the pointer held (see
 * `useCapture`). Says whether it's held now. A test that isn't asking says nothing, so it can't
 * undo one that is.
 */
export function useHold(on: boolean) {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    if (!on) return;
    const me = {};
    holders.add(me);
    syncHold();
    return () => {
      holders.delete(me);
      syncHold();
    };
  }, [on]);
  useRawInput(on, (s: MeterSample) => setHeld(s.held));
  return on && held;
}

/** A row per lift-off tried: how often the sensor lost track on slow slides, and how far lifting
 *  moved the cursor. The one that suits the pad in green. */
export function LodTable({ tried, dpi, chosen }: { tried: LodResult[]; dpi: number; chosen: Lod | null }) {
  const right = { textAlign: "right" } as const;
  return (
    <div className="tw grid items-center gap-x-4 gap-y-1.5 tabular-nums" style={{ gridTemplateColumns: "auto repeat(2, minmax(0, 1fr))" }}>
      {["Lift-off", "Lost track", "Moved per lift"].map((h, i) => (
        <Caption1 key={h} style={{ ...muted, ...(i ? right : {}) }}>
          {h}
        </Caption1>
      ))}
      {tried.map((r) => {
        const stalls = stallsPerMinute(r);
        return (
          <Fragment key={r.lod}>
            <Caption1Strong style={chosen === r.lod ? good : undefined}>{LOD_NAME[r.lod]}</Caption1Strong>
            <Caption1 style={{ ...right, ...(stalls >= 1 ? warn : {}) }}>{stalls < 0.05 ? "never" : `${stalls.toFixed(1)} a minute`}</Caption1>
            <Caption1 style={right}>{mm(leakPerLift(r, dpi))}</Caption1>
          </Fragment>
        );
      })}
    </div>
  );
}
