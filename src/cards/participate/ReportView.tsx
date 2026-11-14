import { useEffect, useState } from "react";
import { Body1, Button, Caption1, Field, Input, Link } from "@fluentui/react-components";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, message } from "@/api";
import { StepCard } from "@/kit";
import { SUPPORT_TEXT, supportOf } from "@/models";
import { toast, useStore } from "@/store";
import { MouseHead, Results, VerdictIcon } from "./parts";
import { asks, buildReport, calledOf, canTry, isTest, issueOf, linesOf, partLabel, reportName, verdictOf, type Part, type Tried } from "./report";
import { Trial } from "./Trial";

const muted = { color: "var(--colorNeutralForeground3)" };

/** The mouse's other way of connecting, to check too: asked while only one is in the report. */
function otherOf(parts: Part[]): { title: string; detail: string; action: string } | null {
  if (parts.length !== 1) return null;
  const both = "Both go in this one report.";
  switch (parts[0].wired) {
    case true:
      return { title: "Has a receiver too?", detail: `Check the mouse through it as well. ${both}`, action: "Check on the receiver too" };
    case false:
      return { title: "Has a cable too?", detail: `Check the mouse on it as well. ${both}`, action: "Check on the cable too" };
    default:
      return { title: "Does it connect another way too?", detail: `Through a receiver, or on a cable: check it there as well. ${both}`, action: "Check another connection" };
  }
}

/**
 * Inspect a mouse's report: what each check came to, said first, then what it found; one change
 * to try, where hrtz can; the mouse's other connection to check too; and the report to send. The
 * GitHub issue opens with the report inside its text, so nothing has to be dragged in; the file is
 * for sending it another way. `parts` is a part per connection checked, `typed` the name its owner
 * gives the mouse, where hrtz can't name it.
 */
export function ReportView(props: {
  parts: Part[];
  typed: string;
  onTyped: (name: string) => void;
  onAgain: (part: Part) => void;
  onAnother: () => void;
  /** Its owner is asked about what hrtz read, again or for the first time. */
  onAsk: (part: Part) => void;
  onTried: (part: Part, t: Tried) => void;
}) {
  const { parts, typed } = props;
  const supported = useStore((x) => x.supported);
  const [saved, setSaved] = useState<{ name: string; path: string } | null>(null);
  /** The issue was opened, and whether all of the report fit in it. */
  const [sent, setSent] = useState<{ whole: boolean } | null>(null);
  const test = isTest(parts);
  const first = parts.find((p) => p.known) ?? parts[0];
  const support = supportOf(first.key, supported);
  const offered = parts.find((p) => p.called)?.called;
  const called = calledOf(parts, typed);
  const other = otherOf(parts);
  const trying = parts.filter((p) => canTry(p) && !p.tried);

  // A report that changed is saved and sent again.
  useEffect(() => {
    setSaved(null);
    setSent(null);
  }, [parts, typed]);

  const save = async () => {
    const path = await saveDialog({ defaultPath: reportName(called || first.name), filters: [{ name: "Mouse report", extensions: ["json"] }] }).catch(() => null);
    if (!path) return;
    try {
      await api.saveReport(path, JSON.stringify(await buildReport(parts, called), null, 2));
      setSaved({ path, name: path.split(/[\\/]/).pop() ?? path });
    } catch (e) {
      toast(message(e), "error");
    }
  };

  // A model hrtz knows goes in as a test report; anything else as a request for support.
  const send = async () => {
    try {
      const issue = await issueOf(parts, called);
      await openUrl(issue.url);
      setSent({ whole: issue.whole });
    } catch (e) {
      toast(message(e), "error");
    }
  };

  return (
    <div className="tw flex w-full flex-col items-center gap-3 text-center">
      <MouseHead
        artKey={first.key}
        variant={first.variant}
        name={first.name}
        caption={`${[...new Set(parts.map((p) => p.ids))].join(" · ")} · ${support ? SUPPORT_TEXT[support] : "Not supported yet"}`}
      />

      {parts.map((p) => {
        const verdict = verdictOf(p);
        const asked = asks(p);
        return (
          <div key={p.id} className="w-full text-left">
            <StepCard
              label={parts.length > 1 && p.name !== first.name ? `${p.name} · ${partLabel(p)}` : partLabel(p)}
              title={
                <span className="flex items-center gap-2">
                  <VerdictIcon tone={verdict.tone} />
                  {verdict.title}
                </span>
              }
              detail={verdict.detail}
            >
              <Results lines={linesOf(p)} />
              <div className="flex min-h-8 items-center justify-end gap-4">
                {asked && (
                  <Link as="button" onClick={() => props.onAsk(p)}>
                    {p.confirmed ? "Change your answers" : "Answer its questions"}
                  </Link>
                )}
                <Link as="button" onClick={() => props.onAgain(p)}>
                  Check again
                </Link>
              </div>
            </StepCard>
          </div>
        );
      })}

      {trying.map((p) => (
        <Trial key={p.id} part={p} onTried={(t) => props.onTried(p, t)} />
      ))}

      {other ? (
        <div className="flex w-full items-center justify-between gap-4 rounded-[var(--borderRadiusMedium)] border px-4 py-3 text-left">
          <div className="flex min-w-0 flex-col gap-0.5">
            <Body1>{other.title}</Body1>
            <Caption1 style={muted}>{other.detail}</Caption1>
          </div>
          <Button style={{ flexShrink: 0 }} onClick={props.onAnother}>
            {other.action}
          </Button>
        </div>
      ) : (
        <Link as="button" onClick={props.onAnother}>
          Check another connection
        </Link>
      )}

      {!test && (
        <Field style={{ width: "100%", textAlign: "left" }} label="What's the mouse called?" hint="Its brand and model, as on the box. A receiver's own name rarely says.">
          <Input value={typed} maxLength={80} placeholder={offered ?? "ATK U2 Plus"} onChange={(_, d) => props.onTyped(d.value)} />
        </Field>
      )}

      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button appearance="primary" onClick={() => void send()}>
          {test ? "Open GitHub issue" : "Request support on GitHub"}
        </Button>
        {saved ? <Button onClick={() => void revealItemInDir(saved.path).catch((e) => toast(message(e), "error"))}>Show in folder</Button> : <Button onClick={() => void save()}>Save as a file…</Button>}
      </div>
      <Caption1 style={muted}>
        {sent && !sent.whole
          ? saved
            ? `Not all of the report fit in the issue. Drag ${saved.name} into it too.`
            : "Not all of the report fit in the issue. Save it as a file and drag that into the issue too."
          : saved
            ? `Saved as ${saved.name}, to send any other way.`
            : "The issue opens with the report in it: all that's left is to submit it. Nothing in it identifies you or this PC."}
      </Caption1>
    </div>
  );
}
