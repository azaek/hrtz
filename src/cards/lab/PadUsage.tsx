import { useEffect, useRef, useState } from "react";
import {
  Body1,
  Body1Strong,
  Button,
  Caption1,
  Combobox,
  Dropdown,
  Link,
  makeStyles,
  Option,
  Subtitle2,
  InteractionTag,
  InteractionTagPrimary,
  InteractionTagSecondary,
  TagGroup,
  tokens,
} from "@fluentui/react-components";
import { Games20Regular } from "@fluentui/react-icons";
import { api, type GameSensitivity, type MeterSample, type PadApp, type PadCalibration, type PadView, type Prefs, type RunningApp } from "@/api";
import { AppIcon } from "@/components";
import { useRawInput } from "@/rawInput";
import { savePrefs, useStore } from "@/store";
import { useReport, type TestProps, type TestResult } from "./result";

/** Sequential blue, light to dark (steps 100–700). Dark mode flips it so "little" recedes into the surface. */
const RAMP = ["#cde2fb", "#9ec5f4", "#6da7ec", "#3987e5", "#256abf", "#184f95", "#0d366b"];
const MAP_MAX_HEIGHT = 260;
/** Room outside the pad for edge-hit marks (px). */
const MARGIN = 8;
/** The app whose map is shown, kept across visits. */
const APP_KEY = "hrtz.padApp";
const ALL = "__all";

type Edge = "top" | "bottom" | "left" | "right";
const STEPS: { key: "home" | Edge; text: string }[] = [
  { key: "home", text: "Put the mouse where you usually rest it when you play, then press Space." },
  { key: "top", text: "Without lifting, slide it to the top edge of the area you use and press Space." },
  { key: "bottom", text: "Slide it to the bottom edge and press Space." },
  { key: "left", text: "Slide it to the left edge and press Space. Move your arm, not just your wrist." },
  { key: "right", text: "Slide it to the right edge and press Space." },
];

