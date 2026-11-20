import { useEffect, useState } from "react";
import { api, type ClickDay } from "@/api";
import { msg, num, t } from "@/i18n";
import type { TestResult } from "./result";

// Double clicks over days: the click log Rust keeps (clicks.rs), counted in the background while
// the switch is on, summed over the last week.

export const NAMES = [msg("Left"), msg("Right"), msg("Middle"), msg("Back"), msg("Forward")];
/** Days a week's count covers, today among them. */
const WEEK = 7;

export type ButtonWeek = { name: string; presses: number; doubles: number; worstMs: number };

const localDate = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

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
  if (!bad.length) return { presses, doubles, text: t("No doubles in {{n}} presses", { count: presses, n: num(presses), defaultValue_one: "No doubles in {{n}} press" }) };
  const who = bad.length === 1 ? t("all on {{button}}", { button: t(bad[0].name, { context: "button" }) }) : bad.map((b) => `${t(b.name, { context: "button" })} ${b.doubles}`).join(", ");
  return { presses, doubles, text: t("{{count}} doubles in {{presses}} presses, {{who}}", { count: doubles, presses: num(presses), who, defaultValue_one: "{{count}} double in {{presses}} presses, {{who}}" }) };
}

/** The Lab list's line from the week, once there's a press in it. */
export function weekResult(week: ButtonWeek[]): TestResult | null {
  const { presses, doubles } = weekText(week);
  if (!presses) return null;
  const bad = week.filter((b) => b.doubles > 0);
  if (!doubles) return { value: t("No doubles this week"), detail: t("{{n}} presses", { count: presses, n: num(presses), defaultValue_one: "{{n}} press" }), tone: "good" };
  return {
    value: t("{{count}} doubles this week", { count: doubles, defaultValue_one: "{{count}} double this week" }),
    detail: bad.length === 1 ? t("All on {{button}}", { button: t(bad[0].name, { context: "button" }) }) : bad.map((b) => `${t(b.name, { context: "button" })} ${b.doubles}`).join(", "),
    tone: "warn",
  };
}
