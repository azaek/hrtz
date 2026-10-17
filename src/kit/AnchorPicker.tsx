import type { CSSProperties, KeyboardEvent, ReactElement } from "react";
import {
  ArrowDown16Regular,
  ArrowDownLeft16Regular,
  ArrowDownRight16Regular,
  ArrowLeft16Regular,
  ArrowRight16Regular,
  ArrowUp16Regular,
  ArrowUpLeft16Regular,
  ArrowUpRight16Regular,
} from "@fluentui/react-icons";
import { cn } from "@/lib/utils";
import { CONTROL_HEIGHT } from "./field";

/** An edge or corner of the screen. */
export type Anchor = "top-left" | "top" | "top-right" | "left" | "right" | "bottom-left" | "bottom" | "bottom-right";

type Cell = { value: Anchor; label: string; icon: ReactElement };

/** The grid row by row. The centre is null: nothing anchors there. */
const CELLS: (Cell | null)[] = [
  { value: "top-left", label: "Top left", icon: <ArrowUpLeft16Regular /> },
  { value: "top", label: "Top", icon: <ArrowUp16Regular /> },
  { value: "top-right", label: "Top right", icon: <ArrowUpRight16Regular /> },
  { value: "left", label: "Left", icon: <ArrowLeft16Regular /> },
  null,
  { value: "right", label: "Right", icon: <ArrowRight16Regular /> },
  { value: "bottom-left", label: "Bottom left", icon: <ArrowDownLeft16Regular /> },
  { value: "bottom", label: "Bottom", icon: <ArrowDown16Regular /> },
  { value: "bottom-right", label: "Bottom right", icon: <ArrowDownRight16Regular /> },
];

const STEP: Record<string, [number, number]> = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] };

/**
 * Where on the screen something sits: a 3 × 3 grid of 32px buttons with the centre left empty,
 * each pointing at its edge or corner. The chosen one takes the tile tint (states.css `.st-tile`),
 * like TileChoice. It's one Tab stop, the chosen one; the arrow keys move between them, stepping
 * over the centre.
 */
export function AnchorPicker(props: { label: string; value: Anchor; onChange: (v: Anchor) => void; disabled?: boolean }) {
  // The Tab stop, should the value be none of these.
  const stop = CELLS.some((c) => c?.value === props.value) ? props.value : "top-left";
  const move = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = STEP[e.key];
    if (!step) return;
    const from = CELLS.findIndex((c) => c?.value === (e.target as HTMLElement).dataset.anchor);
    if (from < 0) return;
    e.preventDefault();
    let [x, y] = [from % 3, Math.floor(from / 3)];
    do {
      x += step[0];
      y += step[1];
    } while (x === 1 && y === 1);
    if (x < 0 || x > 2 || y < 0 || y > 2) return;
    e.currentTarget.querySelector<HTMLButtonElement>(`[data-anchor="${CELLS[y * 3 + x]!.value}"]`)?.focus();
  };
  return (
    <div
      role="group"
      aria-label={props.label}
      className="tw grid gap-1"
      style={{ gridTemplateColumns: `repeat(3, ${CONTROL_HEIGHT}px)`, gridAutoRows: CONTROL_HEIGHT }}
      onKeyDown={move}
    >
      {CELLS.map((c, i) => {
        if (!c) return <span key={i} aria-hidden />;
        const on = c.value === props.value;
        return (
          <button
            key={c.value}
            data-anchor={c.value}
            className={cn(
              "st-tile flex cursor-pointer items-center justify-center rounded-[4px] text-[16px] text-[color:var(--colorNeutralForeground1)] touch-manipulation",
              "disabled:cursor-default disabled:text-[color:var(--colorNeutralForegroundDisabled)] disabled:[--tile-bg-hover:var(--nav-pressed)]",
            )}
            // TileChoice's resting fill, so the chosen one's tint stands out.
            style={{ "--tile-bg": "var(--nav-pressed)", "--tile-bg-hover": "var(--nav-hover)" } as CSSProperties}
            data-selected={on}
            aria-pressed={on}
            tabIndex={c.value === stop ? 0 : -1}
            aria-label={c.label}
            title={c.label}
            disabled={props.disabled}
            onClick={() => !on && props.onChange(c.value)}
          >
            {c.icon}
          </button>
        );
      })}
    </div>
  );
}