const useStyles = makeStyles({
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM },
  muted: { color: tokens.colorNeutralForeground3 },
  // The figures drop under the map when the pane has no room for both.
  layout: { display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))", columnGap: tokens.spacingHorizontalXL, rowGap: tokens.spacingVerticalL, alignItems: "start" },
  mapWrap: { position: "relative", minWidth: 0 },
  canvas: { display: "block" },
  tooltip: {
    position: "absolute",
    pointerEvents: "none",
    padding: `${tokens.spacingVerticalXS} ${tokens.spacingHorizontalS}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--flyout)",
    boxShadow: tokens.shadow8,
    whiteSpace: "nowrap",
    zIndex: 1,
  },
  legend: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    columnGap: tokens.spacingHorizontalL,
    rowGap: tokens.spacingVerticalXS,
    marginTop: tokens.spacingVerticalS,
    color: tokens.colorNeutralForeground3,
  },
  key: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalXS },
  scale: { display: "inline-flex", gap: "2px" },
  swatch: { width: "10px", height: "10px", borderRadius: "2px" },
  summary: { display: "flex", flexDirection: "column", gap: "2px" },
  tiles: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: tokens.spacingVerticalM },
  tile: { display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 },
  value: { fontVariantNumeric: "tabular-nums" },
  actions: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalL },
  steps: { display: "flex", gap: "6px" },
  step: { width: "24px", height: "4px", borderRadius: "2px", backgroundColor: tokens.colorNeutralStroke1 },
  stepDone: { backgroundColor: tokens.colorBrandBackground },
  error: { color: tokens.colorPaletteRedForeground1 },
  picker: { minWidth: "220px", alignSelf: "flex-start" },
  pickerValue: { display: "inline-flex", alignItems: "center", gap: tokens.spacingHorizontalS, minWidth: 0 },
  appIcon: { width: "16px", height: "16px", flexShrink: 0, objectFit: "contain" },
  fit: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS, maxWidth: "70ch" },
  games: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalS, maxWidth: "70ch" },
  tags: { flexWrap: "wrap", rowGap: tokens.spacingVerticalXS },
  add: { width: "260px" },
  listbox: { maxHeight: "280px" },
});

const sameExe = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

/** How far the mouse travels for a turn in a game: 914.4 / (yaw × sensitivity × DPI) cm per 360°. */
type Turns = { c360: number; c180: number; c90: number };
function turnLengths(sens: GameSensitivity, dpi: number): Turns {
  const c360 = 914.4 / (sens.yaw * sens.sens * dpi);
  return { c360, c180: c360 / 2, c90: c360 / 4 };
}

const isDark = () => document.documentElement.dataset.theme === "dark";
const ramp = () => (isDark() ? [...RAMP].reverse() : RAMP);

function length(cm: number) {
  if (cm < 100) return `${Math.round(cm)} cm`;
  if (cm < 100_000) return `${(cm / 100).toFixed(cm < 1000 ? 1 : 0)} m`;
  return `${(cm / 100_000).toFixed(1)} km`;
}

function duration(ms: number) {
  const min = Math.round(ms / 60_000);
  if (min < 1) return "under a minute";
  if (min < 60) return `${min} min`;
  return `${Math.floor(min / 60)} h ${min % 60} min`;
}

/** The width that `share` of swipes stay within, from 1 cm bins; null with too few swipes to say. */
function percentile(bins: number[], share: number): number | null {
  const total = bins.reduce((a, b) => a + b, 0);
  if (total < 20) return null;
  let seen = 0;
  for (let i = 0; i < bins.length; i++) {
    seen += bins[i];
    if (seen >= total * share) return i;
  }
  return bins.length - 1;
}

/** Turn the four edge positions (sensor counts from the resting spot) into a calibration. */
function solve(p: Record<Edge, [number, number]>, dpi: number): PadCalibration | string {
  const k = 2.54 / dpi;
  const up = [p.top[0] - p.bottom[0], p.top[1] - p.bottom[1]];
  const across = [p.right[0] - p.left[0], p.right[1] - p.left[1]];
  // Rotation that turns the sensor's "up" into the pad's, averaged with the same for "right".
  const au = -Math.PI / 2 - Math.atan2(up[1], up[0]);
  const ar = -Math.atan2(across[1], across[0]);
  const skew = Math.abs(((au - ar + 3 * Math.PI) % (2 * Math.PI)) - Math.PI) * (180 / Math.PI);
  if (skew > 20) {
    return `Your up-down and side-to-side slides came out ${Math.round(skew)}° off square. Redo them, sliding straight to each edge.`;
  }
  const angle = Math.atan2(Math.sin(au) + Math.sin(ar), Math.cos(au) + Math.cos(ar));
  const [c, s] = [Math.cos(angle), Math.sin(angle)];
  const rot = ([x, y]: [number, number]) => [(x * c - y * s) * k, (x * s + y * c) * k];
  const cal = { angle, top: -rot(p.top)[1], bottom: rot(p.bottom)[1], left: -rot(p.left)[0], right: rot(p.right)[0], version: 2 };
  const short = (["top", "bottom", "left", "right"] as const).find((e) => cal[e] < 3);
  if (short) return `The ${short} edge came out ${Math.max(0, cal[short]).toFixed(1)} cm from your spot. Start again and slide all the way to each edge.`;
  const [w, h] = [cal.left + cal.right, cal.top + cal.bottom];
  if (w > 120 || h > 120) return `That came out ${Math.round(w)} × ${Math.round(h)} cm, bigger than any pad. Start again and keep the mouse down the whole time.`;
  return cal;
}

/** One-line summary of a calibration: area, where the resting spot sits, grip angle. */
function describe(cal: PadCalibration) {
  const [w, h] = [cal.left + cal.right, cal.top + cal.bottom];
  // The resting spot is the origin; the area's centre is half the span from each edge.
  const right = (cal.left - cal.right) / 2;
  const up = (cal.top - cal.bottom) / -2;
  const side = Math.abs(right) < 1 ? "" : `${Math.round(Math.abs(right))} cm ${right > 0 ? "right" : "left"} of centre`;
  const height = Math.abs(up) < 1 ? "" : `${Math.round(Math.abs(up))} cm ${up > 0 ? "above" : "below"} it`;
  const spot = side && height ? `${side} and ${height}` : side || (height ? height.replace(" it", " centre") : "at the centre");
  const grip = Math.round(Math.abs((cal.angle * 180) / Math.PI));
  return [`Your area is ${Math.round(w)} × ${Math.round(h)} cm.`, `Your spot is ${spot}.`, grip ? `You hold the mouse turned ${grip}°.` : "You hold the mouse straight."];
}

/**
 * Mousepad use: a calibrated map of where the mouse travels, tracked in Rust (in the background
 * too) and polled here once a second.
 */
export function PadUsage({ dpi, onResult }: { dpi: number } & TestProps) {
  const s = useStyles();
  const prefs = useStore((x) => x.prefs);
  const [pad, setPad] = useState<PadView | null>(null);
  const [step, setStep] = useState<number | null>(null);
  const [review, setReview] = useState<PadCalibration | null>(null);
  const [error, setError] = useState("");
  const pos = useRef<[number, number]>([0, 0]);
  const marks = useRef<Partial<Record<Edge, [number, number]>>>({});
  const [, redraw] = useState(0);
  const [app, setApp] = useState<string | null>(() => localStorage.getItem(APP_KEY));
  const choose = (exe: string | null) => {
    setApp(exe);
    if (exe) localStorage.setItem(APP_KEY, exe);
    else localStorage.removeItem(APP_KEY);
  };

  // A game taken off the list keeps its map, hidden, until it's added back.
  const games = prefs?.padGames ?? [];
  const wanted = app && games.some((g) => sameExe(g.exe, app)) ? app : null;
  const refresh = () => api.padState(wanted).then(setPad).catch(() => {});
  // Four times a second while tracking, so the live dot and lift count keep up; otherwise once.
  const tracking = !!pad?.tracking;
  useEffect(() => {
    refresh();
    const t = setInterval(() => !document.hidden && refresh(), tracking ? 250 : 1000);
    return () => clearInterval(t);
  }, [tracking, wanted]);

  // A game's sensitivity, from its own settings file. Read again now and then, since it can change
  // while the game runs.
  const shown = pad?.app ?? null;
  const [sens, setSens] = useState<GameSensitivity | null>(null);
  useEffect(() => {
    setSens(null);
    if (!shown) return;
    let live = true;
    const read = () => api.gameSensitivity(shown).then((x) => live && setSens(x), () => live && setSens(null));
    read();
    const t = setInterval(() => !document.hidden && read(), 10_000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [shown]);
  const turns = sens && dpi > 0 ? turnLengths(sens, dpi) : null;

  // Calibration: add up raw movement and mark each edge on Space.
  const calibrating = step !== null;
  useRawInput(
    calibrating,
    (sample: MeterSample) => {
      pos.current = [pos.current[0] + sample.dx, pos.current[1] + sample.dy];
      redraw((n) => n + 1);
    },
  );
  useEffect(() => {
    if (!calibrating) return;
    const key = (e: KeyboardEvent) => {
      if (e.code === "Escape") {
        e.preventDefault();
        setStep(null);
        return;
      }
      if (e.code !== "Space") return;
      // Space would otherwise press whichever button has focus.
      e.preventDefault();
      e.stopPropagation();
      setStep((i) => {
        if (i === null) return null;
        const at = STEPS[i].key;
        if (at === "home") pos.current = [0, 0];
        else marks.current[at] = [...pos.current];
        if (i < STEPS.length - 1) return i + 1;
        const result = solve(marks.current as Record<Edge, [number, number]>, dpi);
        if (typeof result === "string") {
          setError(result);
        } else {
          setError("");
          setReview(result);
        }
        return null;
      });
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [calibrating, dpi]);

  const startCalibration = () => {
    marks.current = {};
    pos.current = [0, 0];
    setError("");
    setReview(null);
    setStep(0);
  };

  const cal = pad?.calibration ?? null;
  const listedApps = (pad?.apps ?? []).filter((a) => games.some((g) => sameExe(g.exe, a.exe)));
  const fromSpot = Math.hypot(pos.current[0], pos.current[1]) * (2.54 / dpi);

  let content;
  if (calibrating) {
    content = (
      <>
        <div className={s.steps} aria-hidden>
          {STEPS.map((_, i) => (
            <span key={i} className={i <= step! ? `${s.step} ${s.stepDone}` : s.step} />
          ))}
        </div>
        <Body1Strong aria-live="polite">{STEPS[step!].text}</Body1Strong>
        <Caption1 className={s.muted}>
          Step {step! + 1} of {STEPS.length}
          {step! > 0 ? ` · ${fromSpot.toFixed(1)} cm from your spot` : ""} · Esc cancels
        </Caption1>
        <div className={s.actions}>
          <Button onClick={() => setStep(null)}>Cancel</Button>
        </div>
      </>
    );
  } else if (review) {
    content = (
      <>
        <div className={s.summary} aria-live="polite">
          {describe(review).map((line) => (
            <Body1 key={line}>{line}</Body1>
          ))}
        </div>
        <div className={s.actions}>
          <Button
            appearance="primary"
            onClick={() => {
              api.padCalibrate(review).then(refresh);
              setReview(null);
            }}
          >
            Save
          </Button>
          <Button onClick={startCalibration}>Redo</Button>
        </div>
      </>
    );
  } else if (!cal) {
    content = (
      <>
        <Caption1>
          Mark where you usually rest the mouse, then the pad's edges, pressing Space at each. Takes about 20 seconds.
        </Caption1>
        {error && <Caption1 className={s.error}>{error}</Caption1>}
        <div className={s.actions}>
          <Button onClick={startCalibration}>Calibrate</Button>
        </div>
      </>
    );
  } else {
    content = (
      <>
        {(cal.version ?? 0) < 2 && (
          <Caption1>
            This map was calibrated from the pad's centre. Recalibrate from your usual resting spot so it can learn where you put the mouse
            down.
          </Caption1>
        )}
        {listedApps.length > 0 && <AppPicker apps={listedApps} value={shown} onChange={choose} />}
        <div className={s.layout}>
          <PadMap pad={pad!} turns={turns} />
          <Tiles pad={pad!} />
        </div>
        {turns && sens && <Fit pad={pad!} sens={sens} dpi={dpi} turns={turns} />}
        {prefs && <GameList prefs={prefs} onShow={choose} />}
        {error && <Caption1 className={s.error}>{error}</Caption1>}
        <Caption1 className={s.muted}>
          Estimated from movement and checked against the pad's edges. Learns your lifts and resting spot as you play.
        </Caption1>
        <div className={s.actions}>
          <Button onClick={startCalibration}>Recalibrate</Button>
          <Link as="button" onClick={() => api.padClear().then(refresh)}>
            Clear data
          </Link>
        </div>
      </>
    );
  }

  useReport(onResult, padResult(cal, prefs));
  return <div className={s.body}>{content}</div>;
}

/** The mousepad in a few words: the area it maps, and whether it's recording. Null before a
 *  calibration. */
export function padResult(cal: PadCalibration | null, prefs: Prefs | null): TestResult | null {
  if (!cal) return null;
  const games = prefs?.padGames ?? [];
  const recording = games.length ? `Recording in ${games.map((g) => g.name).join(", ")}` : "Starts with your first game";
  return { value: `${Math.round(cal.left + cal.right)} × ${Math.round(cal.top + cal.bottom)} cm`, detail: prefs?.padTracking ? recording : "Not recording", tone: "neutral" };
}

/** Icons for app paths, fetched once each. */
function useIcons(paths: string[]) {
  const [icons, setIcons] = useState<Record<string, string | null>>({});
  const key = paths.join("|");
  useEffect(() => {
    for (const p of paths) {
      if (p && !(p in icons)) api.appIcon(p).then((icon) => setIcons((m) => ({ ...m, [p]: icon })), () => {});
    }
  }, [key]);
  return icons;
}

/** Which app's map to show: every app together, or one of those kept on their own. */
function AppPicker({ apps, value, onChange }: { apps: PadApp[]; value: string | null; onChange: (exe: string | null) => void }) {
  const s = useStyles();
  const icons = useIcons(apps.map((a) => a.path));
  const current = apps.find((a) => a.exe === value);
  const name = current?.name ?? "All games";
  return (
    <Dropdown
      aria-label="Show the map for"
      className={s.picker}
      value={name}
      selectedOptions={[current?.exe ?? ALL]}
      onOptionSelect={(_, d) => onChange(d.optionValue && d.optionValue !== ALL ? d.optionValue : null)}
      button={{
        children: (
          <span className={s.pickerValue}>
            {current ? <AppIcon src={icons[current.path]} className={s.appIcon} /> : <Games20Regular className={s.appIcon} />}
            {name}
          </span>
        ),
      }}
    >
      <Option value={ALL} text="All games">
        <span className={s.pickerValue}>
          <Games20Regular className={s.appIcon} />
          All games
        </span>
      </Option>
      {apps.map((a) => (
        <Option key={a.exe} value={a.exe} text={a.name}>
          <span className={s.pickerValue}>
            <AppIcon src={icons[a.path]} className={s.appIcon} />
            {a.name}
          </span>
        </Option>
      ))}
    </Dropdown>
  );
}

/** The games the map records in. Known games join by themselves; any other is picked from what's running. */
function GameList({ prefs, onShow }: { prefs: Prefs; onShow: (exe: string) => void }) {
  const s = useStyles();
  const games = prefs.padGames;
  const icons = useIcons(games.map((g) => g.path));
  const [running, setRunning] = useState<RunningApp[]>([]);
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const options = running.filter(
    (a) => !games.some((g) => sameExe(g.exe, a.exe)) && (!q || a.name.toLowerCase().includes(q) || a.exe.toLowerCase().includes(q)),
  );

  const add = (a: RunningApp) => {
    const key = a.exe.toLowerCase();
    savePrefs({
      ...prefs,
      padGames: [...games, { exe: a.exe, name: a.name, path: a.path }],
      padGamesRemoved: prefs.padGamesRemoved.filter((e) => e !== key),
    });
    setQuery("");
  };
  const remove = (exe: string) => {
    const key = exe.toLowerCase();
    savePrefs({
      ...prefs,
      padGames: games.filter((g) => !sameExe(g.exe, exe)),
      padGamesRemoved: [...prefs.padGamesRemoved.filter((e) => e !== key), key],
    });
  };

  return (
    <div className={s.games}>
      <Body1Strong>Games</Body1Strong>
      {games.length > 0 && (
        <TagGroup aria-label="Games the map records in" className={s.tags} onDismiss={(_, d) => remove(d.value)}>
          {games.map((g) => (
            <InteractionTag key={g.exe} value={g.exe}>
              <InteractionTagPrimary
                hasSecondaryAction
                media={<AppIcon src={icons[g.path]} className={s.appIcon} />}
                title={`Show ${g.name}'s map`}
                onClick={() => onShow(g.exe.toLowerCase())}
              >
                {g.name}
              </InteractionTagPrimary>
              {/* Read with the name: "Valorant, Stop recording". */}
              <InteractionTagSecondary aria-label="Stop recording" title="Stop recording" />
            </InteractionTag>
          ))}
        </TagGroup>
      )}
      <Caption1 className={s.muted}>
        {games.length
          ? "Only these games are recorded. Known games are added automatically the first time you play them."
          : "Nothing recorded yet. Valorant, Apex and other known games are added automatically the first time you play them. Add others while they're running."}
      </Caption1>
      <Combobox
        className={s.add}
        placeholder="Add a game that's running"
        aria-label="Add a game to record"
        value={query}
        selectedOptions={[]}
        onOpenChange={(_, d) => d.open && api.runningApps().then(setRunning, () => {})}
        onChange={(e) => setQuery(e.target.value)}
        onOptionSelect={(_, d) => {
          const a = running.find((x) => x.exe === d.optionValue);
          if (a) add(a);
        }}
        listbox={{ className: s.listbox }}
      >
        {options.map((a) => (
          <Option key={a.exe} value={a.exe} text={a.name}>
            <span className={s.pickerValue}>
              <AppIcon src={a.icon} className={s.appIcon} />
              {a.name}
            </span>
          </Option>
        ))}
        {options.length === 0 && (
          <Option value="__none" disabled text="">
            {q ? "No running app matches." : "No other apps with windows are open."}
          </Option>
        )}
      </Combobox>
    </div>
  );
}

