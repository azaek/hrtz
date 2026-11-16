import React, { useState, type ComponentType } from "react";
import ReactDOM from "react-dom/client";
import type { PageId } from "@/App";
import { BatteryPage } from "@/pages/Battery";
import { WeekPage } from "@/pages/Week";
import { ButtonsPage } from "@/pages/Buttons";
import { HudPage } from "@/pages/Hud";
import { LabPage } from "@/pages/Lab";
import { ProfilesPage } from "@/pages/Profiles";
import { SensorPage } from "@/pages/Sensor";
import { SettingsPage } from "@/pages/SettingsPage";
import { OPEN_WHATS_NEW } from "@/WhatsNew";
import { pickMouse, setInspecting, startFlow, type Step } from "@/store";
import { env, fake, inspectLab, plugInto, pushDevice, startFakeBackend, wakeWaiting } from "../fake/backend";
import { Frame, PanelButton, PanelTitle } from "../Frame";
import "../../base.css";
import "../../states.css";
import "../../shadcn.css";

// Dev only: the real app in a browser tab, on the stand-in backend with this PC's data, to look
// at a page as built without touching the running app. ?page=lab (or any page id) opens there.
//
// For screenshots: ?test= opens a Lab test, ?play= plays a scenario once it's up (conn, conn2,
// tracking, liftoff, pair), ?scroll= scrolls the page, and ?far, ?spin, ?drift, ?behind, ?port=hub set
// the world first (?clean: a radio that carries every report).
//
// ?inspect opens Inspect a mouse with one of each kind of mouse plugged in beside this PC's, to
// walk every path of its check without a mouse: "2.4G Wireless Receiver" (another maker's mouse
// hrtz leaves alone but can read), "Zaopin 1K Receiver" (its mouse hasn't answered; woken, it's a
// Z11) and "USB Gaming Mouse" (known only through Windows). The panel sets what each does, and
// the GitHub button really opens the form, filled in, in a new tab. ?unknown makes this PC's own
// mouse a model hrtz doesn't know, for the check of a mouse it runs without knowing.

startFakeBackend();

const PAGES: Record<PageId, ComponentType> = {
  sensor: SensorPage,
  buttons: ButtonsPage,
  profiles: ProfilesPage,
  battery: BatteryPage,
  week: WeekPage,
  hud: HudPage,
  lab: LabPage,
  settings: SettingsPage,
};
const params = new URLSearchParams(location.search);
const page = (params.get("page") as PageId) ?? "lab";
const start = page in PAGES ? page : "lab";

if (params.get("test")) localStorage.setItem("hrtz.labOpen", params.get("test")!);
for (const key of ["far", "spin", "drift", "behind"] as const) if (params.has(key)) env[key] = true;
if (params.has("clean")) env.merged = false;
if (params.get("port") === "hub") plugInto(4, true);
if (params.has("inspect")) inspectLab.on = true;
if (params.has("unknown")) inspectLab.unknown = true;

type Lab = Record<string, (...a: unknown[]) => unknown>;
const lab = () => (window as unknown as { lab: Lab }).lab;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
/** Play `samples` a tick's worth at a time until the page says `text`: a step that moves on by
 *  itself takes as long as it takes. */
