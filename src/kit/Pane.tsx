import type { ComponentProps, ReactNode } from "react";
import { Body1, Caption1 } from "@fluentui/react-components";
import { cn } from "@/lib/utils";

/**
 * A card surface for layouts a SettingCard doesn't fit, such as master and detail (the Buttons
 * page): the card's fill, edge and corners, laid out as a column. Put a PaneHeader at the top and
 * a PaneFooter at the bottom.
 */
export function Pane({ className, sticky, ...props }: ComponentProps<"section"> & { sticky?: boolean }) {
  return (
    <section
      className={cn(
        "tw flex min-w-0 flex-col rounded-[var(--borderRadiusLarge)] border border-[color:var(--card-stroke)] bg-[var(--card)]",
        // A master list beside its detail stays in view while a long detail scrolls the page. Only
        // for a list shorter than the window: a taller one would hide its last rows.
        sticky && "@min-[700px]:sticky @min-[700px]:top-4",
        className,
      )}
      {...props}
    />
  );
}

/** A pane's title line, padded like a SettingCard: something before it, the title and a
 *  description, and actions on the right (a MoreMenu). */
export function PaneHeader(props: { lead?: ReactNode; title: ReactNode; description?: ReactNode; actions?: ReactNode; strong?: boolean }) {
  return (
    <div className="flex items-center gap-3 py-3 pr-2 pl-4">
      {props.lead}
      <div className="flex min-w-0 flex-1 flex-col">
        <Body1 style={props.strong ? { fontWeight: 600 } : undefined}>{props.title}</Body1>
        {props.description && <Caption1 style={{ color: "var(--colorNeutralForeground3)" }}>{props.description}</Caption1>}
      </div>
      {props.actions}
    </div>
  );
}

/** The line at the bottom of a pane, under a hairline: a CardNote or an UndoNote. */
export function PaneFooter({ children }: { children: ReactNode }) {
  return <div className="mt-auto border-t px-4 py-3 text-xs">{children}</div>;
}
