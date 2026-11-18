import { dateLocale, num, t } from "@/i18n";

export const DAY = 86_400;

export const unixNow = () => Math.floor(Date.now() / 1000);
/** 0.4 → "24 min", 7.6 → "8 h". */
export const hours = (h: number) => (h < 1.5 ? t("{{n}} min", { n: num(Math.round(h * 60)) }) : t("{{n}} h", { n: num(Math.round(h)) }));
/** 1.2 → "29 hours", 5.12 → "5.1 days". */
export const days = (d: number) => {
  if (d < 1.5) return t("{{count}} hours", { count: Math.round(d * 24) });
  const n = d < 10 ? Math.round(d * 10) / 10 : Math.round(d);
  return t("{{n}} days", { count: n, n: num(n, { maximumFractionDigits: 1 }) });
};
/** Unix seconds → "15 Sept". */
export const date = (t: number) => new Date(t * 1000).toLocaleDateString(dateLocale(), { day: "numeric", month: "short" });
