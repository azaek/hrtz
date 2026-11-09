import { useEffect, useState } from "react";
import { Body1, Button, Caption1, makeStyles, Spinner, Title1, tokens } from "@fluentui/react-components";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, message, type Inspected } from "./api";
import { DrawnMouse } from "./DrawnMouse";
import { reportUrl } from "./report";
import { setInspecting, toast } from "./store";
import { TitleBar } from "./TitleBar";

const useStyles = makeStyles({
  frame: { display: "flex", flexDirection: "column", height: "100%" },
  page: { display: "grid", gridTemplateRows: "minmax(0, 1fr) auto", flexGrow: 1, minHeight: 0 },
  // Auto margins centre the column when it fits and let it scroll from the top when it doesn't.
  scroll: { display: "flex", flexDirection: "column", overflowY: "auto", overflowX: "hidden", scrollbarGutter: "stable both-edges" },
  column: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    textAlign: "center",
    gap: tokens.spacingVerticalM,
    width: "100%",
    maxWidth: "560px",
    boxSizing: "border-box",
    margin: "auto",
    padding: `${tokens.spacingVerticalXXL} ${tokens.spacingHorizontalL} 72px`,
  },
  footer: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    gap: tokens.spacingHorizontalM,
    padding: `${tokens.spacingVerticalL} ${tokens.spacingHorizontalXXL}`,
  },
  title: { margin: 0 },
  muted: { color: tokens.colorNeutralForeground3 },
  status: { display: "flex", alignItems: "center", gap: tokens.spacingHorizontalS, color: tokens.colorNeutralForeground3 },
  list: { display: "flex", flexDirection: "column", gap: "2px", alignSelf: "stretch", textAlign: "left" },
  row: {
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) auto",
    alignItems: "center",
    gap: tokens.spacingHorizontalL,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
  },
  rowText: { display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 },
});

const hex = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");
/** "3554:F527", as Windows and the request form write a device's IDs. */
const ids = (m: Inspected) => `${hex(m.vendorId)}:${hex(m.productId)}`;

/**
 * Inspect a mouse: for requesting support for a mouse that isn't supported yet. It lists the mice and receivers
 * Windows sees, saves one's report (what it tells Windows about itself, with nothing sent to it),
 * and opens the request form on GitHub. A full page, like the onboarding.
 */
export function Inspector() {
  const s = useStyles();
  const [mice, setMice] = useState<Inspected[] | null>(null);
  const [saved, setSaved] = useState<{ mouse: Inspected; file: string } | null>(null);

  // Looked for again every few seconds, so a mouse plugged in now turns up.
  useEffect(() => {
    let live = true;
    const look = () =>
      api.inspectMice().then(
        (m) => live && setMice(m),
        () => live && setMice([]),
      );
    void look();
    const t = setInterval(look, 3000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, []);

  const saveReport = async (m: Inspected) => {
    const name = `hrtz-inspect-${m.name.replace(/[^\w-]+/g, "-").replace(/^-|-$/g, "").toLowerCase() || ids(m).replace(":", "-")}.json`;
    const path = await saveDialog({ defaultPath: name, filters: [{ name: "Mouse report", extensions: ["json"] }] }).catch(() => null);
    if (!path) return;
    try {
      await api.saveInspection(path, m.device);
      setSaved({ mouse: m, file: path.split(/[\\/]/).pop() ?? path });
    } catch (e) {
      toast(message(e), "error");
    }
  };

  const ask = () => {
    const m = saved?.mouse;
    void reportUrl("mouse_request", null, m ? { mouse: m.name, ids: ids(m) } : {})
      .then(openUrl)
      .catch((e) => toast(message(e), "error"));
  };

  return (
    <div className={s.frame}>
      <TitleBar />
      <div className={s.page}>
        <div className={s.scroll}>
          <div className={s.column}>
            <DrawnMouse width={150} outline />
            <Title1 as="h1" className={s.title}>
              Inspect a mouse
            </Title1>
            <Body1 className={s.muted}>
              Is your mouse not supported yet? Save a report for it, then request support on GitHub and attach the report. It's what lets us add your mouse.
            </Body1>
            <Caption1 className={s.muted}>The report holds your mouse's model details. Saving it changes nothing on the mouse, and nothing leaves your PC until you attach it.</Caption1>
            {mice === null ? (
              <div className={s.status}>
                <Spinner size="extra-tiny" />
                <Caption1>Looking for mice</Caption1>
              </div>
            ) : mice.length === 0 ? (
              <Caption1 className={s.muted}>No mouse found. Plug in its receiver or cable.</Caption1>
            ) : (
              <div className={s.list} role="list" aria-label="Mice and receivers">
                {mice.map((m) => (
                  <div key={`${ids(m)}-${m.name}`} className={s.row} role="listitem">
                    <span className={s.rowText}>
                      <Body1>{m.name}</Body1>
                      <Caption1 className={s.muted}>
                        {ids(m)} · {m.supported ? `Supported (${m.supported})` : "Not supported yet"}
                      </Caption1>
                    </span>
                    <Button onClick={() => void saveReport(m)}>Save report</Button>
                  </div>
                ))}
              </div>
            )}
            <Caption1 className={s.muted} aria-live="polite">
              {saved ? `Report saved as ${saved.file}. Now request support on GitHub and attach it.` : "Keyboards can show up here too, since many include a mouse."}
            </Caption1>
          </div>
        </div>
        <div className={s.footer}>
          <Button onClick={() => setInspecting(false)}>Back</Button>
          <Button appearance="primary" onClick={ask}>
            Request support on GitHub
          </Button>
        </div>
      </div>
    </div>
  );
}
