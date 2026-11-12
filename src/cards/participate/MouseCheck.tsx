import { useState } from "react";
import { Body1, Button, Caption1, Link, Title1 } from "@fluentui/react-components";
import { CheckmarkCircle16Filled, Circle16Regular, DismissCircle16Filled } from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, message, type DeviceState, type Inspected } from "@/api";
import { StepCard } from "@/kit";
import { linkName } from "@/links";
import { artOf, SUPPORT_TEXT, type Support } from "@/models";
import { MousePicture } from "@/MousePicture";
import { reportUrl } from "@/report";
import { toast } from "@/store";
import { buildReport, reportName, summaryLines, summaryText, type Line } from "./report";
import { useParticipate } from "./useParticipate";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

/** What each stage asks of you, while the full screen is up. */
const DOING: Record<string, string> = {
  reading: "Reading the mouse",
  moving: "Move the mouse in quick circles",
  buttons: "Press each button when asked",
  writing: "Checking a change",
};

const hex = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");
/** "3554:F527", as Windows and the request form write a device's IDs; a simulated mouse has none. */
export const idsOf = (m: Inspected) => (m.vendorId || m.productId ? `${hex(m.vendorId)}:${hex(m.productId)}` : "Simulated");

/** What the check does, by how far hrtz is with the mouse. */
const ABOUT: Record<Support | "windows", string> = {
  windows: "hrtz doesn't run this mouse yet, so its report holds what it tells Windows: its IDs and how it connects. Nothing is sent to it.",
  unknown:
    "hrtz runs this mouse but doesn't know the model yet. The check reads its settings and memory, measures its polling rate and asks you to press each button. It changes nothing.",
  untested: "A 2-minute check of what works on this model. It reads the mouse, measures its polling rate, asks you to press each button, and changes one DPI stage and puts it back.",
  tested: "This model is tested. If something looks wrong, check it and send the report.",
};

/** What Windows says of a mouse hrtz doesn't run, a line each, for the result list. */
function windowsLines(m: Inspected): Line[] {
  const interfaces = (m.device as { interfaces?: unknown[] } | null)?.interfaces?.length ?? 0;
  return [
    { ok: true, text: `IDs ${idsOf(m)}` },
    { ok: interfaces > 0, text: interfaces ? `${interfaces} interface${interfaces > 1 ? "s" : ""}, with what each reports` : "No interfaces read" },
    { ok: null, text: "Nothing is sent to it until hrtz supports it" },
  ];
}

function Results({ lines }: { lines: Line[] }) {
  return (
    <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-left">
      {lines.map((l, i) => (
        <li key={i} className="flex items-center gap-2 text-sm">
          {l.ok === null ? (
            <Circle16Regular style={muted} />
          ) : l.ok ? (
            <CheckmarkCircle16Filled style={{ color: "var(--colorPaletteGreenForeground1)" }} />
          ) : (
            <DismissCircle16Filled style={{ color: "var(--colorPaletteDarkOrangeForeground1)" }} />
          )}
          <span>{l.text}</span>
        </li>
      ))}
    </ul>
  );
}

/**
 * Inspect a mouse, for the mouse picked: one check that gathers everything hrtz can safely learn of
 * it, then one report to save and one GitHub issue to send it in. A mouse hrtz doesn't run gives
 * what it tells Windows. One it runs is read, its polling measured and each button pressed, on the
 * full screen; a model it knows also has one DPI stage changed and put back.
 */
export function MouseCheck({ mouse, support, device }: { mouse: Inspected; support: Support | undefined; device: DeviceState | null }) {
  const p = useParticipate();
  const [saved, setSaved] = useState<string | null>(null);
  const runs = !!support && !!device?.connected;
  const kind = runs ? support : "windows";
  const writes = kind === "untested" || kind === "tested";
  const running = p.stage !== "idle" && p.stage !== "done";
  const done = runs ? p.stage === "done" && !!p.checks : true;

  const save = async () => {
    const name = runs && device ? reportName(device) : `hrtz-inspect-${mouse.name.replace(/[^\w-]+/g, "-").replace(/^-|-$/g, "").toLowerCase() || "mouse"}.json`;
    const path = await saveDialog({ defaultPath: name, filters: [{ name: "Mouse report", extensions: ["json"] }] }).catch(() => null);
    if (!path) return;
    try {
      if (runs && device && p.checks) {
        const report = { ...(await buildReport(device, p.checks, p.dump)), inspection: mouse.device };
        await api.saveReport(path, JSON.stringify(report, null, 2));
      } else await api.saveInspection(path, mouse.device);
      setSaved(path.split(/[\\/]/).pop() ?? path);
    } catch (e) {
      toast(message(e), "error");
    }
  };

  // A model hrtz knows goes in as a test report; anything else as a request for support.
  const send = async () => {
    const check = p.checks ? summaryText(p.checks) : "";
    const url =
      writes && device
        ? await reportUrl("mouse_report", device, { check })
        : await reportUrl("mouse_request", null, { mouse: mouse.name, ids: idsOf(mouse), connection: device?.connected ? linkName(device) : "", check });
    await openUrl(url).catch((e) => toast(message(e), "error"));
  };

  const start = () => {
    setSaved(null);
    void p.start({ write: writes });
  };

  const lines = runs ? (p.checks ? summaryLines(p.checks) : []) : windowsLines(mouse);

  return (
    <div className="tw flex w-full flex-col items-center gap-3 text-center">
      <MousePicture image={artOf(mouse.key, device?.variant)?.image} width={200} />
      <Title1 as="h1" style={{ margin: 0 }}>
        {mouse.name}
      </Title1>
      <Caption1 style={muted}>
        {idsOf(mouse)} · {support ? SUPPORT_TEXT[support] : "Not supported yet"}
      </Caption1>
      <Body1 style={soft}>
        {running
          ? "Running full screen. Press Esc to stop; what it changed goes back."
          : p.stopped && !done
            ? "Stopped. Your settings are as they were."
            : ABOUT[kind]}
      </Body1>

      {running ? (
        <div className="w-full text-left">
          <StepCard
            label="Checking"
            title={DOING[p.stage]}
            status={<span style={soft}>Follow the full screen.</span>}
            actions={
              <Link as="button" onClick={p.stop}>
                Stop
              </Link>
            }
          />
        </div>
      ) : done ? (
        <div className="flex w-full flex-col items-center gap-3">
          <div className="w-full rounded-[var(--borderRadiusMedium)] border px-4 py-3">
            <Results lines={lines} />
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button appearance="primary" onClick={() => void save()}>
              Save report…
            </Button>
            <Button disabled={!saved} onClick={() => void send()}>
              {writes ? "Open GitHub issue" : "Request support on GitHub"}
            </Button>
            {runs && (
              <Link as="button" onClick={start}>
                Check again
              </Link>
            )}
          </div>
          <Caption1 style={muted}>
            {saved
              ? `Saved as ${saved}. Drag the file into the GitHub issue.`
              : "The report holds what the check found and the mouse's details; nothing that identifies you or this PC. It stays here until you send it."}
          </Caption1>
        </div>
      ) : (
        <Button appearance="primary" disabled={!device?.online} onClick={start}>
          Start the check
        </Button>
      )}
      {runs && !running && !done && !device?.online && <Caption1 style={muted}>Wake the mouse to check it.</Caption1>}
    </div>
  );
}
