import { Dropdown, Option, Switch } from "@fluentui/react-components";
import { BatterySaver20Regular, BatteryWarning20Regular, WeatherMoon20Regular } from "@fluentui/react-icons";
import { topRate, type MousePrefs, type Prefs } from "@/api";
import { Choice, SettingCard } from "@/components";
import { num, t } from "@/i18n";
import { NumberStepper } from "@/kit";
import { hzText } from "@/rates";
import { commit, saveMousePrefs, savePrefs, useMouse, useStore } from "@/store";

/** "10 s", "1 min". */
const shortTime = (s: number) => (s < 60 ? t("{{n}} s", { n: num(s) }) : t("{{n}} min", { n: num(s / 60) }));

/** Battery saver, how soon the mouse sleeps, and the low battery alert, as settings cards. */
export function SavingCards({ prefs }: { prefs: Prefs }) {
  const device = useStore((x) => x.device);
  const caps = device?.capabilities;
  const set = (patch: Partial<Prefs>) => savePrefs({ ...prefs, ...patch });
  // The battery saver is the connected mouse's own.
  const mouse = useMouse();
  const saver = mouse?.prefs;
  const setSaver = (patch: Partial<MousePrefs>) => mouse && saveMousePrefs(mouse.key, { ...mouse.prefs, ...patch });
  // Only the rates below what this connection tops out at: a 4K receiver can drop to 2,000 Hz,
  // a 2K one to 1,000.
  const top = topRate(caps);
  const rates = top !== undefined ? (caps?.polling?.rates ?? []).filter((hz) => hz < top) : [];
  const lowered = saver?.batterySaverRestore ?? null;
  // A connection that tops out at 1,000 Hz has nothing worth lowering: the saver only exists for
  // the high rates that cost the battery. It needs a mouse with a battery and a polling rate.
  const showSaver = !!saver && !!caps?.polling && caps.battery && !(!caps.wired && (top ?? 0) <= 1000 && !lowered);
  // One line: the level and rate are in the controls beside it.
  const saverText = caps?.wired
    ? t("Not needed while wired.")
    : lowered
      ? t("Lowered from {{rate}} until charged.", { rate: hzText(lowered) })
      : saver?.batterySaver
        ? t("Lowers the polling rate on low battery, until charged.")
        : t("Lowers the polling rate when the battery is low.");
  const s = device?.settings;
  // The sleep timer: a Z1 PRO MAX's (0xBB, measured 2026-10-17) and the R1 line's (ATK's 0xAD); others offer none.
  const sleepTimes = caps?.sleepTimesS ?? [];
  // A value set elsewhere stays one of the choices, so it shows as chosen.
  const sleepS = s?.sleepS;
  const sleepChoices = sleepS !== undefined && !sleepTimes.includes(sleepS) ? [...sleepTimes, sleepS].sort((a, b) => a - b) : sleepTimes;
  return (
    <>
      {showSaver && (
        <SettingCard icon={<BatterySaver20Regular />} title={t("Battery saver")} description={saverText}>
          {saver?.batterySaver && !caps?.wired && (
            <div className="tw flex items-center gap-2">
              <NumberStepper label={t("Battery level to start saving at")} unit="%" min={5} max={50} step={5} value={saver.batterySaverBelow} onChange={(v) => setSaver({ batterySaverBelow: v })} />
              <Dropdown
                aria-label={t("Polling rate while saving")}
                style={{ minWidth: 128 }}
                value={hzText(saver.batterySaverHz)}
                selectedOptions={[String(saver.batterySaverHz)]}
                onOptionSelect={(_, d) => d.optionValue && setSaver({ batterySaverHz: Number(d.optionValue) })}
              >
                {rates.map((hz) => (
                  <Option key={hz} value={String(hz)}>
                    {hzText(hz)}
                  </Option>
                ))}
              </Dropdown>
            </div>
          )}
          <Switch checked={!!saver?.batterySaver} disabled={caps?.wired} onChange={(_, d) => setSaver({ batterySaver: d.checked })} aria-label={t("Battery saver")} />
        </SettingCard>
      )}
      {s && sleepS !== undefined && sleepTimes.length > 0 && !caps?.wired && (
        <SettingCard
          icon={<WeatherMoon20Regular />}
          title={t("Sleep after")}
          description={device?.online ? t("Idle time before the mouse sleeps. Longer uses more battery.") : t("Wake the mouse to change this.")}
        >
          {sleepChoices.length > 5 ? (
            // An R1 has eight, too many for one row.
            <Dropdown
              aria-label={t("Sleep after")}
              style={{ minWidth: 128 }}
              disabled={!device?.online}
              value={shortTime(sleepS)}
              selectedOptions={[String(sleepS)]}
              onOptionSelect={(_, d) => d.optionValue && void commit({ ...s, sleepS: Number(d.optionValue) })}
            >
              {sleepChoices.map((v) => (
                <Option key={v} value={String(v)}>
                  {shortTime(v)}
                </Option>
              ))}
            </Dropdown>
          ) : (
            <Choice
              label={t("Sleep after")}
              value={sleepS}
              disabled={!device?.online}
              options={sleepChoices.map((v) => ({ value: v, label: shortTime(v) }))}
              onChange={(v) => void commit({ ...s, sleepS: v })}
            />
          )}
        </SettingCard>
      )}
      <SettingCard icon={<BatteryWarning20Regular />} title={t("Low battery alert")} description={t("Notifies you once when the battery drops to this level.")}>
        {prefs.batteryAlerts && <NumberStepper label={t("Low battery level")} unit="%" min={5} max={50} step={5} value={prefs.lowBatteryPercent} onChange={(v) => set({ lowBatteryPercent: v })} />}
        <Switch checked={prefs.batteryAlerts} onChange={(_, d) => set({ batteryAlerts: d.checked })} aria-label={t("Low battery alert")} />
      </SettingCard>
    </>
  );
}
