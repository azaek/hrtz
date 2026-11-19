import { DataTrending20Regular } from "@fluentui/react-icons";
import { BatteryChart } from "./BatteryChart";
import { date, days, DAY, hours } from "./format";
import { LevelBar } from "./LevelBar";
import { SavingCards } from "./SavingCards";
import { hoursLeft, lifeAt, useBatteryData } from "./useBatteryData";
import { Choice, Group, SettingCard } from "@/components";
import { num, t } from "@/i18n";
import { StatTile } from "@/kit";
import { makerOf } from "@/models";
import { hzText } from "@/rates";
import { useMouse, useStore } from "@/store";

/**
 * Battery at a glance: four figures across the top (the level, time left, how long a charge
 * lasts, and health), then the level over time with each rate's battery life under it, then the
 * saving cards. The numbers come first; the words under them say what they mean. Decided in the
 * Battery lab (DECISIONS.md, "Battery page").
 */
export function BatteryOverview() {
  const device = useStore((x) => x.device);
  const prefs = useStore((x) => x.prefs);
  const saver = useMouse()?.prefs;
  const { summary, range, pickRange, rows, now } = useBatteryData();
  const b = device?.connected ? device.battery : null;
  const hz = device?.settings?.pollingHz;
  const left = hoursLeft(device, summary);
  const life = lifeAt(summary, hz, device);
  // Until hrtz has measured this rate, the figures are the maker's, and say so.
  const maker = makerOf(device);
  const ofUse =
    life?.rated && hz
      ? maker
        ? t("{{maker}}'s figure, {{rate}}", { maker, rate: hzText(hz) })
        : t("The maker's figure, {{rate}}", { rate: hzText(hz) })
      : hz
        ? t("of use at {{rate}}", { rate: hzText(hz) })
        : "";
  const asleep = !!device?.connected && !device.online;
  const unknown = <StatTile.Unknown />;

  return (
    <>
      <div className="tw @container/battery">
        <div className="grid grid-cols-2 gap-2 @min-[680px]/battery:grid-cols-4">
          <StatTile
            label={t("Battery")}
            value={b ? t("{{percent}}%", { percent: num(b.level) }) : unknown}
            detail={!b ? (device?.connected ? t("Move the mouse to wake it") : t("Connect the receiver")) : asleep ? t("Asleep, last reading") : b.charging ? t("Charging") : b.millivolts > 0 ? t("{{volts}} V", { volts: num(b.millivolts / 1000, { minimumFractionDigits: 2, maximumFractionDigits: 2 }) }) : t("On battery")}
          >
            {b && <LevelBar level={b.level} charging={b.charging} low={prefs?.batteryAlerts ? prefs.lowBatteryPercent : undefined} className="my-1" />}
          </StatTile>
          <StatTile
            label={t("Time left")}
            value={b?.charging ? t("Charging") : left != null ? hours(left) : unknown}
            detail={left != null && hz ? ofUse : b?.charging ? t("Shown when unplugged") : t("After an hour of use")}
          />
          <StatTile
            label={t("A charge lasts")}
            value={life ? hours(life.hours) : unknown}
            detail={
              summary?.daysPerCharge && summary.awakePerDay
                ? t("or {{days}} at {{hours}} a day", { days: days(summary.daysPerCharge), hours: hours(summary.awakePerDay) })
                : life && hz
                  ? ofUse
                  : t("After an hour of use")
            }
          />
          <StatTile
            label={t("Health")}
            value={summary?.capacity != null ? t("{{percent}}%", { percent: num(Math.round(summary.capacity * 100)) }) : unknown}
            detail={
              summary?.capacity != null
                ? t("{{n}} charge cycles", { count: summary.cycles, n: num(summary.cycles, { minimumFractionDigits: 1, maximumFractionDigits: 1 }) })
                : summary?.since
                  ? t("Known from about {{date}}", { date: date(summary.since + 30 * DAY) })
                  : t("After a month of use")
            }
          />
        </div>
      </div>

      <Group>
        <SettingCard
          stacked
          icon={<DataTrending20Regular />}
          title={t("Battery level")}
          description={t("The level over time, with charging and battery saver.")}
          below={
            <div className="tw flex flex-col gap-3 pl-9">
              <BatteryChart
                rows={rows}
                days={range}
                now={now}
                summary={summary}
                hz={hz}
                low={prefs?.batteryAlerts ? prefs.lowBatteryPercent : undefined}
                saverAt={saver?.batterySaver && !device?.capabilities?.wired ? saver.batterySaverBelow : undefined}
              />
            </div>
          }
        >
          <Choice
            label={t("Time range")}
            value={range}
            options={[
              { value: 1, label: t("24 hours") },
              { value: 7, label: t("7 days") },
              { value: 30, label: t("30 days") },
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
