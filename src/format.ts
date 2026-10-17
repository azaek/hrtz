export function formatDuration(s: number) {
  return s < 60 ? `${s} seconds` : s === 60 ? "1 minute" : `${s / 60} minutes`;
}

/** When a mouse was last connected, from seconds since 1970: "today at 9:41", "yesterday",
 *  "Oct 15". */
export function seenAt(secs: number, now = new Date()): string {
  const t = new Date(secs * 1000);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(now) - day(t)) / 86_400_000);
  if (days <= 0) return `today at ${t.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })}`;
  if (days === 1) return "yesterday";
  return t.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
