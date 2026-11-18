import { useEffect, useRef, useState } from "react";
import { Body1, Button, Caption1, Link, makeStyles, Spinner, Title1, tokens } from "@fluentui/react-components";
import { api, type Inspected } from "./api";
import { Confirm } from "./cards/participate/Confirm";
import { MouseCheck } from "./cards/participate/MouseCheck";
import { asks, idOf, idsOf, nameOf, statusOf, windowsPart, type Part } from "./cards/participate/report";
import { ReportView } from "./cards/participate/ReportView";
import { t } from "./i18n";
import { artOf, supportOf } from "./models";
import { MousePicture, MousePlaceholder } from "./MousePicture";
import { pickMouse, setInspecting, toast, useStore } from "./store";
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

/** How long after a mouse is plugged in hrtz may still be meeting it: the hub looks every 1.5 s,
 *  then the mouse is asked who it is, then the list is read again. */
const SETTLES_MS = 5000;

/**
 * Inspect a mouse: the one place to check a mouse and send what hrtz learns of it, for a mouse
 * that isn't supported yet or a model that isn't tested yet. A full page, like the onboarding.
 * Its "Not tested" and "Not supported yet" badges, and Settings, open it at that mouse.
 *
 * The mice and receivers Windows sees, each with how far hrtz is with it; the check of the one
 * picked (`MouseCheck`); for a mouse hrtz doesn't know, a few questions about what it read
 * (`Confirm`); and its report (`ReportView`), to send. A mouse with a receiver and a cable is
 * checked on each, into the one report: from the report, the list comes back to pick its other
 * connection.
 */
