import type { CSSProperties, ReactElement } from "react";
import { Caption1 } from "@fluentui/react-components";
import { cn } from "@/lib/utils";

/**
 * One of several kinds of thing, as icon tiles in a grid: the Buttons page's functions. Tiles
 * have a faint fill; the chosen one takes the tile tint (states.css `.st-tile`), not a ring.
 * Clicking the chosen one does nothing. Icons are `*20Regular`.
 */
export function TileChoice<T extends string>(props: {
  label: string;
  value: T | null;
  options: { value: T; label: string; icon: ReactElement; disabled?: boolean }[];
  onChange: (v: T) => void;
  disabled?: boolean;
  columns?: number;
}) {
  return (
    <div role="group" aria-label={props.label} className="tw grid gap-1.5" style={{ gridTemplateColumns: `repeat(${props.columns ?? 4}, minmax(0, 1fr))` }}>
      {props.options.map((o) => {
        const on = o.value === props.value;
        return (
          <button
            key={o.value}
            className={cn(
              "st-tile flex h-16 min-w-0 cursor-pointer flex-col items-center justify-center gap-1 rounded-md px-1 text-[color:var(--colorNeutralForeground1)] touch-manipulation",
              "disabled:cursor-default disabled:text-[color:var(--colorNeutralForegroundDisabled)] disabled:[--tile-bg-hover:var(--nav-pressed)]",
            )}
            // Quieter than a button at rest, so the chosen tile's tint stands out.
            style={{ "--tile-bg": "var(--nav-pressed)", "--tile-bg-hover": "var(--nav-hover)" } as CSSProperties}
            data-selected={on}
            aria-pressed={on}
            disabled={props.disabled || o.disabled}
            onClick={() => !on && props.onChange(o.value)}
          >
            <span aria-hidden className="flex text-[20px]">
              {o.icon}
            </span>
            <Caption1 className="max-w-full truncate">{o.label}</Caption1>
          </button>
        );
      })}
    </div>
  );
}
