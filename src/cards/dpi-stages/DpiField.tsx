import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { InputGroup, InputGroupAddon, InputGroupInput, InputGroupText } from "@/components/ui/input-group";
import { clamp, fmtDpi, nextFree, parseDpi, toStep, type DpiRange } from "@/dpi";
import { cn } from "@/lib/utils";
import { fieldLook } from "@/kit";

export type Note = { text: string; warn?: boolean } | null;

/**
 * The value, typed. ↑/↓ step 50, Shift or PgUp/PgDn 500, the wheel while focused. "1.6k" and
 * "800*2" work. Rounding, limits and clashes with another stage are said (through `onNote`),
 * never applied silently.
 */
export function DpiField(props: {
  value: number;
  range: DpiRange;
  label: string;
  taken: Set<number>;
  onChange: (v: number) => void;
  onNote: (n: Note) => void;
  className?: string;
}) {
  const r = props.range;
  const ref = useRef<HTMLInputElement>(null);
  const [focused, setFocused] = useState(false);
  const [text, setText] = useState(String(props.value));
  useEffect(() => {
    if (!focused) setText(String(props.value));
  }, [props.value, focused]);

  const step = (dir: 1 | -1, big: boolean) => {
    const v = nextFree(r, clamp(props.value + dir * (big ? 500 : r.step), r.min, r.max), props.taken, dir);
    props.onChange(v);
    setText(String(v));
    props.onNote(null);
  };
  // React's wheel listener is passive, so it can't stop the page scrolling.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (document.activeElement !== el) return;
      e.preventDefault();
      step(e.deltaY < 0 ? 1 : -1, e.shiftKey);
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  const commit = () => {
    const raw = parseDpi(text);
    if (raw === null) {
      props.onNote(text.trim() ? { text: "Type a number, like 1600 or 1.6k.", warn: true } : null);
      setText(String(props.value));
      return;
    }
    const v = toStep(r, raw);
    if (v !== props.value && props.taken.has(v)) {
      props.onNote({ text: `Another stage already uses ${fmtDpi(v)}.`, warn: true });
      setText(String(props.value));
      return;
    }
    props.onNote(
      raw > r.max
        ? { text: `This mouse goes up to ${fmtDpi(r.max)}.` }
        : raw < r.min
          ? { text: `The minimum is ${r.min}.` }
          : v !== raw
            ? { text: `Rounded to ${fmtDpi(v)}, in steps of ${r.step}.` }
            : null,
    );
    setText(String(v));
    if (v !== props.value) props.onChange(v);
  };
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    const map: Record<string, () => void> = {
      ArrowUp: () => step(1, e.shiftKey),
      ArrowDown: () => step(-1, e.shiftKey),
      PageUp: () => step(1, true),
      PageDown: () => step(-1, true),
      Enter: () => commit(),
      Escape: () => (setText(String(props.value)), ref.current?.blur()),
    };
    if (map[e.key]) {
      e.preventDefault();
      map[e.key]();
    }
  };

  return (
    <InputGroup className={cn(fieldLook, props.className)}>
      <InputGroupInput
        ref={ref}
        role="spinbutton"
        inputMode="numeric"
        aria-label={props.label}
        aria-valuemin={r.min}
        aria-valuemax={r.max}
        aria-valuenow={props.value}
        aria-valuetext={`${fmtDpi(props.value)} DPI`}
        className={cn("font-medium tabular-nums")}
        value={focused ? text : fmtDpi(props.value)}
        onChange={(e) => setText(e.target.value)}
        onFocus={(e) => {
          setFocused(true);
          setText(String(props.value));
          requestAnimationFrame(() => e.target.select());
        }}
        onBlur={() => {
          commit();
          setFocused(false);
        }}
        onKeyDown={keys}
      />
      <InputGroupAddon align="inline-end">
        <InputGroupText className="text-xs">DPI</InputGroupText>
      </InputGroupAddon>
    </InputGroup>
  );
}