export function Inspector() {
  const s = useStyles();
  const supported = useStore((x) => x.supported);
  const openedFor = useStore((x) => x.inspectKey);
  const [mice, setMice] = useState<Inspected[] | null>(null);
  const [picked, setPicked] = useState<Inspected | null>(null);
  const [view, setView] = useState<"list" | "check" | "confirm" | "report">("list");
  /** The part its owner is asked about. */
  const [asking, setAsking] = useState<string | null>(null);
  /** The report so far: a part per connection checked. */
  const [parts, setParts] = useState<Part[]>([]);
  /** The list is up to pick the mouse's other connection, for the same report. */
  const [adding, setAdding] = useState(false);
  /** What its owner calls the mouse, where hrtz can't name it. */
  const [typed, setTyped] = useState("");

  // The picked mouse as listed now: how far hrtz is with it changes as it wakes.
  const shown = picked ? (mice?.find((x) => idOf(x) === idOf(picked)) ?? picked) : null;
  const device = useStore((x) => (shown?.key ? (x.devices[shown.key] ?? null) : null));

  // When each mouse was first listed: one just plugged in may not have been met by hrtz yet, so
  // how far hrtz gets with it isn't settled. The ones there as Inspect opens count as old.
  const firstListed = useRef<Map<string, number> | null>(null);
  const settled = (m: Inspected) => Date.now() - (firstListed.current?.get(idOf(m)) ?? 0) >= SETTLES_MS;
  const note = (list: Inspected[]) => {
    const first = !firstListed.current;
    firstListed.current ??= new Map();
    for (const m of list) if (!firstListed.current.has(idOf(m))) firstListed.current.set(idOf(m), first ? 0 : Date.now());
    setMice(list);
  };

  // Looked for again every few seconds, so a mouse plugged in now turns up; every second while
  // the one picked is waited for (asleep, or not settled), so its check is there as it wakes.
  const often = view === "check" && (shown?.reach === "asleep" || shown?.reach === "windows" || (shown?.reach === "runs" && !device?.online));
  useEffect(() => {
    let live = true;
    const look = () =>
      api.inspectMice().then(
        (m) => live && note(m),
        () => live && setMice([]),
      );
    void look();
    const t = setInterval(look, often ? 1000 : 3000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [often]);

  // The check runs on the mouse the window shows: its key comes once its receiver names it.
  useEffect(() => {
    if (shown?.key) pickMouse(shown.key);
  }, [shown?.key]);

  /** A check came to its part of the report: a connection checked again keeps its place. */
  const done = (part: Part) => {
    setParts((all) => (all.some((p) => p.id === part.id) ? all.map((p) => (p.id === part.id ? part : p)) : [...all, part]));
    setAdding(false);
    // A mouse hrtz doesn't know: what it read is put to its owner first, while they're here.
    if (asks(part)) ask(part);
    else setView("report");
  };

  const ask = (part: Part) => {
    setAsking(part.id);
    setView("confirm");
  };
  /** Something learnt of a part after its check: its owner's answers, a change tried. */
  const learnt = (part: Part, more: Partial<Part>) => setParts((all) => all.map((p) => (p.id === part.id ? { ...p, ...more } : p)));
  const asked = parts.find((p) => p.id === asking);

  /** Check `m`: from the list it starts a new report, unless it's the mouse's other connection. */
  const check = (m: Inspected, fresh: boolean) => {
    if (fresh) {
      setParts([]);
      setTyped("");
    }
    setPicked(m);
    // One hrtz can't read has nothing to check: its report is what Windows knows. One just
    // plugged in gets its check's page first, in case hrtz turns out to get further with it.
    if (m.reach === "windows" && settled(m)) done(windowsPart(m));
    else setView("check");
  };

  const again = (part: Part) => {
    const m = mice?.find((x) => idOf(x) === part.id);
    if (m) check(m, false);
    else toast(t("It isn't plugged in any more. Plug it back in to check it again."), "error");
  };

  // Opened for a mouse: it's picked as soon as it's listed, once (Back then shows the list).
  useEffect(() => {
    if (!openedFor || picked || !mice) return;
    const m = mice.find((x) => x.key === openedFor);
    if (!m) return;
    check(m, true);
    setInspecting(true);
  }, [mice, openedFor]);

  const back = () => {
    if (view === "check") setView(parts.length && !adding ? "report" : "list");
    else if (view === "confirm") setView("report");
    else if (view === "report") setView("list");
    else if (adding) {
      setAdding(false);
      setView("report");
    } else setInspecting(false);
  };

  return (
    <div className={s.frame}>
      <TitleBar />
      <div className={s.page}>
        <div className={s.scroll}>
          <div className={s.column}>
            {view === "check" && shown ? (
              <MouseCheck key={idOf(shown)} mouse={shown} support={supportOf(shown.key, supported)} device={device} onDone={done} />
            ) : view === "confirm" && asked ? (
              <Confirm
                key={asked.id}
                part={asked}
                onDone={(confirmed) => {
                  learnt(asked, { confirmed });
                  setView("report");
                }}
              />
            ) : (view === "report" || view === "confirm") && parts.length ? (
              <ReportView
                parts={parts}
                typed={typed}
                onTyped={setTyped}
                onAgain={again}
                onAsk={ask}
                onTried={(p, tried) => learnt(p, { tried })}
                onAnother={() => {
                  setAdding(true);
                  setView("list");
                }}
              />
            ) : (
              <>
                <MousePlaceholder width={150} />
                <Title1 as="h1" className={s.title}>
                  {adding ? t("Check its other connection") : t("Inspect a mouse")}
                </Title1>
                <Body1 className={s.muted}>
                  {adding
                    ? t("Plug in the mouse's receiver or its cable, then pick it here. Its check goes into the same report.")
                    : t("Is your mouse not supported or not tested yet? Check it here and send us the report. It's what lets us add your mouse.")}
                </Body1>
                {mice === null ? (
                  <div className={s.status}>
                    <Spinner size="extra-tiny" />
                    <Caption1>{t("Looking for mice")}</Caption1>
                  </div>
                ) : mice.length === 0 ? (
                  <Caption1 className={s.muted}>{t("No mouse found. Plug in its receiver or cable.")}</Caption1>
                ) : (
                  <div className={s.list} role="list" aria-label={t("Mice and receivers")}>
                    {mice.map((m) => {
                      const support = supportOf(m.key, supported);
                      const inReport = adding && parts.some((p) => p.id === idOf(m));
                      return (
                        <div key={idOf(m)} className={s.row} role="listitem">
                          <span className={s.rowArt}>
                            <MousePicture image={artOf(m.key)?.image} width={48} />
                          </span>
                          <span className={s.rowText}>
                            <Body1>{nameOf(m)}</Body1>
                            <Caption1 className={s.muted}>
                              {idsOf(m)} · {inReport ? t("In this report") : statusOf(m, support)}
                            </Caption1>
                          </span>
                          <Button onClick={() => check(m, !adding)}>{inReport ? t("Check again") : t("Check")}</Button>
                        </div>
                      );
                    })}
                  </div>
                )}
                <Caption1 className={s.muted}>{t("Keyboards can show up here too, since many include a mouse.")}</Caption1>
                {/* Back from a report leaves it as it was, until another mouse is checked. */}
                {!adding && parts.length > 0 && (
                  <Link as="button" onClick={() => setView("report")}>
                    {t("Back to your report")}
                  </Link>
                )}
              </>
            )}
          </div>
        </div>
        <div className={s.footer}>
          <Button onClick={back}>{t("Back")}</Button>
        </div>
      </div>
    </div>
  );
}
