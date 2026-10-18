import React, { useState, type ComponentType } from "react";
import ReactDOM from "react-dom/client";
import type { PageId } from "@/App";
import { BatteryPage } from "@/pages/Battery";
import { ButtonsPage } from "@/pages/Buttons";
import { HudPage } from "@/pages/Hud";
import { LabPage } from "@/pages/Lab";
import { ProfilesPage } from "@/pages/Profiles";
import { SensorPage } from "@/pages/Sensor";
import { SettingsPage } from "@/pages/SettingsPage";
import { OPEN_WHATS_NEW } from "@/WhatsNew";
import { startFlow, type Step } from "@/store";
import { env, fake, plugInto, pushDevice, startFakeBackend } from "../fake/backend";
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

startFakeBackend();

const PAGES: Record<PageId, ComponentType> = {
  sensor: SensorPage,
  buttons: ButtonsPage,
  profiles: ProfilesPage,
  battery: BatteryPage,
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
window.addEventListener("load", async () => {
  await wait(1200);
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
