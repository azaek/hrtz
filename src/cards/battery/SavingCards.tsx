import { Dropdown, Option, Switch } from "@fluentui/react-components";
import { BatterySaver20Regular, BatteryWarning20Regular, WeatherMoon20Regular } from "@fluentui/react-icons";
import type { Prefs } from "@/api";
import { Choice, SettingCard } from "@/components";
import { NumberStepper } from "@/kit";
import { hzText } from "@/rates";
import { commit, savePrefs, useStore } from "@/store";

/** "10 s", "1 min". */
const shortTime = (s: number) => (s < 60 ? `${s} s` : `${s / 60} min`);

/** Battery saver, how soon the mouse sleeps, and the low battery alert, as settings cards. */
export function SavingCards({ prefs }: { prefs: Prefs }) {
  const device = useStore((x) => x.device);
  const caps = device?.capabilities;
  const set = (patch: Partial<Prefs>) => savePrefs({ ...prefs, ...patch });
  // Only the rates below what this connection tops out at: a 4K receiver can drop to 2,000 Hz,
  // a 2K one to 1,000.
  const rates = caps ? caps.pollingHz.filter((hz) => hz < caps.maxPollingHz) : [];
  const lowered = prefs.batterySaverRestore;
  // A connection that tops out at 1,000 Hz has nothing worth lowering: the saver only exists for
  // the high rates that cost the battery.
  const showSaver = !(caps && !caps.wired && caps.maxPollingHz <= 1000 && !lowered);
  // One line: the level and rate are in the controls beside it.
  const saverText = caps?.wired
    ? "Not needed while wired."
    : lowered
      ? `Lowered from ${hzText(lowered)} until charged.`
      : prefs.batterySaver
        ? "Lowers the polling rate on low battery, until charged."
        : "Lowers the polling rate when the battery is low.";
  const s = device?.settings;
  // The sleep timer (flash 0xBB) was measured on a Z1 PRO MAX (2026-10-17); other models offer none.
  const sleepTimes = caps?.sleepTimesS ?? [];
  // A value set elsewhere stays one of the choices, so it shows as chosen.
  const sleepChoices = s && !sleepTimes.includes(s.sleepS) ? [...sleepTimes, s.sleepS].sort((a, b) => a - b) : sleepTimes;
  return (
    <>
      {showSaver && (
        <SettingCard icon={<BatterySaver20Regular />} title="Battery saver" description={saverText}>
          {prefs.batterySaver && !caps?.wired && (
            <div className="tw flex items-center gap-2">
              <NumberStepper label="Battery level to start saving at" unit="%" min={5} max={50} step={5} value={prefs.batterySaverBelow} onChange={(v) => set({ batterySaverBelow: v })} />
              <Dropdown
                aria-label="Polling rate while saving"
                style={{ minWidth: 128 }}
                value={hzText(prefs.batterySaverHz)}
                selectedOptions={[String(prefs.batterySaverHz)]}
                onOptionSelect={(_, d) => d.optionValue && set({ batterySaverHz: Number(d.optionValue) })}
              >
                {rates.map((hz) => (
                  <Option key={hz} value={String(hz)}>
                    {hzText(hz)}
                  </Option>
                ))}
              </Dropdown>
            </div>
          )}
          <Switch checked={prefs.batterySaver} disabled={caps?.wired} onChange={(_, d) => set({ batterySaver: d.checked })} aria-label="Battery saver" />
        </SettingCard>
      )}
      {s && sleepTimes.length > 0 && !caps?.wired && (
        <SettingCard
          icon={<WeatherMoon20Regular />}
          title="Sleep after"
          description={device?.online ? "Idle time before the mouse sleeps. Longer uses more battery." : "Wake the mouse to change this."}
        >
          <Choice
            label="Sleep after"
            value={s.sleepS}
            disabled={!device?.online}
            options={sleepChoices.map((t) => ({ value: t, label: shortTime(t) }))}
            onChange={(t) => void commit({ ...s, sleepS: t })}
          />
        </SettingCard>
      )}
      <SettingCard icon={<BatteryWarning20Regular />} title="Low battery alert" description="Notifies you once when the battery drops to this level.">
        {prefs.batteryAlerts && <NumberStepper label="Low battery level" unit="%" min={5} max={50} step={5} value={prefs.lowBatteryPercent} onChange={(v) => set({ lowBatteryPercent: v })} />}
        <Switch checked={prefs.batteryAlerts} onChange={(_, d) => set({ batteryAlerts: d.checked })} aria-label="Low battery alert" />
      </SettingCard>
    </>
  );
}