const sum = (v: number[]) => v.reduce((a, b) => a + b, 0);

/** Does the game's turning fit the pad, and which way runs out first. */
function Fit({ pad, sens, dpi, turns }: { pad: PadView; sens: GameSensitivity; dpi: number; turns: Turns }) {
  const s = useStyles();
  const cal = pad.calibration!;
  const r = Math.round;
  const { c360, c180, c90 } = turns;
  const width = cal.left + cal.right;
  const room = { left: cal.left + pad.home[0], right: cal.right - pad.home[0] };
  const short = room.right < room.left ? "right" : "left";
  const long = short === "right" ? "left" : "right";
  const lines: string[] = [];

  if (width < c180) lines.push(`A 180° turn takes ${r(c180)} cm, more than your ${r(width)} cm area, so big turns need a lift.`);
  else if (width < c360) lines.push(`Your ${r(width)} cm area fits a 180° turn (${r(c180)} cm) but not a full turn, which is the usual target.`);
  else lines.push(`Your ${r(width)} cm area fits a full turn, with ${r(width - c360)} cm to spare.`);

  if (room[short] >= c180) lines.push("A 180° turn fits either way from your spot.");
  else if (room[short] >= c90)
    lines.push(`A 90° turn fits either way, but a 180° to the ${short} runs out: you have ${r(room[short])} cm ${short} of your spot.`);
  else lines.push(`Turning ${short} runs out first: a 90° turn takes ${r(c90)} cm and you have ${r(room[short])} cm ${short} of your spot.`);

  // Only say what the edges show when there's enough of it, and it agrees.
  const hits = { left: sum(pad.edgeLeft), right: sum(pad.edgeRight) };
  if (hits.left + hits.right >= 10 && hits[short] >= 1.5 * hits[long]) {
    lines.push(
      hits[long] < 1
        ? `You've hit the ${short} edge ${r(hits[short])} times and the ${long} edge hardly at all.`
        : `You've hit the ${short} edge ${r(hits[short] / hits[long])} times as often as the ${long}.`,
    );
  }
  const shift = (room[long] - room[short]) / 2;
  if (room[short] < c180 && shift >= 2) {
    lines.push(`Resting the mouse about ${r(shift)} cm further ${long} would leave ${r((room.left + room.right) / 2)} cm each way.`);
  }

  return (
    <div className={s.fit}>
      <Body1Strong>
        {r(c360)} cm for a full turn in {sens.game}
      </Body1Strong>
      {lines.map((line) => (
        <Body1 key={line}>{line}</Body1>
      ))}
      <Caption1 className={s.muted}>
        {Number(sens.sens.toFixed(3))} sensitivity at {dpi.toLocaleString("en-US")} DPI, read from the game's settings.
        {sens.acceleration ? " Mouse acceleration is on in the game, so turns vary with how fast you move." : ""}
      </Caption1>
    </div>
  );
}

