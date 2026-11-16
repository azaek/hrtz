import { useEffect, useState } from "react";
import { api, type ActivityDay, type ClickDay } from "@/api";

// Your week: the last seven days of the activity log and the click log, against the seven before.

export type WeekGame = { exe: string; cm: number; movingS: number; topMps: number; turns: number };

export type Week = {
  /** The seven days, oldest first, today last; a day without use is there with nothing. */
  days: { date: string; cm: number }[];
  cm: number;
  movingS: number;
  topMps: number;
  /** The day of the fastest moment, and the game it was in, if it was in one. */
  topDate: string | null;
  topGame: string | null;
  /** The seven days before, for comparing. */
  prevCm: number;
  clicks: number;
  /** Left, right, middle, back, forward. */
  buttons: number[];
  /** Most played first. */
  games: WeekGame[];
};

/** `d` as the logs date it: "2026-11-16", local. */
export const dateOf = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

export function weekOf(activity: ActivityDay[], clicks: ClickDay[], today = new Date()): Week {
  const dates = Array.from({ length: 14 }, (_, i) => dateOf(new Date(today.getFullYear(), today.getMonth(), today.getDate() - 13 + i)));
  const these = new Set(dates.slice(7));
  const before = new Set(dates.slice(0, 7));
  const byDate = new Map(activity.map((d) => [d.date, d]));
  const week = activity.filter((d) => these.has(d.date));
  const games = new Map<string, WeekGame>();
  let top: { mps: number; date: string | null; game: string | null } = { mps: 0, date: null, game: null };
  for (const d of week) {
    if (d.topMps > top.mps) top = { mps: d.topMps, date: d.date, game: d.games.find((g) => g.topMps >= d.topMps)?.exe ?? null };
    for (const g of d.games) {
      const w = games.get(g.exe) ?? { exe: g.exe, cm: 0, movingS: 0, topMps: 0, turns: 0 };
      w.cm += g.cm;
      w.movingS += g.movingS;
      w.turns += g.turns;
      w.topMps = Math.max(w.topMps, g.topMps);
      games.set(g.exe, w);
    }
  }
  const buttons = [0, 0, 0, 0, 0];
  for (const d of clicks.filter((c) => these.has(c.date))) d.buttons.forEach((b, i) => (buttons[i] += b.presses));
  return {
    days: dates.slice(7).map((date) => ({ date, cm: byDate.get(date)?.cm ?? 0 })),
    cm: week.reduce((n, d) => n + d.cm, 0),
    movingS: week.reduce((n, d) => n + d.movingS, 0),
    topMps: top.mps,
    topDate: top.date,
    topGame: top.game,
    prevCm: activity.filter((d) => before.has(d.date)).reduce((n, d) => n + d.cm, 0),
    clicks: buttons.reduce((n, b) => n + b, 0),
    buttons,
    games: [...games.values()].sort((a, b) => b.cm - a.cm),
  };
}

/** The activity and click logs, read now and every minute while shown. */
export function useWeek(): Week | null {
  const [week, setWeek] = useState<Week | null>(null);
  useEffect(() => {
    const load = () =>
      Promise.all([api.activityDays(), api.clickHistory()]).then(
        ([a, c]) => setWeek(weekOf(a, c)),
        () => {},
      );
    void load();
    const t = setInterval(() => !document.hidden && void load(), 60_000);
    return () => clearInterval(t);
  }, []);
  return week;
}

const fmt = (n: number) => Math.round(n).toLocaleString("en-US");

/** "4.2 km", "830 m". */
export const distance = (cm: number) => (cm >= 100_000 ? `${(cm / 100_000).toFixed(cm >= 1_000_000 ? 0 : 1)} km` : `${fmt(cm / 100)} m`);

/** "6 h 20 min", "45 min". */
export const timeText = (s: number) => {
  const m = Math.round(s / 60);
  return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`;
};

/** How far that is, in something you can picture: football pitches of 105 m. */
export const pitches = (cm: number) => {
  const n = Math.round(cm / 10_500);
  return n >= 2 ? `about ${fmt(n)} football pitches end to end` : "about one football pitch";
};

/** "18% more than the week before", or nothing to compare with. */
export function versus(cm: number, prev: number): string | null {
  if (prev < 1_000) return null;
  const change = (cm - prev) / prev;
  if (Math.abs(change) < 0.05) return "about the same as the week before";
  return `${Math.round(Math.abs(change) * 100)}% ${change > 0 ? "more" : "less"} than the week before`;
}

/** "Tuesday", from a log date. */
export const dayName = (date: string) => {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("en-US", { weekday: "long" });
};

/** The week in a line to paste anywhere. */
export function summaryText(w: Week, name: (exe: string) => string): string {
  const parts = [`${distance(w.cm)}`, `${timeText(w.movingS)} moving`];
  if (w.topMps > 0) parts.push(`fastest ${w.topMps.toFixed(1)} m/s`);
  const turns = w.games.find((g) => g.turns >= 1);
  if (turns) parts.push(`${fmt(turns.turns)} full turns in ${name(turns.exe)}`);
  if (w.clicks) parts.push(`${fmt(w.clicks)} clicks`);
  return `My mouse this week: ${parts.join(", ")}. (hrtz)`;
}
