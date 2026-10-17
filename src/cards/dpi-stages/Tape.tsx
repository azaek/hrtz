import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { COMMON_DPI, clamp, fmtDpi, nextFree, shortDpi, type DpiRange } from "@/dpi";
import { cn } from "@/lib/utils";

// The inline tape: a ruler that slides under a fixed needle. Values as tuned in the DPI card lab
// (DECISIONS.md, "DPI stages card").

/** Width of one doubling (400 → 800), so 400, 800, 1600 and 3200 sit evenly. */
const OCTAVE = 95;
const HEIGHT = 32;
const LABEL_SIZE = 10;
const LABEL_Y = 11;
/** Ticks, major and minor alike, run from here to BOTTOM; labels and strength tell them apart. */
const TICK_TOP = 22;
const BOTTOM = 28;
const STRENGTH = 0.6;
/** The fade at each end, % of the tape. */
const FADE = 22;
/** Tape px per pointer px. */
const GAIN = 1.1;
/** How near a common value (in doublings) a drag must come to settle on it. */
const SNAP = 0.06;

/** Steps that read as round numbers at each size: 50s low down, 500s up high. */
const unitFor = (v: number) => (v < 1000 ? 50 : v < 4000 ? 100 : v < 10000 ? 250 : 500);
const LABELLED = [50, 100, 200, 400, 800, 1600, 3200, 6400, 12800, 25600];

/** Every tick on the range: dense (half the round step), with every labelled value included. */
function ticksFor(r: DpiRange) {
  const out = new Set(LABELLED.filter((v) => v >= r.min && v <= r.max));
  for (let v = r.min; v <= r.max; v += Math.max(r.step, unitFor(v) / 2)) out.add(v);
  out.add(r.max);
  return [...out].sort((a, b) => a - b);
}

/** Where a drag lands: a common value if it's close, otherwise the nearest round step, never one
 *  another stage has. */
function land(r: DpiRange, raw: number, taken: Set<number>, dir: 1 | -1) {
  const near = COMMON_DPI.find((d) => d <= r.max && Math.abs(Math.log2(raw / d)) < SNAP);
  const unit = Math.max(r.step, unitFor(raw));
  return nextFree(r, near ?? clamp(Math.round(raw / unit) * unit, r.min, r.max), taken, dir);
}

/**
 * Drag it like a dial: left brings bigger values under the needle. Shift drags at a fifth of the
 * speed; arrow keys nudge a round step (Shift: 500). The value shows above the needle while
 * dragging. `onActive` says when it's in use (dragging or keyboard focus), so the row can keep it
 * on screen while the pointer wanders off.
 */
export function Tape(props: {
  value: number;
  range: DpiRange;
  taken: Set<number>;
  label: string;
  onDrag: (v: number) => void;
  onDragEnd: () => void;
  onActive?: (on: boolean) => void;
  className?: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(296);
  const [dragging, setDragging] = useState(false);
  const drag = useRef<{ centre: number; acc: number; last: number } | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const r = props.range;
  const centre = Math.log2(props.value);
  const nudge = (dir: 1 | -1, big: boolean) => {
    props.onDrag(nextFree(r, clamp(props.value + dir * (big ? 500 : unitFor(props.value)), r.min, r.max), props.taken, dir));
    props.onDragEnd();
  };
  const end = () => {
    if (!drag.current) return;
    drag.current = null;
    setDragging(false);
    props.onActive?.(false);
    props.onDragEnd();
  };

  return (
    <div
      ref={box}
      role="slider"
      tabIndex={0}
      aria-label={props.label}
      aria-valuemin={r.min}
      aria-valuemax={r.max}
      aria-valuenow={props.value}
      aria-valuetext={`${fmtDpi(props.value)} DPI`}
      className={cn("relative cursor-ew-resize touch-none rounded-md outline-none select-none focus-visible:ring-2 focus-visible:ring-ring", props.className)}
      style={{ height: HEIGHT }}
      onFocus={(e) => e.currentTarget.matches(":focus-visible") && props.onActive?.(true)}
      onBlur={() => !drag.current && props.onActive?.(false)}
      onKeyDown={(e: KeyboardEvent) => {
        if (e.key === "ArrowRight" || e.key === "ArrowUp") (e.preventDefault(), nudge(1, e.shiftKey));
        if (e.key === "ArrowLeft" || e.key === "ArrowDown") (e.preventDefault(), nudge(-1, e.shiftKey));
      }}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        drag.current = { centre, acc: 0, last: props.value };
        try {
          e.currentTarget.setPointerCapture(e.pointerId);
        } catch {
          /* keep going without capture */
        }
        setDragging(true);
        props.onActive?.(true);
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        d.acc += e.movementX * GAIN * (e.shiftKey ? 0.2 : 1);
        const v = land(r, 2 ** (d.centre - d.acc / OCTAVE), props.taken, e.movementX < 0 ? 1 : -1);
        if (v !== d.last) {
          d.last = v;
          props.onDrag(v);
        }
      }}
      onPointerUp={end}
      onPointerCancel={end}
    >
      <div
        className="absolute inset-0 overflow-hidden"
        style={{ maskImage: `linear-gradient(to right, transparent, black ${FADE}%, black ${100 - FADE}%, transparent)` }}
      >
        <svg width={width} height={HEIGHT} aria-hidden className="absolute inset-0">
          {ticksFor(r).map((t) => {
            const x = (Math.log2(t) - centre) * OCTAVE + width / 2;
            if (x < -12 || x > width + 12) return null;
            const major = LABELLED.includes(t) || t === r.max;
            return (
              <g key={t}>
                <line x1={x} x2={x} y1={TICK_TOP} y2={BOTTOM} stroke="currentColor" strokeOpacity={major ? STRENGTH : STRENGTH * 0.4} />
                {major && (
                  <text x={x} y={LABEL_Y} textAnchor="middle" fontSize={LABEL_SIZE} fill="currentColor" fillOpacity={0.6} style={{ fontVariantNumeric: "tabular-nums" }}>
                    {shortDpi(t)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      </div>
      {/* The needle stays put; the ruler moves. A ring of the window's colour keeps it clear of the ticks. */}
      <span
        aria-hidden
        className="pointer-events-none absolute left-1/2 w-1.5 -translate-x-1/2 rounded-full bg-foreground"
        style={{ top: TICK_TOP - 4, bottom: HEIGHT - BOTTOM - 2, boxShadow: "0 0 0 1.5px var(--background)" }}
      />
      {dragging && (
        <span className="pointer-events-none absolute bottom-full left-1/2 mb-1 -translate-x-1/2 rounded-md bg-popover px-1.5 py-0.5 text-xs font-medium tabular-nums shadow-sm ring-1 ring-foreground/10">
          {fmtDpi(props.value)}
        </span>
      )}
    </div>
  );
}
