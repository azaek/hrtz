import { useEffect, useState } from "react";
import { Body1, Button, Caption1, makeStyles, Spinner, Title1, tokens } from "@fluentui/react-components";
import { api, type Inspected } from "./api";
import { idsOf, MouseCheck } from "./cards/participate/MouseCheck";
import { artOf, SUPPORT_TEXT, supportOf } from "./models";
import { MousePicture, MousePlaceholder } from "./MousePicture";
import { pickMouse, setInspecting, useStore } from "./store";
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
    gridTemplateColumns: "auto minmax(0, 1fr) auto",
    alignItems: "center",
    gap: tokens.spacingHorizontalL,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    borderRadius: tokens.borderRadiusMedium,
    backgroundColor: "var(--card)",
    boxShadow: "0 0 0 1px var(--card-stroke)",
  },
  rowArt: { color: tokens.colorNeutralForeground3 },
  rowText: { display: "flex", flexDirection: "column", gap: "2px", minWidth: 0 },
});

/** A listed mouse, the same across the list being looked for again. */
const idOf = (m: Inspected) => `${idsOf(m)}-${m.name}-${m.key ?? ""}`;

/**
 * Inspect a mouse: the one place to check a mouse and send what hrtz learns of it, for a mouse
 * that isn't supported yet or a model that isn't tested yet. It lists the mice and receivers
 * Windows sees, each with how far hrtz is with it; picking one opens its check (`MouseCheck`). A
 * full page, like the onboarding. Its "Not tested" and "Not supported yet" badges, and Settings,
 * open it at that mouse.
 */
export function Inspector() {
  const s = useStyles();
  const supported = useStore((x) => x.supported);
  const openedFor = useStore((x) => x.inspectKey);
  const [mice, setMice] = useState<Inspected[] | null>(null);
  const [picked, setPicked] = useState<Inspected | null>(null);
  const device = useStore((x) => (picked?.key ? (x.devices[picked.key] ?? null) : null));

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

  const pick = (m: Inspected) => {
    // The check runs on the mouse the window shows.
    if (m.key) pickMouse(m.key);
    setPicked(m);
  };

  // Opened for a mouse: it's picked as soon as it's listed, once (Back then shows the list).
  useEffect(() => {
    if (!openedFor || picked || !mice) return;
    const m = mice.find((x) => x.key === openedFor);
    if (!m) return;
    pick(m);
    setInspecting(true);
  }, [mice, openedFor]);

  // The picked mouse as listed now, for what Windows says of it.
  const shown = picked ? (mice?.find((x) => idOf(x) === idOf(picked)) ?? picked) : null;

  return (
    <div className={s.frame}>
      <TitleBar />
      <div className={s.page}>
        <div className={s.scroll}>
          <div className={s.column}>
            {shown ? (
              <MouseCheck key={idOf(shown)} mouse={shown} support={supportOf(shown.key, supported)} device={device} />
            ) : (
              <>
                <MousePlaceholder width={150} />
                <Title1 as="h1" className={s.title}>
                  Inspect a mouse
                </Title1>
                <Body1 className={s.muted}>
                  Is your mouse not supported or not tested yet? Check it here and send us the report. It's what lets us add your mouse.
                </Body1>
                {mice === null ? (
                  <div className={s.status}>
                    <Spinner size="extra-tiny" />
                    <Caption1>Looking for mice</Caption1>
                  </div>
                ) : mice.length === 0 ? (
                  <Caption1 className={s.muted}>No mouse found. Plug in its receiver or cable.</Caption1>
                ) : (
                  <div className={s.list} role="list" aria-label="Mice and receivers">
                    {mice.map((m) => {
                      const support = supportOf(m.key, supported);
                      return (
                        <div key={idOf(m)} className={s.row} role="listitem">
                          <span className={s.rowArt}>
                            <MousePicture image={artOf(m.key)?.image} width={48} />
                          </span>
                          <span className={s.rowText}>
                            <Body1>{m.name}</Body1>
                            <Caption1 className={s.muted}>
                              {idsOf(m)} · {support ? SUPPORT_TEXT[support] : "Not supported yet"}
                            </Caption1>
                          </span>
                          <Button onClick={() => pick(m)}>Check</Button>
                        </div>
                      );
                    })}
                  </div>
                )}
                <Caption1 className={s.muted}>Keyboards can show up here too, since many include a mouse.</Caption1>
              </>
            )}
          </div>
        </div>
        <div className={s.footer}>
          <Button onClick={() => (shown ? setPicked(null) : setInspecting(false))}>Back</Button>
        </div>
      </div>
    </div>
  );
}
