import React from "react";
import ReactDOM from "react-dom/client";
import { SensorPage } from "@/pages/Sensor";
import { openTest } from "@/cards/lab/tests";
import { setDiscovery, setWeekMomentAlways, type Discovery } from "@/discover";
import { goTo } from "@/store";
import { fake, pushPrefs, startFakeBackend } from "../fake/backend";
import { Frame, PanelButton, PanelTitle, type Variant } from "../Frame";
import "../../base.css";
import "../../states.css";
import "../../shadcn.css";

// Throwaway, like the labs: how the three new features are found. The features themselves are the
// same in every direction (the blind test in Lab, Your week as a page, the games card on Sensor,
// each with its "New" badge until seen); the directions differ in what else invites you in. On the
// stand-in backend with this PC's data, two weeks of made-up activity, and CS2 at 1.25.

startFakeBackend();

const NEW = ["blindtest", "week", "gamesens"];

/** Each direction starts as a first look: badges unseen, nothing hidden. */
function fresh(mode: Discovery) {
  setDiscovery(mode);
  setWeekMomentAlways(true);
  localStorage.removeItem("hrtz.newThingsHidden");
  localStorage.removeItem("hrtz.weekShown");
  pushPrefs({ ...fake.prefs, seen: (fake.prefs.seen ?? []).filter((id) => !NEW.includes(id)) });
}

// A page component each, so switching directions mounts the page afresh.
const Badges = () => <SensorPage />;
const FrontDoor = () => <SensorPage />;
const Moments = () => <SensorPage />;

const variants: Variant[] = [
  {
    name: "Badges",
    Component: Badges,
    note: "Each feature where it belongs, with its New badge until seen: Your week in the sidebar, the blind test first in Lab, the games card under the DPI stages. Nothing else asks for attention.",
    enter: () => fresh("badges"),
  },
  {
    name: "Front door",
    Component: FrontDoor,
    note: "\"New in hrtz\" at the top of the Sensor page, the page hrtz opens on: the three, each a button away, until hidden. Plus the badges.",
    enter: () => fresh("front"),
  },
  {
    name: "Moments",
    Component: Moments,
    note: "Each at the moment it matters: Your week pops up once a week when there's a week to show; the polling rate card asks \"Can you feel 8,000 Hz?\"; the games card sits under the DPI stages. Plus the badges.",
    enter: () => fresh("moments"),
  },
];

function Panel() {
  return (
    <>
      <PanelTitle>Try each feature</PanelTitle>
      <div className="grid grid-cols-1 gap-1.5">
        <PanelButton onClick={() => openTest("blind")}>Open the blind test (Lab)</PanelButton>
        <PanelButton onClick={() => goTo("week")}>Open Your week</PanelButton>
        <PanelButton onClick={() => goTo("sensor")}>Back to Sensor</PanelButton>
        <PanelButton onClick={() => pushPrefs({ ...fake.prefs, seen: (fake.prefs.seen ?? []).filter((id) => !NEW.includes(id)) })}>Show the New badges again</PanelButton>
      </div>
      <p className="m-0">
        The blind test runs here too: it switches the stand-in mouse, and the dot follows real time. Your week shows two made-up weeks; the games card reads Valorant and Apex
        from this PC and CS2 at 1.25.
      </p>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Frame page="sensor" variants={variants} panel={() => <Panel />} />
  </React.StrictMode>,
);
