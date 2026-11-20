import type { BatterySummary } from "@/api";
import { num, t } from "@/i18n";
import { hzText } from "@/rates";
import { duration, hours } from "./format";

/** The time awake and asleep over the range shown, in seconds, if the chart saw any of it. */
export type Totals = { awake: number; asleep: number } | null;

/**
 * The figures under the battery chart, as picked in the battery chart lab (DECISIONS.md,
 * "Battery chart"): time awake and asleep over the range at either end of a bar split between
 * them, then how long a charge lasts at each rate measured (the rate in use marked) and the drain
 * asleep.
 */
export function BatteryFooter({ totals, summary, hz }: { totals: Totals; summary: BatterySummary | null; hz?: number }) {
  const life = summary?.life ?? [];
  const standby = summary?.standbyPerDay ?? null;
  const total = totals ? totals.awake + totals.asleep : 0;
  if (!total && !life.length && standby == null) return null;

  // 6 px more above than the card's gap, and 10 px under, off the card's edge.
  return (
    <div className="flex flex-col gap-3" style={{ marginTop: 6, paddingBottom: 10 }}>
      {totals && total > 0 && (
        <div className="flex flex-col gap-1.5">
          <div className="flex justify-between text-sm tabular-nums">
            <span>
              <span className="text-muted-foreground">{t("Awake")}</span> <span className="font-semibold">{duration(totals.awake / 3600)}</span>
            </span>
            <span>
              <span className="text-muted-foreground">{t("Asleep")}</span> <span className="font-semibold">{duration(totals.asleep / 3600)}</span>
            </span>
          </div>
          <div className="flex h-1.5 overflow-hidden rounded-full bg-foreground/10" role="presentation">
            <div className="h-full rounded-full bg-foreground/60" style={{ width: `${(totals.awake / total) * 100}%` }} />
          </div>
        </div>
      )}
      {(life.length > 0 || standby != null) && (
        <div className="flex flex-wrap gap-x-6 gap-y-1 text-sm tabular-nums">
          {life.map((l) => (
            <span key={l.hz} className="flex items-center gap-1.5" title={t("Measured over {{hours}} of use", { hours: hours(l.measuredHours) })}>
              {l.hz === hz && <span aria-label={t("In use")} className="size-1.5 rounded-full" style={{ background: "var(--brand)" }} />}
              {t("{{rate}}: ~{{hours}}", { rate: hzText(l.hz), hours: hours(l.hours) })}
            </span>
          ))}
          {standby != null && (
            <span className="text-muted-foreground">{t("Standby: {{percent}}% a day", { percent: num(Math.max(0.1, standby), { minimumFractionDigits: 1, maximumFractionDigits: 1 }) })}</span>
          )}
        </div>
      )}
    </div>
  );
}
