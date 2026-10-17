import React from "react";
import ReactDOM from "react-dom/client";
import { startFakeBackend } from "../fake/backend";
import { Frame, PanelTitle, type Variant } from "../Frame";
import { MOUSE_IDS, mouseName, pickPreset, PRESETS, setPlugged, setVendorApp, useWorld, type PresetId } from "./mice";
import { Shell } from "./Shell";
import "../../base.css";
import "../../states.css";
import "../../shadcn.css";

// Dev only: the app's shell with the mouse switcher, and the onboarding as a full page
// (lab-shell.html). ?s= the situation (first, one, two, mixed, away, none, add); ?plug=zaopin,atk,
// ?step= and ?vendor=1 set the world for screenshots.

startFakeBackend();

// For screenshots: ?click= clicks the element with that accessible name once the app is up.
const click = new URLSearchParams(location.search).get("click");
if (click) window.addEventListener("load", () => setTimeout(() => document.querySelector<HTMLElement>(`[aria-label="${click}"]`)?.click(), 1200));

const AS_PICKED: Variant[] = [
  {
    name: "As picked",
    Component: () => null,
    note: "The switcher, and the onboarding as a full page, centred like Windows Setup. Plug mice in and out from the panel.",
  },
];

function Panel() {
  const w = useWorld();
  return (
    <>
      <PanelTitle>Start from</PanelTitle>
      {(Object.keys(PRESETS) as PresetId[]).map((id) => (
        <label key={id} className="flex items-center gap-2">
          <input type="radio" name="preset" checked={w.preset === id} onChange={() => pickPreset(id)} />
          {PRESETS[id].label}
        </label>
      ))}
      <PanelTitle>Plugged in</PanelTitle>
      <p className="m-0">Plug a mouse in while the onboarding waits, or while the app is open: one hrtz hasn't met opens its setup.</p>
      {MOUSE_IDS.map((id) => (
        <label key={id} className="flex items-center gap-2">
          <input type="checkbox" checked={w.plugged.includes(id)} onChange={(e) => setPlugged(id, e.target.checked)} />
          {mouseName(id)}
          {id === "z2" ? " (not tested)" : id === "atk" ? " (not supported)" : ""}
        </label>
      ))}
      <label className="flex items-center gap-2">
        <input type="checkbox" checked={w.vendorApp} onChange={(e) => setVendorApp(e.target.checked)} />
        Its maker's app is running (Zaopin's app, G HUB)
      </label>
      <PanelTitle>Notes</PanelTitle>
      <p className="m-0">
        The Zaopin's pages are the real ones, on this PC's data; the others are placeholders, and the Superlight's picture is drawn until it has a render.
      </p>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Frame page="sensor" variants={AS_PICKED} panel={() => <Panel />} render={() => <Shell />} />
  </React.StrictMode>,
);
