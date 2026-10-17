import { useState } from "react";
import { Body1, Caption1 } from "@fluentui/react-components";
import { CardNote, Pane, PaneFooter, PaneHeader } from "@/kit";
import { useStore } from "@/store";
import { useClickWeek, weekResult } from "./clicks";
import { ClickSwitch, PadSwitch, TestBody, TESTS, testOf, usePadStatus, useResults, whenText, type TestId } from "./tests";
import { WindowsCheck } from "./WindowsCheck";

const muted = { color: "var(--colorNeutralForeground3)" };
const warn = { color: "var(--colorPaletteMarigoldForeground1)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;
/** The test that was open last, opened again next time. */
const OPEN_KEY = "hrtz.labOpen";

/**
 * The Lab page: master and detail, like Buttons and Profiles. The tests on the left, each with
 * its last result (the mousepad's is live, and so is the double clicks', from the week counted in
 * the background); the one picked on the right, running while it's open.
 * One test at a time, so a drag for the DPI test isn't also counted as clicks. Windows settings
 * that skew the tests sit on top, one line each, only when one is off. Decided in the Lab page's
 * lab (DECISIONS.md, "Lab page").
 */
export function LabPanel() {
  const [sel, setSel] = useState<TestId>(() => testOf(localStorage.getItem(OPEN_KEY) as TestId).id);
  const { kept, keep } = useResults();
  const pad = usePadStatus();
  const week = useClickWeek();
  const counting = useStore((x) => x.prefs?.clickHistory ?? true);
  const clicks = counting && week ? weekResult(week) : null;
  const t = testOf(sel);
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
            <PaneHeader title="Tests" description="Each test runs while it's open." />
            <div role="list" aria-label="Tests" className="flex flex-col gap-0.5 px-1 pb-1">
              {TESTS.map((x) => {
                const r = x.id === "pad" ? pad : x.id === "clicks" ? (clicks ?? kept.clicks?.result) : kept[x.id]?.result;
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
                      <Body1 style={truncate}>{x.name}</Body1>
                      <Caption1 style={{ ...truncate, ...(r?.tone === "warn" ? warn : muted) }}>
                        {r ? `${r.value} · ${r.detail}` : x.id === "pad" ? "Not calibrated yet" : "Not run yet"}
                      </Caption1>
                    </span>
                  </button>
                );
              })}
            </div>
          </Pane>

          <Pane aria-label={t.title}>
            <PaneHeader
              strong
              lead={<span className="flex text-[20px] text-[color:var(--colorNeutralForeground2)]">{t.icon}</span>}
              title={t.title}
              description={t.description}
              actions={
                t.id === "pad" ? (
                  <div className="pr-2">
                    <PadSwitch calibrated={!!pad} />
                  </div>
                ) : t.id === "clicks" ? (
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
              <CardNote hint={last ? `${t.hint} Last checked ${whenText(last.at)}.` : t.hint} />
            </PaneFooter>
          </Pane>
        </div>
      </div>
    </>
  );
}
