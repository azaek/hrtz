import i18n, { num, t } from "./i18n";

export function formatDuration(s: number) {
  return s < 60
    ? t("{{count}} seconds", { count: s, defaultValue_one: "{{count}} second" })
    : t("{{n}} minutes", { count: s / 60, n: num(s / 60), defaultValue_one: "{{n}} minute" });
}

/** When a mouse was last connected, from seconds since 1970: "today at 9:41", "yesterday",
 *  "Oct 15". */
export function seenAt(secs: number, now = new Date()): string {
  const at = new Date(secs * 1000);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(now) - day(at)) / 86_400_000);
  if (days <= 0) return t("today at {{time}}", { time: new Intl.DateTimeFormat(i18n.language, { hour: "numeric", minute: "2-digit" }).format(at) });
  if (days === 1) return t("yesterday");
  return new Intl.DateTimeFormat(i18n.language, { month: "short", day: "numeric" }).format(at);
}
