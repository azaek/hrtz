import { cn } from "@/lib/utils";
import type { ModelArt } from "@/models";
import { DPI_BUTTON } from "./functions";

/** A button's number: dark with a light ring at rest, inverted while it's pointed at or picked.
 *  The same chip on the picture and in the list, so the two read as one. No accent. */
export function Chip({ n, lit, onPicture }: { n: number; lit?: boolean; onPicture?: boolean }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-grid size-5 shrink-0 place-items-center rounded-full text-[11px] leading-none font-semibold tabular-nums transition-[background-color,color,box-shadow,scale] duration-150 ease-out",
        lit
          ? onPicture
            ? "scale-110 bg-white text-neutral-900 shadow-[0_0_0_1.5px_rgb(23_23_23),0_1px_3px_rgb(0_0_0/0.4)]"
            : "bg-white text-neutral-900 shadow-[0_0_0_1px_rgb(255_255_255)]"
          : onPicture
            ? "bg-neutral-900/85 text-white shadow-[0_0_0_1.5px_rgb(255_255_255/0.85),0_1px_3px_rgb(0_0_0/0.4)]"
            : "bg-neutral-900 text-white/80 shadow-[0_0_0_1px_rgb(255_255_255/0.14)]",
      )}
    >
      {n}
    </span>
  );
}

/**
 * The vendor's render of this model with a chip on each button it shows (the DPI button is
 * underneath, so it has none). Pointing at a chip lights it; clicking it picks that button.
 */
export function MouseArt(props: {
  art: ModelArt;
  buttons: [number, string][];
  width: number;
  lit: number | null;
  onLight: (i: number | null) => void;
  onPick: (i: number) => void;
}) {
  const [w, h] = props.art.size;
  const numberOf = (i: number) => props.buttons.findIndex(([b]) => b === i) + 1;
  return (
    <div className="relative shrink-0" style={{ width: props.width, height: (props.width * h) / w }}>
      <img src={props.art.image} alt={`${props.art.name}, ${props.art.view ?? "viewed from the left"}`} className="pointer-events-none size-full select-none" draggable={false} />
      {props.buttons
        .filter(([i]) => i !== DPI_BUTTON && props.art.hotspots[i])
        .map(([i, name]) => {
          const [x, y] = props.art.hotspots[i];
          return (
            <button
              key={i}
              aria-label={name}
              title={name}
              // The list beside it is the keyboard's way in.
              tabIndex={-1}
              className="absolute grid size-8 -translate-x-1/2 -translate-y-1/2 cursor-pointer place-items-center rounded-full touch-manipulation"
              style={{ left: `${(x / w) * 100}%`, top: `${(y / h) * 100}%` }}
              onPointerEnter={() => props.onLight(i)}
              onPointerLeave={() => props.onLight(null)}
              onClick={() => props.onPick(i)}
            >
              <Chip n={numberOf(i)} lit={props.lit === i} onPicture />
            </button>
          );
        })}
    </div>
  );
}
