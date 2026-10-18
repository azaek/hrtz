import { useEffect, useRef, useState } from "react";
import { Input } from "@fluentui/react-components";
import { Keyboard16Regular } from "@fluentui/react-icons";
import { comboLabel, KEY_USAGE, MODIFIER_BIT } from "@/hid";

type Combo = { modifiers: number; keys: number[] };

/** Keys in a combination, modifiers counted once each whichever side was pressed. */
const count = (c: Combo) => c.keys.length + [0x11, 0x22, 0x44, 0x88].filter((b) => c.modifiers & b).length;

/**
 * A key combination field: focus it, press up to three keys together, let go. The combination is
 * saved when every key is up (or when you leave the field mid-press), and the next press starts a
 * new one. Tab still moves on.
 */
export function KeyRecorder(props: {
  label: string;
  value: Combo;
  autoFocus?: boolean;
  disabled?: boolean;
  /** One key besides any modifiers (a Logitech's), not three counting them. */
  oneKey?: boolean;
  onCommit: (v: Combo) => void;
  onLeaveEmpty?: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  const held = useRef(new Set<string>());
  // The combination being pressed, in a ref too: keys arrive faster than renders.
  const pressing = useRef<Combo | null>(null);
  const [draft, setDraftState] = useState<Combo | null>(null);
  const setDraft = (c: Combo | null) => ((pressing.current = c), setDraftState(c));
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (props.autoFocus) ref.current?.focus();
  }, [props.autoFocus]);

  const commit = () => {
    const c = pressing.current;
    setDraft(null);
    if (c && count(c)) props.onCommit(c);
  };
  const shown = draft ?? props.value;
  return (
    <Input
      ref={ref}
      readOnly
      disabled={props.disabled}
      aria-label={`${props.label}. Press the keys together, then let go.`}
      contentBefore={<Keyboard16Regular />}
      value={count(shown) ? comboLabel(shown.modifiers, shown.keys) : ""}
      placeholder={focused ? "Press up to three keys" : "Click here, then press keys"}
      style={{ width: 240 }}
      onFocus={() => setFocused(true)}
      onBlur={() => {
        setFocused(false);
        held.current.clear();
        const had = pressing.current;
        commit();
        if (!had && !count(props.value)) props.onLeaveEmpty?.();
      }}
      onKeyDown={(e) => {
        if (e.key === "Tab") return;
        e.preventDefault();
        if (e.repeat) return;
        const start = held.current.size === 0 ? { modifiers: 0, keys: [] } : (pressing.current ?? { modifiers: 0, keys: [] });
        held.current.add(e.code);
        const mod = MODIFIER_BIT[e.code];
        const usage = KEY_USAGE[e.code];
        let next = start;
        const fits = (c: Combo) => (props.oneKey ? c.keys.length <= 1 : count(c) <= 3);
        if (mod && fits({ ...start, modifiers: start.modifiers | mod })) next = { ...start, modifiers: start.modifiers | mod };
        else if (usage && !start.keys.includes(usage) && fits({ ...start, keys: [...start.keys, usage] })) next = { ...start, keys: [...start.keys, usage] };
        setDraft(next);
      }}
      onKeyUp={(e) => {
        held.current.delete(e.code);
        if (held.current.size === 0) commit();
      }}
    />
  );
}