function Tiles({ pad }: { pad: PadView }) {
  const s = useStyles();
  const cal = pad.calibration!;
  // Edge hits are expected counts (sums of probabilities), so show them rounded. Lifts count the
  // pauses judged more likely lifts than not. Say so for a moment when the lift count goes up, so
  // it's easy to see the detection working.
  const lifts = Math.round(pad.lifts);
  const seen = useRef(lifts);
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    if (lifts <= seen.current) {
      seen.current = lifts;
      return;
    }
    seen.current = lifts;
    setFresh(true);
    const t = setTimeout(() => setFresh(false), 1500);
    return () => clearTimeout(t);
  }, [lifts]);
  const w = percentile(pad.reachX, 0.95);
  const h = percentile(pad.reachY, 0.95);
  const hits = Math.round([pad.edgeTop, pad.edgeBottom, pad.edgeLeft, pad.edgeRight].flat().reduce((a, b) => a + b, 0));
  const grip = Math.round(Math.abs(pad.gripDeg));
  const tiles: [string, string, string][] = [
    [
      "Swipe reach",
      w === null || h === null ? "—" : `${w} × ${h} cm`,
      w === null ? "Needs more swipes" : `95% of swipes · area is ${Math.round(cal.left + cal.right)} × ${Math.round(cal.top + cal.bottom)} cm`,
    ],
    ["Edge hits", hits.toLocaleString("en-US"), "Times you ran out of pad"],
    ["Lifts", lifts.toLocaleString("en-US"), fresh ? "Just now" : "Estimated, improves as you play"],
    ["Movement", length(pad.distanceCm), duration(pad.movingMs)],
    [
      "Accuracy",
      pad.accuracy === null ? "Learning" : `Within ${Math.max(1, Math.round(pad.accuracy))} cm`,
      pad.accuracy === null ? "Needs a few trips to the edges" : "Typical correction at an edge",
    ],
    ["Grip", grip ? `Turned ${grip}°` : "Straight", "From calibration and the edges"],
  ];
  return (
    <div className={s.tiles}>
      {tiles.map(([label, value, sub]) => (
        <div key={label} className={s.tile}>
          <Caption1 className={s.muted}>{label}</Caption1>
          <Subtitle2 className={s.value}>{value}</Subtitle2>
          <Caption1 className={s.muted}>{sub}</Caption1>
        </div>
      ))}
    </div>
  );
}

