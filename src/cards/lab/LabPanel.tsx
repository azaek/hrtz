import { useState } from "react";
import { Body1, Caption1 } from "@fluentui/react-components";
import { t } from "@/i18n";
import { CardNote, Pane, PaneFooter, PaneHeader } from "@/kit";
import { useStore } from "@/store";
import { useClickWeek, weekResult } from "./clicks";
import { ClickSwitch, OPEN_KEY, PadSwitch, shownResult, TestBody, testsFor, usePadStatus, useResults, whenText, type TestId } from "./tests";
import { WindowsCheck } from "./WindowsCheck";
import { NewBadge } from "@/kit";
import { useSeen } from "@/news";

/** The blind test's "New", until it's been on screen. */
function BlindNew() {
  const news = useSeen("blindtest");
  return news.isNew ? <NewBadge ref={news.ref} /> : null;
}

const muted = { color: "var(--colorNeutralForeground3)" };
const warn = { color: "var(--colorPaletteMarigoldForeground1)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;

/**
 * The Lab page: master and detail, like Buttons and Profiles. The tests on the left, each with
 * its last result (the mousepad's is live, and so is the double clicks', from the week counted in
 * the background); the one picked on the right, running while it's open.
 * One test at a time, so a drag for the DPI test isn't also counted as clicks. Windows settings
 * that skew the tests sit on top, one line each, only when one is off. Decided in the Lab page's
 * lab (DECISIONS.md, "Lab page").
 */
export function LabPanel() {
  const caps = useStore((x) => x.device?.capabilities);
  const tests = testsFor(caps);
  const [want, setSel] = useState(() => localStorage.getItem(OPEN_KEY) as TestId | null);
  // The one opened last, while this mouse can run it.
  const test = tests.find((x) => x.id === want) ?? tests[0];
  const sel = test.id;
  const { kept, keep } = useResults();
  const pad = usePadStatus();
  const week = useClickWeek();
  const counting = useStore((x) => x.prefs?.clickHistory ?? true);
  const clicks = counting && week ? weekResult(week) : null;
  const pick = (id: TestId) => {
    setSel(id);
    localStorage.setItem(OPEN_KEY, id);
  };
  const last = sel === "pad" ? undefined : kept[sel];

  return (
    <>
      <WindowsCheck />
      <div className="tw @container/lab">
        <div className="grid items-start gap-2 @min-[700px]/lab:grid-cols-[264px_minmax(0,1fr)]">
          <Pane sticky>
            <PaneHeader title={t("Tests")} description={t("Each test runs while it's open.")} />
            <div role="list" aria-label={t("Tests")} className="flex flex-col gap-0.5 px-1 pb-1">
              {tests.map((x) => {
                const r = x.id === "pad" ? pad : x.id === "clicks" ? (clicks ?? shownResult(kept.clicks)) : shownResult(kept[x.id]);
                return (
                  <button
                    key={x.id}
                    role="listitem"
                    className="st-row"
                    data-selected={sel === x.id}
                    aria-current={sel === x.id ? "true" : undefined}
                    // The mousepad records in the background, unlike the tests above it.
                    style={{ height: 52, gap: 12, ...(x.id === "pad" ? { marginTop: 6 } : {}) }}
                    onClick={() => pick(x.id)}
                  >
                    <span className="st-row__icon">{x.icon}</span>
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="flex items-center gap-2">
                        <Body1 style={truncate}>{x.name}</Body1>
                        {x.id === "blind" && <BlindNew />}
                      </span>
                      <Caption1 style={{ ...truncate, ...(r?.tone === "warn" ? warn : muted) }}>
                        {r ? [r.value, r.detail].filter(Boolean).join(" · ") : x.id === "pad" ? t("Not calibrated yet") : t("Not run yet")}
                      </Caption1>
                    </span>
                  </button>
                );
              })}
            </div>
          </Pane>

          <Pane aria-label={test.title}>
            <PaneHeader
              strong
              lead={<span className="flex text-[20px] text-[color:var(--colorNeutralForeground2)]">{test.icon}</span>}
              title={test.title}
              description={test.description}
              actions={
                test.id === "pad" ? (
                  <div className="pr-2">
                    <PadSwitch calibrated={!!pad} />
                  </div>
                ) : test.id === "clicks" ? (
                  <div className="pr-2">
                    <ClickSwitch />
                  </div>
                ) : undefined
              }
            />
            <div className="px-4 pt-1 pb-4">
              <TestBody key={sel} id={sel} onResult={keep(sel)} />
            </div>
            <PaneFooter>
              {/* A date with its own full stop ("18. Okt.") takes no second one. */}
              <CardNote hint={last ? `${test.hint} ${t("Last checked {{when}}.", { when: whenText(last.at) })}`.replace(/\.\.$/, ".") : test.hint} />
            </PaneFooter>
          </Pane>
        </div>
      </div>
    </>
  );
}
