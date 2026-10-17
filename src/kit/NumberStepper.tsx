import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { Minus, Plus } from "lucide-react";
import { InputGroup, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { cn } from "@/lib/utils";
import { fieldLook } from "./field";

/** How long a value sits still before it's written. */
const SETTLE_MS = 400;

/**
 * A number with minus and plus either side, in one field: type it, step it, or hold a button to
 * repeat. Steps show at once; `onChange` gets the value once it settles (let go, Enter, leaving
 * the field, or a short pause), so holding a button doesn't send a write per step.
 * ↑/↓ step, PgUp/PgDn five steps, Home/End the ends, the wheel while focused.
 */
export function NumberStepper(props: {
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Shown after the value, as in "8 ms". */
  unit?: string;
  label: string;
  onChange: (v: number) => void;
  disabled?: boolean;
  className?: string;
}) {
  const step = props.step ?? 1;
  const ref = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(props.value);
  const [text, setText] = useState(String(props.value));
  const [focused, setFocused] = useState(false);
  const latest = useRef(props.value);
  const settle = useRef<number | null>(null);
  const hold = useRef<number | null>(null);

  // Follow the mouse unless a change is still on its way.
  useEffect(() => {
    if (settle.current === null && hold.current === null && !focused) {
      setDraft(props.value);
      latest.current = props.value;
    }
  }, [props.value, focused]);
  useEffect(() => () => (clearTimeout(settle.current ?? 0), clearTimeout(hold.current ?? 0)), []);

  const clampStep = (v: number) => Math.min(props.max, Math.max(props.min, Math.round(v / step) * step));
  const flush = () => {
    if (settle.current !== null) clearTimeout(settle.current);
    settle.current = null;
    if (latest.current !== props.value) props.onChange(latest.current);
  };
  const show = (v: number, write: "now" | "soon" | "later") => {
    latest.current = v;
    setDraft(v);
    setText(String(v));
    if (settle.current !== null) clearTimeout(settle.current);
    settle.current = null;
    if (write === "now") flush();
    else if (write === "soon") settle.current = window.setTimeout(flush, SETTLE_MS);
  };
  const by = (n: number, write: "soon" | "later" = "soon") => show(clampStep(latest.current + n * step), write);

  // Holding a button: one step now, then repeats until let go; written when let go.
  const holdStart = (dir: 1 | -1, e: PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    by(dir, "later");
    const tick = () => {
      by(dir, "later");
      hold.current = window.setTimeout(tick, 90);
    };
    hold.current = window.setTimeout(tick, 400);
  };
  const holdEnd = () => {
    if (hold.current === null) return;
    clearTimeout(hold.current);
    hold.current = null;
    flush();
  };

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const wheel = (e: WheelEvent) => {
      if (document.activeElement !== el) return;
      e.preventDefault();
      by(e.deltaY < 0 ? 1 : -1);
    };
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  const commitText = () => {
    const v = parseFloat(text.replace(/[^\d.-]/g, ""));
    show(Number.isFinite(v) ? clampStep(v) : latest.current, "now");
  };
  const keys = (e: KeyboardEvent<HTMLInputElement>) => {
    const map: Record<string, () => void> = {
      ArrowUp: () => by(1),
      ArrowDown: () => by(-1),
      PageUp: () => by(5),
      PageDown: () => by(-5),
      Home: () => show(props.min, "soon"),
      End: () => show(props.max, "soon"),
      Enter: commitText,
      Escape: () => (setText(String(latest.current)), ref.current?.blur()),
    };
    if (map[e.key]) {
      e.preventDefault();
      map[e.key]();
    }
  };

  // "8 ms", but "20%".
  const unit = props.unit ? (props.unit === "%" ? "%" : ` ${props.unit}`) : "";
  const button = (dir: 1 | -1) => (
    <InputGroupButton
      size="icon-sm"
      // A flush end of the field: full height inside its border, the field's inner corner on the
      // outside, square against the number, so the hover wash meets the edges evenly.
      className={cn("h-full w-8 shrink-0 border-0 rounded-none", dir > 0 ? "rounded-r-[3px]" : "rounded-l-[3px]")}
      aria-label={dir > 0 ? `More ${props.label.toLowerCase()}` : `Less ${props.label.toLowerCase()}`}
      title={dir > 0 ? "More (hold to repeat)" : "Less (hold to repeat)"}
      disabled={props.disabled || (dir > 0 ? draft >= props.max : draft <= props.min)}
      onPointerDown={(e) => holdStart(dir, e)}
      onPointerUp={holdEnd}
      onPointerLeave={holdEnd}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), by(dir))}
    >
      {dir > 0 ? <Plus /> : <Minus />}
    </InputGroupButton>
  );

  return (
    <InputGroup
      className={cn(
        fieldLook,
        "w-32",
        // shadcn fades the whole group when anything in it is disabled; at a limit only that end's
        // button is, so keep the field as it is unless the stepper itself is off.
        !props.disabled && "has-disabled:bg-[var(--colorNeutralBackground1)] has-disabled:opacity-100 dark:has-disabled:bg-[var(--colorNeutralBackground1)]",
        props.className,
      )}
    >
      {button(-1)}
      <InputGroupInput
        ref={ref}
        role="spinbutton"
        inputMode="numeric"
        aria-label={props.label}
        aria-valuemin={props.min}
        aria-valuemax={props.max}
        aria-valuenow={draft}
        aria-valuetext={`${draft}${unit}`}
        disabled={props.disabled}
        className="h-full px-1 text-center font-medium tabular-nums"
        value={focused ? text : `${draft}${unit}`}
        onChange={(e) => setText(e.target.value)}
        onFocus={(e) => {
          setFocused(true);
          setText(String(draft));
          requestAnimationFrame(() => e.target.select());
        }}
        onBlur={() => {
          commitText();
          setFocused(false);
        }}
        onKeyDown={keys}
      />
      {button(1)}
    </InputGroup>
  );
}