const until = async (text: string, samples: () => unknown) => {
  for (let i = 0; i < 300 && !document.body.textContent?.includes(text); i++) {
    lab().play(samples());
    await wait(100);
  }
};
const click = (text: string) => {
  const b = [...document.querySelectorAll("button")].find((x) => x.textContent === text);
  b?.click();
  return !!b;
};
const SCENARIOS: Record<string, () => Promise<void>> = {
  /** The mouse switcher's list open (with `many` for a long one). */
  switcher: async () => {
    document.querySelector<HTMLElement>('[aria-label="Switch mouse"]')?.click();
    await wait(600);
  },
  /** A connection check in game mode, stepping down a rate if it needs to. */
  conn: async () => {
    click("Start");
    await wait(300);
    lab().circlesAlong();
    await wait(5000);
  },
  /** A check, then the receiver moved to another port and checked again. */
  conn2: async () => {
    await SCENARIOS.conn();
    lab().plugInto(3, false);
    await wait(500);
    click("Check again");
    await wait(300);
    lab().circlesAlong();
    await wait(5000);
  },
  /** The tracking check in game mode: let go, a hand resting, then swipes. */
  tracking: async () => {
    click("Start");
    await wait(300);
    await until("Part 2 of 3", () => lab().stillFor(1));
    await until("Part 3 of 3", () => lab().stillFor(1, true));
    lab().play(lab().swipes());
    await wait(1500);
  },
  /** Settings: pairing a mouse, a couple of seconds in. */
  pair: async () => {
    click("Pair");
    await wait(2500);
  },
  /** The whole lift-off comparison. */
  liftoff: async () => {
    click("Start");
    await wait(300);
    lab().liftOffAlong();
    await wait(14_000);
  },
};
// Settings: a backup under way, its progress under the card (with ?scroll= to see it).
SCENARIOS.backup = async () => {
  (lab() as unknown as { fake: { flashMs: number } }).fake.flashMs = 20_000;
  click("Back up now");
  await wait(3000);
};
// Clicks "Check for updates", which dev builds don't do by themselves (with ?update=…).
SCENARIOS.update = async () => {
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("Check for updates"))?.click();
};
// The "Not tested" badge: opens Settings at Help test (with ?mouse=superlight&untested).
SCENARIOS.badge = async () => {
  document.querySelector<HTMLElement>('[aria-label^="hrtz hasn"]')?.click();
  await wait(700);
};
/** Plays a mouse's check along, once it's started: circles, then each button it can see pressed in
 *  turn. */
const playAlong = async () => {
  lab().circlesAlong();
  await wait(14000);
  const press = (b: number) => ({ buttons: [{ button: b, down: true, tUs: 0, dx: 0, dy: 0 }, { button: b, down: false, tUs: 0, dx: 0, dy: 0 }] });
  for (const b of [0, 1, 2, 3, 4]) {
    lab().play([press(b)]);
    await wait(800);
  }
};
// A mouse's check from start to finish, in Inspect a mouse with the mouse picked (with ?inspect,
// and ?mouse=superlight&untested for a model not tested yet).
SCENARIOS.participate = async () => {
  click("Start the check");
  await wait(500);
  await playAlong();
  await wait(3000);
};
// Removes the mouse shown, which isn't connected (with ?select=<its key>).
SCENARIOS.remove = async () => {
  click("Remove");
  await wait(600);
};
window.addEventListener("load", async () => {
  await wait(1200);
  // `?select=<key>`: show that mouse, as the switcher would.
  const select = params.get("select");
  if (select) {
    pickMouse(select);
    await wait(300);
  }
  const play = params.get("play");
  if (play && SCENARIOS[play]) await SCENARIOS[play]();
  const scroll = Number(params.get("scroll"));
  if (scroll) {
    // The page's own scroller: the tallest that scrolls.
    const sc = [...document.querySelectorAll<HTMLElement>("*")]
      .filter((e) => e.scrollHeight > e.clientHeight + 20 && /auto|scroll/.test(getComputedStyle(e).overflowY))
      .sort((a, b) => b.scrollHeight - a.scrollHeight)[0];
    if (sc) sc.scrollTop = scroll;
  }
  // ?away: the mouse unplugged, so its pages show its last settings.
  if (params.has("away")) pushDevice({ connected: false, online: false, battery: null });
  // ?click= clicks the element with that accessible name: "Switch mouse" opens the switcher.
  const clickOn = params.get("click");
  if (clickOn) document.querySelector<HTMLElement>(`[aria-label="${clickOn}"]`)?.click();
  // A window over the page: ?dialog=whats-new.
  const dialog = params.get("dialog");
  if (dialog === "whats-new") window.dispatchEvent(new Event(OPEN_WHATS_NEW));
  // The onboarding: ?flow=first (from its welcome) or ?flow=add (connecting a new mouse), and
  // ?step= for one of its steps.
  const flow = params.get("flow");
  if (flow === "first" || flow === "add") startFlow(flow, (params.get("step") as Step | null) ?? undefined);
  if (params.has("inspect")) setInspecting(true);
});

const WORLD: Record<keyof typeof env, string> = {
  merged: "Radio carries about 3,600 a second (merges the rest)",
  far: "Receiver far from the mouse (interference)",
  spin: "Sensor spins out over 3 m/s",
  drift: "Sensor creeps at rest",
  behind: "hrtz behind another window",
};

