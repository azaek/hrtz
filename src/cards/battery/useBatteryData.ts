import { useEffect, useState } from "react";
import { api, type BatteryEntry, type BatterySummary, type DeviceState } from "@/api";
import { DAY, unixNow } from "./format";

const RANGE_KEY = "hrtz.batteryRange";

/**
 * The battery summary and the chart's rows, reloaded once a minute (the log grows by a row every
 * few minutes), and the chart's range, kept between visits. Until a range is picked, a young log
 * opens on the last day rather than a week of nothing.
 */
export function useBatteryData() {
  const [summary, setSummary] = useState<BatterySummary | null>(null);
  const [range, setRange] = useState<number>(() => Number(localStorage.getItem(RANGE_KEY)) || 7);
  const [rows, setRows] = useState<BatteryEntry[]>([]);
  const [now, setNow] = useState(unixNow);
  useEffect(() => {
    if (!localStorage.getItem(RANGE_KEY) && summary?.since && unixNow() - summary.since < 2 * DAY) setRange(1);
  }, [summary?.since]);
  useEffect(() => {
    const load = () => {
      setNow(unixNow());
      api.batterySummary().then(setSummary, () => {});
      api.batteryHistory(range).then(setRows, () => {});
    };
    load();
    const t = setInterval(() => !document.hidden && load(), 60_000);
    return () => clearInterval(t);
  }, [range]);
  const pickRange = (d: number) => {
    setRange(d);
    localStorage.setItem(RANGE_KEY, String(d));
  };
  return { summary, range, pickRange, rows, now };
}

export const lifeAt = (summary: BatterySummary | null, hz: number | undefined) => summary?.life.find((l) => l.hz === hz);

/** Hours of use left now at the rate in use, once that rate has been measured. */
export function hoursLeft(device: DeviceState | null, summary: BatterySummary | null) {
  const b = device?.battery;
  const life = lifeAt(summary, device?.settings?.pollingHz);
  return b && life && !b.charging ? (life.hours * b.level) / 100 : null;
}
