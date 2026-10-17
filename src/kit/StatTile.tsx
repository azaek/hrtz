import type { ReactNode } from "react";
import { Caption1 } from "@fluentui/react-components";

/**
 * One figure on a card surface: a label, the number large (tabular), anything that goes with it
 * (a bar), and a line under it saying what it means. Put several in a grid. A figure that isn't
 * known yet is `<StatTile.Unknown />`, with the line saying why.
 */
export function StatTile({ label, value, detail, children }: { label: string; value: ReactNode; detail: ReactNode; children?: ReactNode }) {
  return (
    <div className="tw flex min-w-0 flex-col gap-1 rounded-[var(--borderRadiusLarge)] border border-[color:var(--card-stroke)] bg-[var(--card)] px-4 pt-3 pb-3.5">
      <Caption1 style={{ color: "var(--colorNeutralForeground3)" }}>{label}</Caption1>
      <span className="truncate text-[28px] leading-9 font-semibold tracking-tight tabular-nums">{value}</span>
      {children}
      {/* One line: Fluent's text styles win over Tailwind's, so truncate in style. */}
      <Caption1 style={{ color: "var(--colorNeutralForeground3)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {detail}
      </Caption1>
    </div>
  );
}

/** A quiet dash for a figure that isn't known yet. */
StatTile.Unknown = function Unknown() {
  return <span className="text-muted-foreground">–</span>;
};