function Panel() {
  const [, redraw] = useState(0);
  const [asleep, setAsleep] = useState(false);
  const port = fake.device.port?.path.match(/USB\((\d+)\)/)?.[1];
  return (
    <>
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={asleep} onChange={(e) => (setAsleep(e.target.checked), pushDevice({ online: !e.target.checked }))} /> Mouse asleep
      </label>
      {inspectLab.on && (
        <>
          <PanelTitle>Inspect a mouse</PanelTitle>
          <PanelButton onClick={() => void playAlong()}>Play along with this PC's mouse's check</PanelButton>
          <p className="m-0">Start its check first. The stand-in has no full screen to show; Esc stops it.</p>
          <PanelButton onClick={() => lab().circlesAlong()}>Quick circles while a mouse is measured</PanelButton>
          <p className="m-0">The other maker's mouse is measured after it's read, and again when a change is tried. Its toggles below count from its next check.</p>
          {(
            [
              ["foreignAsleep", "2.4G Wireless Receiver: its mouse is asleep"],
              ["foreignStops", "2.4G Wireless Receiver: its mouse falls asleep while read"],
              ["foreignOneWay", "…its DPI stages read one way only"],
              ["foreignListed", "…its maker's own list names it (nothing more to ask)"],
              ["foreignElsewhere", "…its settings are kept where hrtz doesn't look"],
              ["foreignForgets", "…it doesn't keep a change tried on it"],
            ] as const
          ).map(([key, label]) => (
            <label key={key} className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={inspectLab[key]}
                onChange={(e) => {
                  inspectLab[key] = e.target.checked;
                  redraw((n) => n + 1);
                }}
              />
              {label}
            </label>
          ))}
          <PanelButton onClick={() => (wakeWaiting(), redraw((n) => n + 1))}>
            {inspectLab.woken ? "Zaopin 1K Receiver: woken, a Z11" : "Zaopin 1K Receiver: wake its mouse (a Z11)"}
          </PanelButton>
        </>
      )}
      <PanelTitle>The world around the mouse</PanelTitle>
      {(Object.keys(WORLD) as (keyof typeof env)[]).map((key) => (
        <label key={key} className="flex items-center gap-2">
          <input
            type="checkbox"
            checked={env[key]}
            onChange={(e) => {
              env[key] = e.target.checked;
              redraw((n) => n + 1);
            }}
          />
          {WORLD[key]}
        </label>
      ))}
      <p className="m-0">
        Receiver in {port ? `port ${port}` : "?"}
        {fake.device.port?.hubs ? ", through a hub" : ""}.
      </p>
      <div className="grid grid-cols-3 gap-1.5">
        <PanelButton onClick={() => (plugInto(11, false), redraw((n) => n + 1))}>Port 11</PanelButton>
        <PanelButton onClick={() => (plugInto(3, false), redraw((n) => n + 1))}>Port 3</PanelButton>
        <PanelButton onClick={() => (plugInto(4, true), redraw((n) => n + 1))}>A hub</PanelButton>
      </div>
      <PanelTitle>Make something happen</PanelTitle>
      <p className="m-0">
        Goes to whichever test is listening, ten times faster than life. Real clicks, the wheel and pointer movement over this page count too: quick circles make connection
        runs, long drags make swipes.
      </p>
      <div className="grid grid-cols-1 gap-1.5">
        <PanelButton onClick={() => lab().play(lab().movement(10))}>Ten seconds of quick circles</PanelButton>
        <PanelButton onClick={() => lab().circlesAlong()}>Quick circles while game mode is up</PanelButton>
        <PanelButton onClick={() => lab().play(lab().dpiRuns())}>Six DPI runs (reads 1.2% high)</PanelButton>
        <PanelButton onClick={() => lab().play(lab().stillFor(12))}>Let go for 12 s</PanelButton>
        <PanelButton onClick={() => lab().play(lab().stillFor(12, true))}>Rest a hand on it for 12 s</PanelButton>
        <PanelButton onClick={() => lab().play(lab().swipes())}>Eight swipes between stops, faster each time</PanelButton>
        <PanelButton onClick={() => lab().liftOffAlong()}>Play along with the lift-off test</PanelButton>
        <PanelButton onClick={() => lab().play(lab().doubleClick())}>Twelve clicks, one double</PanelButton>
        <PanelButton onClick={() => lab().play(lab().scrollNotches())}>Forty notches, one wrong way</PanelButton>
      </div>
      <p className="m-0">Backups: {fake.backups.length} on the stand-in (this PC's, read at load).</p>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Frame page={start} variants={[{ name: "As built", Component: PAGES[start], note: "The real page, on the stand-in backend with this PC's data." }]} panel={() => <Panel />} />
  </React.StrictMode>,
);
