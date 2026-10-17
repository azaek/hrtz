import { useEffect, useRef, useState } from "react";
import type { DpiStage, Settings } from "@/api";
import { clamp, newStageValue, type DpiRange } from "@/dpi";
import { commit } from "@/store";
import { LED_PALETTE as PALETTE } from "./palette";
import type { Note } from "./DpiField";

/** A stage as the card holds it: an id that survives re-sorting, so a stage keeps its row,
 *  selection and focus when the list puts itself back in order. */
export type Stage = { id: number; dpi: number; color: string };

let nextId = 1;
const same = (a: { dpi: number; color: string }, b: { dpi: number; color: string }) => a.dpi === b.dpi && a.color.toUpperCase() === b.color.toUpperCase();

/** The mouse's stages, keeping the ids of the ones that haven't changed (by value and colour),
 *  then of whatever was in the same place. */
function reconcile(prev: Stage[], device: DpiStage[]): Stage[] {
  const used = new Set<number>();
  const out: (Stage | null)[] = device.map((d) => {
    const m = prev.find((x) => !used.has(x.id) && same(x, d));
    if (!m) return null;
    used.add(m.id);
    return { ...m };
  });
  return out.map((x, i) => {
    if (x) return x;
    const d = device[i];
    const m = prev[i] && !used.has(prev[i].id) ? prev[i] : undefined;
    if (m) used.add(m.id);
    return { id: m?.id ?? nextId++, dpi: d.dpi, color: d.color };
  });
}

const ascending = (list: Stage[]) => list.every((x, i) => i === 0 || list[i - 1].dpi < x.dpi);
const sorted = (list: Stage[]) => [...list].sort((a, b) => a.dpi - b.dpi);

/**
 * The DPI stages card's state and every change it can make. Changes write straight to the mouse
 * (commit is optimistic), except while dragging the tape: the drag shows live in the card and is
 * written once, when it ends, so the mouse's memory isn't rewritten for every tick.
 */
export function useStages(s: Settings, range: DpiRange) {
  // Shown only for a mouse with DPI stages, so these are there.
  const slots = s.stages ?? [];
  const count = s.dpiStageCount ?? slots.length;
  const inUse = s.dpiStage ?? 0;
  const [stages, setStages] = useState<Stage[]>(() => reconcile([], slots.slice(0, count)));
  const [selectedId, setSelectedId] = useState<number>(() => stages[inUse]?.id ?? stages[0]?.id);
  const [note, setNote] = useState<Note>(null);
  const [removed, setRemoved] = useState<{ stage: Stage; index: number; wasActive: boolean } | null>(null);
  const dragFrom = useRef<Stage[] | null>(null);
  const dragLive = useRef<Stage[] | null>(null);

  // Follow the mouse (a DPI-button press, another write, a profile switch), except mid-drag.
  useEffect(() => {
    if (dragFrom.current) return;
    setStages((prev) => reconcile(prev, slots.slice(0, count)));
  }, [s.stages, s.dpiStageCount]);
  useEffect(() => {
    if (!removed) return;
    const t = setTimeout(() => setRemoved(null), 6000);
    return () => clearTimeout(t);
  }, [removed]);
  useEffect(() => setNote(null), [selectedId]);

  const activeId = stages[clamp(inUse, 0, stages.length - 1)]?.id;
  const selected = stages.find((x) => x.id === selectedId) ?? stages[0];

  /** Put `list` on the mouse. Stages past the count keep their values; `parked` goes first among
   *  them, so a removed stage can come back exactly as it was. */
  const write = (list: Stage[], active = activeId, parked?: Stage) => {
    setStages(list);
    const visible: DpiStage[] = list.map(({ dpi, color }) => ({ dpi, color }));
    const hidden = slots.slice(count);
    const all = [...visible, ...(parked ? [{ dpi: parked.dpi, color: parked.color }] : []), ...hidden].slice(0, slots.length);
    while (all.length < slots.length) all.push(slots[all.length]);
    const index = list.findIndex((x) => x.id === active);
    void commit({ ...s, stages: all, dpiStageCount: list.length, dpiStage: Math.max(0, index) });
  };

  const taken = (id: number) => new Set(stages.filter((x) => x.id !== id).map((x) => x.dpi));

  /** A typed or keyed value. If the stages went up in order, they still do. */
  const setValue = (id: number, dpi: number) => {
    const next = stages.map((x) => (x.id === id ? { ...x, dpi } : x));
    write(ascending(stages) ? sorted(next) : next);
  };
  /** The tape, live: shown in the card, not written yet. */
  const drag = (id: number, dpi: number) => {
    dragFrom.current ??= stages;
    const next = (dragLive.current ?? stages).map((x) => (x.id === id ? { ...x, dpi } : x));
    dragLive.current = next;
    setStages(next);
  };
  /** The tape let go: put the stages in order (if they were) and write once. */
  const dragEnd = () => {
    const from = dragFrom.current;
    const list = dragLive.current;
    dragFrom.current = null;
    dragLive.current = null;
    if (!from || !list) return;
    write(ascending(from) ? sorted(list) : list);
  };
  const setColour = (id: number, color: string) => write(stages.map((x) => (x.id === id ? { ...x, color } : x)));
  const use = (id: number) => write(stages, id);
  const move = (id: number, dir: -1 | 1) => {
    const i = stages.findIndex((x) => x.id === id);
    const j = i + dir;
    if (j < 0 || j >= stages.length) return;
    const next = [...stages];
    [next[i], next[j]] = [next[j], next[i]];
    write(next);
  };
  const add = () => {
    const used = new Set(stages.map((x) => x.color.toUpperCase()));
    const color = (PALETTE.find((c) => !used.has(c.hex)) ?? PALETTE[0]).hex;
    const id = nextId++;
    const next = [...stages, { id, dpi: newStageValue(range, stages.map((x) => x.dpi)), color }];
    write(ascending(stages) ? sorted(next) : next);
    setSelectedId(id);
  };
  const remove = (id: number) => {
    const at = stages.findIndex((x) => x.id === id);
    const stage = stages[at];
    const rest = stages.filter((x) => x.id !== id);
    const wasActive = id === activeId;
    const neighbour = rest[Math.min(at, rest.length - 1)];
    setRemoved({ stage, index: at, wasActive });
    write(rest, wasActive ? neighbour.id : activeId, stage);
    setSelectedId(neighbour.id);
  };
  const undo = () => {
    if (!removed) return;
    const next = [...stages];
    next.splice(removed.index, 0, removed.stage);
    write(next, removed.wasActive ? removed.stage.id : activeId);
    setSelectedId(removed.stage.id);
    setRemoved(null);
  };

  return { stages, activeId, selected, selectedId: selected?.id, setSelectedId, note, setNote, removed, undo, taken, setValue, drag, dragEnd, setColour, use, move, add, remove };
}