function PadMap({ pad, turns }: { pad: PadView; turns: Turns | null }) {
  const s = useStyles();
  const wrap = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [hover, setHover] = useState<{ x: number; y: number; col: number; row: number } | null>(null);
  const cal = pad.calibration!;

  useEffect(() => {
    if (!wrap.current) return;
    const ro = new ResizeObserver(([e]) => setWidth(e.contentRect.width));
    ro.observe(wrap.current);
    return () => ro.disconnect();
  }, []);

  const cell = Math.max(2, Math.min((width - MARGIN * 2) / pad.cols, MAP_MAX_HEIGHT / pad.rows));
  const padW = cell * pad.cols;
  const padH = cell * pad.rows;
  const toPx = ([x, y]: [number, number]) => [MARGIN + (x + cal.left) * cell, MARGIN + (y + cal.top) * cell];

  useEffect(() => {
    const c = canvas.current;
    if (!c || !width) return;
    const dpr = window.devicePixelRatio || 1;
    const w = padW + MARGIN * 2;
    const h = padH + MARGIN * 2;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    c.style.width = `${w}px`;
    c.style.height = `${h}px`;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(c);
    const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
    const surface = v("--colorNeutralBackground1", "#292929");
    const stroke = v("--colorNeutralStroke2", "#555");
    const ink = v("--colorNeutralForeground2", "#ccc");
    const accent = v("--colorBrandForeground1", "#479ef5");

    // The pad.
    g.fillStyle = isDark() ? "rgba(255,255,255,0.03)" : "rgba(0,0,0,0.025)";
    g.beginPath();
    g.roundRect(MARGIN, MARGIN, padW, padH, 4);
    g.fill();

    // Movement, in seven steps of a log scale: the centre gets far more than the edges.
    const colors = ramp();
    const max = Math.max(...pad.cells);
    const gap = cell >= 6 ? 1 : 0;
    if (max > 0) {
      for (let i = 0; i < pad.cells.length; i++) {
        const value = pad.cells[i];
        if (value <= 0) continue;
        const level = Math.min(colors.length - 1, Math.floor((Math.log1p(value) / Math.log1p(max)) * colors.length));
        g.fillStyle = colors[level];
        const col = i % pad.cols;
        const row = Math.floor(i / pad.cols);
        g.fillRect(MARGIN + col * cell + gap / 2, MARGIN + row * cell + gap / 2, cell - gap, cell - gap);
      }
    }

    g.strokeStyle = stroke;
    g.lineWidth = 1;
    g.beginPath();
    g.roundRect(MARGIN + 0.5, MARGIN + 0.5, padW - 1, padH - 1, 4);
    g.stroke();

    // Edge hits: marks just outside the pad, longer where you hit more often.
    const edges: [number[], (i: number, len: number) => [number, number, number, number]][] = [
      [pad.edgeTop, (i, len) => [MARGIN + (i + 0.5) * cell, MARGIN - 2, MARGIN + (i + 0.5) * cell, MARGIN - 2 - len]],
      [pad.edgeBottom, (i, len) => [MARGIN + (i + 0.5) * cell, MARGIN + padH + 2, MARGIN + (i + 0.5) * cell, MARGIN + padH + 2 + len]],
      [pad.edgeLeft, (i, len) => [MARGIN - 2, MARGIN + (i + 0.5) * cell, MARGIN - 2 - len, MARGIN + (i + 0.5) * cell]],
      [pad.edgeRight, (i, len) => [MARGIN + padW + 2, MARGIN + (i + 0.5) * cell, MARGIN + padW + 2 + len, MARGIN + (i + 0.5) * cell]],
    ];
    const most = Math.max(1, ...edges.flatMap(([hits]) => hits));
    g.strokeStyle = ink;
    g.lineWidth = Math.min(2, cell * 0.6);
    g.lineCap = "round";
    for (const [hits, at] of edges) {
      hits.forEach((n, i) => {
        if (!n) return;
        const [x1, y1, x2, y2] = at(i, 1 + Math.sqrt(n / most) * (MARGIN - 4));
        g.beginPath();
        g.moveTo(x1, y1);
        g.lineTo(x2, y2);
        g.stroke();
      });
    }

    // Where you usually put the mouse down: its learned spread as an ellipse (one standard
    // deviation), with a ring at its centre. A surface ring under each mark keeps it readable.
    const [hx, hy] = toPx(pad.home);
    const [xx, xy, yy] = pad.homeSpread;
    const mid = (xx + yy) / 2;
    const half = Math.sqrt(((xx - yy) / 2) ** 2 + xy ** 2);
    const tilt = 0.5 * Math.atan2(2 * xy, xx - yy);
    g.save();
    g.globalAlpha = 0.5;
    g.strokeStyle = ink;
    g.lineWidth = 1;
    g.beginPath();
    g.ellipse(hx, hy, Math.sqrt(mid + half) * cell, Math.sqrt(Math.max(mid - half, 0.01)) * cell, tilt, 0, Math.PI * 2);
    g.stroke();
    g.restore();
    g.lineWidth = 4;
    g.strokeStyle = surface;
    g.beginPath();
    g.arc(hx, hy, 5, 0, Math.PI * 2);
    g.stroke();
    g.lineWidth = 2;
    g.strokeStyle = ink;
    g.stroke();

    // How far 90° and 180° turns reach either side of the spot, cut off where the pad ends.
    if (turns) {
      const [left, right] = [MARGIN, MARGIN + padW];
      // Each mark goes down twice: a soft surface-coloured line under the ink, so it reads over
      // light and dark cells alike.
      const mark = (draw: () => void, dash: number[] = []) => {
        g.setLineDash([]);
        g.globalAlpha = 0.55;
        g.strokeStyle = surface;
        g.lineWidth = 3;
        g.beginPath();
        draw();
        g.stroke();
        g.setLineDash(dash);
        g.globalAlpha = 0.9;
        g.strokeStyle = ink;
        g.lineWidth = 1;
        g.beginPath();
        draw();
        g.stroke();
        g.setLineDash([]);
      };
      const label = (text: string, x: number, align: CanvasTextAlign, alpha = 0.9) => {
        g.textAlign = align;
        g.globalAlpha = alpha;
        g.lineWidth = 3;
        g.strokeStyle = surface;
        g.strokeText(text, x, hy - 5);
        g.fillText(text, x, hy - 5);
      };
      g.save();
      g.fillStyle = ink;
      g.font = `10px ${v("--fontFamilyBase", "sans-serif")}`;
      g.textBaseline = "bottom";
      mark(() => {
        g.moveTo(Math.max(left, hx - turns.c180 * cell), hy + 0.5);
        g.lineTo(Math.min(right, hx + turns.c180 * cell), hy + 0.5);
      }, [3, 3]);
      for (const side of [-1, 1]) {
        let offPad: number | null = null;
        for (const [deg, cm] of [
          [90, turns.c90],
          [180, turns.c180],
        ]) {
          const x = hx + side * cm * cell;
          if (x < left || x > right) {
            offPad ??= deg;
            continue;
          }
          // Too close to the spot to label apart from it.
          if (cm * cell < 14) continue;
          mark(() => {
            g.moveTo(x, hy - 4);
            g.lineTo(x, hy + 5);
          });
          label(`${deg}°`, x, "center");
        }
        // The first turn that runs off this side, marked where the pad ends.
        if (offPad !== null) {
          if (side > 0) label(`${offPad}° ›`, right - 3, "right", 0.7);
          else label(`‹ ${offPad}°`, left + 3, "left", 0.7);
        }
      }
      g.restore();
    }

    if (pad.tracking) {
      // The live estimate, ringed by how unsure it is.
      const [px, py] = toPx(pad.pos);
      g.save();
      g.globalAlpha = 0.35;
      g.strokeStyle = accent;
      g.lineWidth = 1;
      g.beginPath();
      g.arc(px, py, Math.max(6, pad.spread * cell), 0, Math.PI * 2);
      g.stroke();
      g.restore();
      g.fillStyle = accent;
      g.strokeStyle = surface;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(px, py, 4, 0, Math.PI * 2);
      g.fill();
      g.stroke();
    }
  }, [pad, turns, width, cell, padW, padH]);

  const onMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    const col = Math.floor((x - MARGIN) / cell);
    const row = Math.floor((y - MARGIN) / cell);
    setHover(col >= 0 && row >= 0 && col < pad.cols && row < pad.rows ? { x, y, col, row } : null);
  };

  let tip = "";
  if (hover) {
    const cx = hover.col + 0.5 - cal.left;
    const cy = hover.row + 0.5 - cal.top;
    const where = [
      Math.abs(cx) >= 1 ? `${Math.round(Math.abs(cx))} cm ${cx > 0 ? "right" : "left"}` : "",
      Math.abs(cy) >= 1 ? `${Math.round(Math.abs(cy))} cm ${cy > 0 ? "down" : "up"}` : "",
    ]
      .filter(Boolean)
      .join(", ");
    const moved = pad.cells[hover.row * pad.cols + hover.col];
    tip = `${where ? `${where} from your spot` : "Your spot"} · ${moved > 0 ? `${length(moved)} of movement` : "no movement"}`;
  }

  const colors = ramp();
  return (
    <div ref={wrap} className={s.mapWrap}>
      <canvas
        ref={canvas}
        className={s.canvas}
        role="img"
        aria-label={`Map of your ${Math.round(cal.left + cal.right)} by ${Math.round(cal.top + cal.bottom)} cm pad, shaded by how much the mouse moved in each spot`}
        onMouseMove={onMove}
        onMouseLeave={() => setHover(null)}
      />
      {hover && (
        <div className={s.tooltip} style={{ left: Math.min(hover.x + 12, Math.max(0, width - 200)), top: hover.y + 12 }}>
          <Caption1>{tip}</Caption1>
        </div>
      )}
      <div className={s.legend}>
        <span className={s.key}>
          <Caption1>Less</Caption1>
          <span className={s.scale} aria-hidden>
            {RAMP.map((_, i) => (
              <span key={i} className={s.swatch} style={{ background: colors[i] }} />
            ))}
          </span>
          <Caption1>More movement</Caption1>
        </span>
        <span className={s.key}>
          <svg width="14" height="14" aria-hidden>
            <circle cx="7" cy="7" r="5" fill="none" stroke={tokens.colorNeutralForeground2} strokeWidth="2" />
          </svg>
          <Caption1>Where you usually put it down</Caption1>
        </span>
        <span className={s.key}>
          <svg width="14" height="14" aria-hidden>
            <line x1="7" y1="2" x2="7" y2="12" stroke={tokens.colorNeutralForeground2} strokeWidth="2" strokeLinecap="round" />
          </svg>
          <Caption1>Edge hits</Caption1>
        </span>
        {turns && (
          <span className={s.key}>
            <svg width="14" height="14" aria-hidden>
              <line x1="1" y1="8.5" x2="13" y2="8.5" stroke={tokens.colorNeutralForeground2} strokeDasharray="3 3" />
              <line x1="11" y1="4" x2="11" y2="13" stroke={tokens.colorNeutralForeground2} />
            </svg>
            <Caption1>Turns from your spot</Caption1>
          </span>
        )}
        {pad.tracking && (
          <span className={s.key}>
            <svg width="14" height="14" aria-hidden>
              <circle cx="7" cy="7" r="4" fill={tokens.colorBrandForeground1} />
            </svg>
            <Caption1>Mouse now</Caption1>
          </span>
        )}
      </div>
    </div>
  );
}
