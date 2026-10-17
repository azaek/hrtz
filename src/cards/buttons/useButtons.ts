import { useEffect, useRef, useState } from "react";
import type { ButtonAction, Settings } from "@/api";
import { actionLabel } from "@/hid";
import { commit } from "@/store";
import { DEFAULT_ACTIONS, isLeftClick, sameAction } from "./functions";

/** How long Undo stays offered after a change. */
const UNDO_MS = 6000;

/**
 * The Buttons page's edit state. A change writes to the mouse at once and offers Undo. Changing
 * the only left click is refused, out loud. A key combination isn't written until keys are
 * recorded, so picking it only marks the button as waiting for keys.
 */
export function useButtons(s: Settings, physical: [number, string][]) {
  const [note, setNote] = useState<{ text: string; warn?: boolean } | null>(null);
  const [undo, setUndo] = useState<{ text: string; prev: ButtonAction[] } | null>(null);
  const [waiting, setWaiting] = useState<number | null>(null);
  const timer = useRef<number | null>(null);
  useEffect(() => () => clearTimeout(timer.current ?? 0), []);

  const nameOf = (i: number) => physical.find(([b]) => b === i)?.[1] ?? `Button ${i + 1}`;
  const write = (buttons: ButtonAction[], undoText: string) => {
    setUndo({ text: undoText, prev: s.buttons });
    clearTimeout(timer.current ?? 0);
    timer.current = window.setTimeout(() => setUndo(null), UNDO_MS);
    commit({ ...s, buttons });
  };
  const onlyLeft = (i: number) => isLeftClick(s.buttons[i]) && physical.filter(([b]) => isLeftClick(s.buttons[b])).length === 1;

  const set = (i: number, a: ButtonAction) => {
    if (a.kind === "keys" && !a.modifiers && !a.keys.length) {
      setWaiting(i);
      return;
    }
    setWaiting(null);
    if (sameAction(s.buttons[i], a)) return;
    if (onlyLeft(i) && !isLeftClick(a)) {
      setNote({ text: `${nameOf(i)} is your only left click. Set another button to Left click first.`, warn: true });
      return;
    }
    setNote(null);
    write(
      s.buttons.map((b, j) => (j === i ? a : b)),
      `${nameOf(i)} is set to ${actionLabel(a)}`,
    );
  };

  const isDefault = physical.every(([i]) => !DEFAULT_ACTIONS[i] || sameAction(s.buttons[i], DEFAULT_ACTIONS[i]));
  const restore = () => {
    setWaiting(null);
    setNote(null);
    write(
      s.buttons.map((b, i) => (physical.some(([p]) => p === i) && DEFAULT_ACTIONS[i] ? DEFAULT_ACTIONS[i] : b)),
      "Default buttons restored",
    );
  };
  const revert = () => {
    if (!undo) return;
    setUndo(null);
    commit({ ...s, buttons: undo.prev });
  };

  /** What a button's editor shows: its action, or an empty key combination while it waits for keys. */
  const shown = (i: number): ButtonAction => (waiting === i ? { kind: "keys", modifiers: 0, keys: [] } : s.buttons[i]);
  return { set, shown, waiting, cancelWaiting: () => setWaiting(null), onlyLeft, note, undo, revert, isDefault, restore };
}
