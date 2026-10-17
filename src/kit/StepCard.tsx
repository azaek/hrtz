import type { ReactNode } from "react";
import { Caption1, Subtitle2 } from "@fluentui/react-components";

const muted = { color: "var(--colorNeutralForeground3)" };
const soft = { color: "var(--colorNeutralForeground2)" };

/** A thin neutral line that fills: a run counting, a countdown. Never the accent. */
export function ProgressLine({ value, label }: { value: number; label?: string }) {
  const v = Math.min(1, Math.max(0, value));
  return (
    <div
      className="tw h-1 w-full overflow-hidden rounded-full bg-[color:var(--colorNeutralStroke2)]"
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(v * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="h-full rounded-full bg-[color:var(--colorNeutralForeground3)] transition-[width] duration-200" style={{ width: `${v * 100}%` }} />
    </div>
  );
}

/**
 * One step of a guided check: where it is ("Step 1", "Run 2 · 8,000 Hz"), what to do or what was
 * found, said big, a line more, and something drawn on the right (the motion asked for). Then how
 * far along it is, a status line with links, and anything else below.
 */
export function StepCard(props: {
  label: ReactNode;
  title: ReactNode;
  detail?: ReactNode;
  art?: ReactNode;
  progress?: number | null;
  status?: ReactNode;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="tw flex flex-col gap-3 rounded-[var(--borderRadiusMedium)] border px-4 py-3.5">
      <div className="flex items-start gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <Caption1 style={muted}>{props.label}</Caption1>
          <Subtitle2>{props.title}</Subtitle2>
          {props.detail && <Caption1 style={soft}>{props.detail}</Caption1>}
        </div>
        {props.art}
      </div>
      {props.progress != null && <ProgressLine value={props.progress} />}
      {(props.status || props.actions) && (
        <div className="flex min-h-8 items-center justify-between gap-3">
          <Caption1 className="tabular-nums">{props.status}</Caption1>
          <div className="flex items-center gap-3">{props.actions}</div>
        </div>
      )}
      {props.children}
    </div>
  );
}
