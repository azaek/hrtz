import { useEffect, useState } from "react";
import { api, type ClickDay } from "@/api";
import type { TestResult } from "./result";

// Double clicks over days: the click log Rust keeps (clicks.rs), counted in the background while
// the switch is on, summed over the last week.

export const NAMES = ["Left", "Right", "Middle", "Back", "Forward"];
/** Days a week's count covers, today among them. */
const WEEK = 7;

export type ButtonWeek = { name: string; presses: number; doubles: number; worstMs: number };

const localDate = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const fmt = (n: number) => n.toLocaleString("en-US");

/** The last seven days of the log, per button. */
export function weekOf(days: ClickDay[], today = new Date()): ButtonWeek[] {
  const from = new Date(today);
  from.setDate(from.getDate() - (WEEK - 1));
  const recent = days.filter((d) => d.date >= localDate(from));
  return NAMES.map((name, i) => ({
    name,
    presses: recent.reduce((a, d) => a + (d.buttons[i]?.presses ?? 0), 0),
    doubles: recent.reduce((a, d) => a + (d.buttons[i]?.doubles ?? 0), 0),
    worstMs: recent.reduce((a, d) => Math.max(a, d.buttons[i]?.worstMs ?? 0), 0),
  }));
}

/** The log, read now and every 5 s while the window shows: it fills in the background. */
export function useClickWeek(): ButtonWeek[] | null {
  const [days, setDays] = useState<ClickDay[] | null>(null);
  useEffect(() => {
    const read = () => api.clickHistory().then(setDays, () => {});
    read();
    const t = setInterval(() => !document.hidden && read(), 5000);
    return () => clearInterval(t);
  }, []);
  return days && weekOf(days);
}

/** The week in a few words: "No doubles in 3,412 presses", or which buttons doubled. */
export function weekText(week: ButtonWeek[]) {
  const presses = week.reduce((a, b) => a + b.presses, 0);
  const bad = week.filter((b) => b.doubles > 0);
  const doubles = bad.reduce((a, b) => a + b.doubles, 0);
  if (!bad.length) return { presses, doubles, text: `No doubles in ${fmt(presses)} ${presses === 1 ? "press" : "presses"}` };
  const who = bad.length === 1 ? `all on ${bad[0].name}` : bad.map((b) => `${b.name} ${b.doubles}`).join(", ");
  return { presses, doubles, text: `${doubles} ${doubles === 1 ? "double" : "doubles"} in ${fmt(presses)} presses, ${who}` };
}

/** The Lab list's line from the week, once there's a press in it. */
export function weekResult(week: ButtonWeek[]): TestResult | null {
  const { presses, doubles } = weekText(week);
  if (!presses) return null;
  const bad = week.filter((b) => b.doubles > 0);
  if (!doubles) return { value: "No doubles this week", detail: `${fmt(presses)} presses`, tone: "good" };
  return { value: `${doubles} ${doubles === 1 ? "double" : "doubles"} this week`, detail: bad.length === 1 ? `All on ${bad[0].name}` : bad.map((b) => `${b.name} ${b.doubles}`).join(", "), tone: "warn" };
}
