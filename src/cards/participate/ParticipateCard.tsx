import { useState } from "react";
import { Button, Caption1, Link } from "@fluentui/react-components";
import { CheckmarkCircle16Filled, Circle16Regular, DismissCircle16Filled, PersonFeedback20Regular } from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, message, type DeviceState } from "@/api";
import { SettingCard } from "@/components";
import { StepCard } from "@/kit";
import { reportUrl } from "@/report";
import { toast } from "@/store";
import { buildReport, reportName, summaryLines, summaryText } from "./report";
import { useParticipate } from "./useParticipate";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

/** What each stage asks of you, for the card while the game screen is up. */
const DOING: Record<string, string> = {
  reading: "Reading the mouse",
  moving: "Move the mouse in quick circles",
  buttons: "Press each button when asked",
  writing: "Checking a change",
};

/**
 * "Help test your …", for a mouse hrtz lists but hasn't been tried on: a check on game mode's
 * screen (`useParticipate`) that reads the mouse, measures its polling, has each button pressed,
 * and changes one setting and puts it back; then what it found, the report saved as a file, and a
 * GitHub issue to send it in, filled in.
 */
export function ParticipateCard({ device }: { device: DeviceState }) {
  const p = useParticipate();
  const [saved, setSaved] = useState<string | null>(null);
  const running = p.stage !== "idle" && p.stage !== "done";

  const save = async () => {
    if (!p.checks) return;
    const path = await saveDialog({ defaultPath: reportName(device), filters: [{ name: "Mouse report", extensions: ["json"] }] });
    if (!path) return;
    try {
      const report = await buildReport(device, p.checks, p.dump);
      await api.saveReport(path, JSON.stringify(report, null, 2));
      setSaved(path);
    } catch (e) {
      toast(message(e), "error");
    }
  };
  const send = async () => {
    if (!p.checks) return;
    const url = await reportUrl("mouse_report", device, { check: summaryText(p.checks) });
    await openUrl(url).catch((e) => toast(message(e), "error"));
  };
  const start = () => {
    setSaved(null);
    void p.start();
  };

  const description = running
    ? "Running full screen. Press Esc to stop; what it changed goes back."
    : p.stage === "done"
      ? "Done. Save the report, then send it in a GitHub issue."
      : p.stopped
        ? "Stopped. Your settings are as they were."
      : "A 2-minute check of what works on this model, saved as a report for GitHub. It changes one DPI stage and puts it back.";

  return (
    <SettingCard
      stacked={running || p.stage === "done"}
      icon={<PersonFeedback20Regular />}
      title={`Help test your ${device.model}`}
      description={description}
      below={
        running ? (
          <div className="pl-9">
            <StepCard
              label="Help test"
              title={DOING[p.stage]}
              status={<span style={soft}>Follow the full screen.</span>}
              actions={
                <Link as="button" onClick={p.stop}>
                  Stop
                </Link>
              }
            />
          </div>
        ) : p.stage === "done" && p.checks ? (
          <div className="tw flex flex-col gap-3 pl-9">
            <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
              {summaryLines(p.checks).map((l, i) => (
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
            <div className="flex flex-wrap items-center gap-2">
              <Button appearance="primary" onClick={() => void save()}>
                Save report…
              </Button>
              <Button disabled={!saved} onClick={() => void send()}>
                Open GitHub issue
              </Button>
              <Link as="button" onClick={start}>
                Check again
              </Link>
            </div>
            <Caption1 style={muted}>
              {saved
                ? `Saved to ${saved}. Drag the file into the GitHub issue.`
                : "The report holds the mouse's settings and memory, and what this check found; nothing that identifies you or this PC. It stays here until you send it."}
            </Caption1>
          </div>
        ) : undefined
      }
    >
      {!running && p.stage !== "done" && (
        <Button disabled={!device.online} onClick={start}>
          Start
        </Button>
      )}
    </SettingCard>
  );
}
