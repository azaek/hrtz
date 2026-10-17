import { useState, type ComponentProps, type ReactNode } from "react";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

/**
 * Rows inside a Fluent SettingCard, edge to edge under its header like a Windows settings list:
 * pass it as the card's `below`. Separators are the card's hairline. `footer` is a line under the
 * rows (a CardNote or an UndoNote).
 */
export function CardList({ children, footer }: { children: ReactNode; footer?: ReactNode }) {
  return (
    <div className="tw -mx-4 -mb-3">
      <ul className="divide-y border-y">{children}</ul>
      {footer && <div className="px-4 py-3 text-xs">{footer}</div>}
    </div>
  );
}

/** One row of a CardList: a soft wash on hover, nothing on click. A hover group (`group/row`),
 *  so things inside can appear on hover with `group-hover/row:`. */
export function CardListRow({ className, ...props }: ComponentProps<"li">) {
  return <li className={cn("group/row flex items-center gap-3 px-4 py-2.5 transition-colors hover:bg-muted/30", className)} {...props} />;
}

/** The line under a list: what just happened (a rounding, a clash, in amber when it's a
 *  warning), or else a hint. Read out when it changes. */
export function CardNote({ note, hint }: { note?: { text: string; warn?: boolean } | null; hint?: ReactNode }) {
  return (
    <p aria-live="polite" className={cn("tabular-nums", note?.warn ? "text-amber-400" : "text-muted-foreground")}>
      {note?.text ?? hint}
    </p>
  );
}

/** "Removed stage 3 (3,200) · Undo": removals happen at once and offer this, not a confirm. */
export function UndoNote({ children, onUndo }: { children: ReactNode; onUndo: () => void }) {
  return (
    <div role="status" className="flex items-center gap-3 text-muted-foreground">
      {children}
      <button className="cursor-pointer font-medium text-foreground underline-offset-4 hover:underline" onClick={onUndo}>
        Undo
      </button>
    </div>
  );
}

/**
 * Two things in one place in a CardListRow: `rest` normally, `active` while the row is pointed at
 * or while `active` says it's in use (dragging, keyboard focus), even if the pointer leaves. Both
 * stay laid out, so nothing moves. `active` gets the setter to call with true and false.
 */
export function HoverSwap({ rest, active }: { rest: ReactNode; active: (setInUse: (on: boolean) => void) => ReactNode }) {
  const [inUse, setInUse] = useState(false);
  return (
    <div className="grid min-w-0 flex-1 items-center">
      <span className={cn("col-start-1 row-start-1 truncate text-xs text-muted-foreground tabular-nums transition-opacity duration-150", inUse ? "opacity-0" : "group-hover/row:opacity-0")}>
        {rest}
      </span>
      <div
        className={cn(
          "col-start-1 row-start-1 max-w-80 transition-opacity duration-150",
          inUse ? "opacity-100" : "pointer-events-none opacity-0 group-hover/row:pointer-events-auto group-hover/row:opacity-100",
        )}
      >
        {active(setInUse)}
      </div>
    </div>
  );
}

/** A soft badge with the accent dot: the app's one accent mark, for "this is the current one"
 *  (the stage the mouse is using). */
export function AccentBadge({ children }: { children: ReactNode }) {
  return (
    <Badge variant="secondary" className="tw gap-1.5 text-brand">
      <span aria-hidden className="size-1.5 rounded-full bg-brand" />
      {children}
    </Badge>
  );
}
