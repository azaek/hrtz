import { useState, type ReactNode } from "react";
import { Body1, Button, Field, Input, Link, Radio, RadioGroup, Textarea } from "@fluentui/react-components";
import type { Way } from "@/api";
import { StepCard } from "@/kit";
import { MouseHead } from "./parts";
import { partLabel, readingOf, shownOf, type Confirmed, type Part } from "./report";

const soft = { color: "var(--colorNeutralForeground2)" };

/** A setting as read: each way it could be, where there's more than one. */
const said = (ways: Way[]) => ways.map((w, i) => (i ? w.text.toLowerCase() : w.text)).join(", or ");

/** The answers so far, starting from the ones given before. */
function useAnswers(part: Part) {
  const ways = readingOf(part, "stages")?.ways ?? [];
  const shown = shownOf(part);
  const was = part.confirmed;
  const [pick, setPick] = useState<string | null>(was?.stages ?? null);
  const [stagesAre, setStagesAre] = useState(was?.stagesAre ?? "");
  const [rest, setRest] = useState<Confirmed["rest"] | null>(was?.rest ?? null);
  const [note, setNote] = useState(was?.note ?? "");
  const [topDpi, setTopDpi] = useState(was?.topDpi ?? "");
  const [sensor, setSensor] = useState(was?.sensor ?? "");
  const answers = (): Confirmed => ({
    stages: ways.length ? (pick ?? "unsure") : null,
    stagesAre: pick === "none" ? stagesAre.trim() : "",
    rest: shown.length ? (rest ?? "unsure") : "unsure",
    note: rest === "wrong" ? note.trim() : "",
    topDpi: topDpi.trim(),
    sensor: sensor.trim(),
  });
  return { ways, shown, pick, setPick, stagesAre, setStagesAre, rest, setRest, note, setNote, topDpi, setTopDpi, sensor, setSensor, answers };
}
type Answers = ReturnType<typeof useAnswers>;

/** What its own app shows instead, asked when none of hrtz's readings was right. */
function StagesAre({ a }: { a: Answers }) {
  return (
    <Field label="What does its own app show?" style={{ width: "100%" }}>
      <Input value={a.stagesAre} maxLength={80} placeholder="400, 800, 1600, 3200" onChange={(_, d) => a.setStagesAre(d.value)} />
    </Field>
  );
}

function WhatsDifferent({ a }: { a: Answers }) {
  return (
    <Field label="What's different?" style={{ width: "100%" }}>
      <Textarea value={a.note} maxLength={300} placeholder="Debounce is 2 ms, not 4" onChange={(_, d) => a.setNote(d.value)} />
    </Field>
  );
}

/** What hrtz can't read off a mouse, where its maker's own list doesn't name the model. */
function More({ a }: { a: Answers }) {
  return (
    <div className="grid w-full grid-cols-2 gap-3">
      <Field label="Highest DPI its own app offers">
        <Input value={a.topDpi} maxLength={12} placeholder="26000" onChange={(_, d) => a.setTopDpi(d.value)} />
      </Field>
      <Field label="Its sensor, if you know it">
        <Input value={a.sensor} maxLength={40} placeholder="PAW3395" onChange={(_, d) => a.setSensor(d.value)} />
      </Field>
    </div>
  );
}

/** The settings read, a row each. */
function Rows({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="m-0 flex flex-col">
      {rows.map(([name, value], i) => (
        <div key={name} className={`flex min-h-10 items-center justify-between gap-4 py-1.5 ${i ? "border-t" : ""}`}>
          <dt className="text-sm" style={soft}>
            {name}
          </dt>
          <dd className="m-0 text-right text-sm tabular-nums">{value}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A question at a time: the DPI stages, the rest, then what hrtz can't read. */
function Steps({ part, onDone }: { part: Part; onDone: (c: Confirmed) => void }) {
  const a = useAnswers(part);
  const steps = [a.ways.length > 0 && "stages", a.shown.length > 0 && "rest", !part.listed && "more"].filter((s): s is "stages" | "rest" | "more" => !!s);
  const [at, setAt] = useState(0);
  const step = steps[Math.min(at, steps.length - 1)];
  const last = at >= steps.length - 1;
  const one = a.ways.length === 1;
  const answered = step === "stages" ? !!a.pick : step === "rest" ? !!a.rest : true;

  const title = step === "stages" ? (one ? "Are these your DPI stages?" : "Which of these are your DPI stages?") : step === "rest" ? "Is the rest right?" : "Two things hrtz can't read";
  const detail =
    step === "stages"
      ? one
        ? "As its own app shows them."
        : "Its memory reads more than one way. Pick the one its own app shows."
      : step === "rest"
        ? "As its own app shows them."
        : "Leave either empty if you're not sure.";

  return (
    <StepCard label={`Question ${at + 1} of ${steps.length}`} title={title} detail={detail}>
      {step === "stages" && (
        <>
          <RadioGroup value={a.pick ?? ""} onChange={(_, d) => a.setPick(d.value)}>
            {a.ways.map((w) => (
              <Radio key={w.piece} value={w.piece} label={w.text} />
            ))}
            <Radio value="none" label={one ? "No, they're different" : "None of these"} />
            <Radio value="unsure" label="I'm not sure" />
          </RadioGroup>
          {a.pick === "none" && <StagesAre a={a} />}
        </>
      )}
      {step === "rest" && (
        <>
          <Rows rows={a.shown.map((s) => [s.name, said(s.ways)])} />
          <RadioGroup layout="horizontal" value={a.rest ?? ""} onChange={(_, d) => a.setRest(d.value as Confirmed["rest"])}>
            <Radio value="right" label="Yes" />
            <Radio value="wrong" label="Not all of it" />
            <Radio value="unsure" label="I'm not sure" />
          </RadioGroup>
          {a.rest === "wrong" && <WhatsDifferent a={a} />}
        </>
      )}
      {step === "more" && <More a={a} />}
      <div className="flex min-h-8 items-center justify-between gap-3">
        <span>
          {at > 0 && (
            <Link as="button" onClick={() => setAt(at - 1)}>
              Previous question
            </Link>
          )}
        </span>
        <Button appearance="primary" disabled={!answered} onClick={() => (last ? onDone(a.answers()) : setAt(at + 1))}>
          {last ? "Done" : "Next"}
        </Button>
      </div>
    </StepCard>
  );
}

/**
 * Inspect a mouse's questions about what hrtz read from a mouse it doesn't know: which reading of
 * its DPI stages is the owner's, whether the rest is right, and what the mouse can't say itself.
 * Memory shows where a setting is kept, not what its bytes mean, so this is what lets one report
 * be enough. A question at a time, before the report (the user's pick, 2026-11-14, over one page
 * of everything read, and over asking only the DPI stages on the report's card).
 */
export function Confirm({ part, onDone }: { part: Part; onDone: (c: Confirmed) => void }) {
  return (
    <div className="tw flex w-full flex-col items-center gap-3 text-center">
      <MouseHead artKey={part.key} variant={part.variant} name={part.name} caption={partLabel(part)} />
      <Body1 style={soft}>
        {part.known
          ? "hrtz doesn't set this model's DPI yet, as its sensor can keep it more than one way. Your answer tells us which."
          : "hrtz read these from the mouse's memory. Your answers are what lets us add it without asking you again."}
      </Body1>
      <div className="w-full text-left">
        <Steps part={part} onDone={onDone} />
      </div>
    </div>
  );
}
