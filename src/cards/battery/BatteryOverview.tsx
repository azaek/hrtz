import { DataTrending20Regular } from "@fluentui/react-icons";
import { BatteryChart } from "./BatteryChart";
import { date, days, DAY, hours } from "./format";
import { LevelBar } from "./LevelBar";
import { SavingCards } from "./SavingCards";
import { hoursLeft, lifeAt, useBatteryData } from "./useBatteryData";
import { Choice, Group, SettingCard } from "@/components";
import { StatTile } from "@/kit";
import { hzText, rateColor } from "@/rates";
import { useStore } from "@/store";

/**
 * Battery at a glance: four figures across the top (the level, time left, how long a charge
 * lasts, and health), then the level over time with each rate's battery life under it, then the
 * saving cards. The numbers come first; the words under them say what they mean. Decided in the
 * Battery lab (DECISIONS.md, "Battery page").
 */
export function BatteryOverview() {
  const device = useStore((x) => x.device);
  const prefs = useStore((x) => x.prefs);
  const { summary, range, pickRange, rows, now } = useBatteryData();
  const b = device?.connected ? device.battery : null;
  const hz = device?.settings?.pollingHz;
  const left = hoursLeft(device, summary);
  const life = lifeAt(summary, hz);
  const asleep = !!device?.connected && !device.online;
  const unknown = <StatTile.Unknown />;

  return (
    <>
      <div className="tw @container/battery">
        <div className="grid grid-cols-2 gap-2 @min-[680px]/battery:grid-cols-4">
          <StatTile
            label="Battery"
            value={b ? `${b.level}%` : unknown}
            detail={!b ? (device?.connected ? "Move the mouse to wake it" : "Connect the receiver") : asleep ? "Asleep, last reading" : b.charging ? "Charging" : b.millivolts > 0 ? `${(b.millivolts / 1000).toFixed(2)} V` : "On battery"}
          >
            {b && <LevelBar level={b.level} charging={b.charging} low={prefs?.batteryAlerts ? prefs.lowBatteryPercent : undefined} className="my-1" />}
          </StatTile>
          <StatTile
            label="Time left"
            value={b?.charging ? "Charging" : left != null ? hours(left) : unknown}
            detail={left != null && hz ? `of use at ${hzText(hz)}` : b?.charging ? "Shown when unplugged" : "After an hour of use"}
          />
          <StatTile
            label="A charge lasts"
            value={life ? hours(life.hours) : unknown}
            detail={
              summary?.daysPerCharge && summary.awakePerDay
                ? `or ${days(summary.daysPerCharge)} at ${hours(summary.awakePerDay)} a day`
                : life && hz
                  ? `of use at ${hzText(hz)}`
                  : "After an hour of use"
            }
          />
          <StatTile
            label="Health"
            value={summary?.capacity != null ? `${Math.round(summary.capacity * 100)}%` : unknown}
            detail={
              summary?.capacity != null
                ? `${summary.cycles.toFixed(1)} charge cycles`
                : summary?.since
                  ? `Known from about ${date(summary.since + 30 * DAY)}`
                  : "After a month of use"
            }
          />
        </div>
      </div>

      <Group>
        <SettingCard
          stacked
          icon={<DataTrending20Regular />}
          title="Battery level"
          description="Charging, awake time and polling rate over time."
          below={
            <div className="tw flex flex-col gap-3 pl-9">
              <BatteryChart rows={rows} from={now - range * DAY} now={now} />
              {summary && summary.life.length > 0 && (
                <div className="flex flex-wrap gap-x-6 gap-y-1 border-t pt-3 text-sm tabular-nums">
                  {summary.life.map((l) => (
                    <span key={l.hz} className="flex items-center gap-2">
                      <span aria-hidden className="size-2 rounded-full" style={{ background: rateColor(l.hz) }} />
                      {hzText(l.hz)}: about {hours(l.hours)} a charge
                      <span className="text-xs text-muted-foreground">(over {hours(l.measuredHours)})</span>
                    </span>
                  ))}
                  {summary.standbyPerDay != null && <span className="text-muted-foreground">Asleep: {Math.max(0.1, summary.standbyPerDay).toFixed(1)}% a day</span>}
                </div>
              )}
            </div>
          }
        >
          <Choice
            label="Time range"
            value={range}
            options={[
              { value: 1, label: "24 hours" },
              { value: 7, label: "7 days" },
              { value: 30, label: "30 days" },
            ]}
            onChange={pickRange}
          />
        </SettingCard>
      </Group>

      {prefs && (
        <Group>
          <SavingCards prefs={prefs} />
        </Group>
      )}
    </>
  );
}
