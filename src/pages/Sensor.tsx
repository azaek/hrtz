import { Body1, Dropdown, Option, Switch } from "@fluentui/react-components";
import {
  ArrowSync20Regular,
  Flash20Regular,
  Line20Regular,
  Pulse20Regular,
  Ruler20Regular,
  Rocket20Regular,
  Timer20Regular,
  TopSpeed20Regular,
} from "@fluentui/react-icons";
import { Choice, Group, SettingCard } from "../components";
import { commit, useStore } from "../store";
import type { Settings } from "../api";
import { formatDuration } from "../format";
import { DpiStagesCard } from "../cards/dpi-stages/DpiStagesCard";
import { hzText } from "../rates";
import { NumberStepper } from "../kit";

export function SensorPage() {
  const device = useStore((x) => x.device!);
  const busy = useStore((x) => x.busy);
  const s = device.settings!;
  const caps = device.capabilities!;
  const update = (patch: Partial<Settings>) => commit({ ...s, ...patch });
  const lockedCorded = s.pollingHz >= 2000 || caps.wired;
  const aboveLink = s.pollingHz > caps.maxPollingHz;
  const disabled = !device.online;

  return (
    <fieldset disabled={disabled || busy} style={{ border: 0, margin: 0, padding: 0, minWidth: 0, opacity: disabled ? 0.6 : 1 }}>

      <Group title="Tracking">
        <DpiStagesCard s={s} caps={caps} />
        <SettingCard
          stacked
          icon={<TopSpeed20Regular />}
          title="Polling rate"
          description={
            aboveLink
              ? `Set to ${hzText(s.pollingHz)} for the receiver. Wired, it runs at up to ${hzText(caps.maxPollingHz)}.`
              : caps.wired
                ? `How often the mouse reports to your PC. Up to ${hzText(caps.maxPollingHz)} when wired.`
                : "How often the mouse reports to your PC."
          }
          below={
            <div style={{ marginTop: 12 }}>
              <Choice
                label="Polling rate"
                value={s.pollingHz}
                options={caps.pollingHz.map((hz) => ({
                  value: hz,
                  label: hz >= 1000 ? `${hz / 1000}K Hz` : `${hz} Hz`,
                }))}
                onChange={(pollingHz) => update({ pollingHz })}
              />
            </div>
          }
        />
        <SettingCard icon={<Ruler20Regular />} title="Lift-off distance" description="How high the mouse can lift before the sensor stops tracking.">
          <Choice
            label="Lift-off distance"
            value={s.lod}
            options={[
              ...(caps.lowLod ? [{ value: "0.7mm" as const, label: "0.7 mm" }] : []),
              { value: "1mm" as const, label: "1 mm" },
              { value: "2mm" as const, label: "2 mm" },
            ]}
            onChange={(lod) => update({ lod })}
          />
        </SettingCard>
        <SettingCard
          icon={<Rocket20Regular />}
          title="Sensor mode"
          description={
            caps.wired
              ? "Always Corded while wired."
              : lockedCorded
                ? "Always Corded at 2,000 Hz and above."
              : "Low power saves battery. High performance tracks faster and uses more."
          }
        >
          {lockedCorded ? (
            <Body1>Corded</Body1>
          ) : (
            <Choice
              label="Sensor mode"
              value={s.sensorMode === "corded" ? "hp" : s.sensorMode}
              options={[
                { value: "lp" as const, label: "Low power" },
                { value: "hp" as const, label: "High performance" },
              ]}
              onChange={(sensorMode) => update({ sensorMode })}
            />
          )}
        </SettingCard>
        <SettingCard icon={<ArrowSync20Regular />} title="Motion sync" description="Aligns sensor reads with USB reports. Smoother, with slightly more latency.">
          <Switch checked={s.motionSync} onChange={(_, d) => update({ motionSync: d.checked })} aria-label="Motion sync" />
        </SettingCard>
        <SettingCard icon={<Line20Regular />} title="Angle snapping" description="Straightens near-horizontal and near-vertical movement. Usually off for gaming.">
          <Switch checked={s.angleSnapping} onChange={(_, d) => update({ angleSnapping: d.checked })} aria-label="Angle snapping" />
        </SettingCard>
        <SettingCard icon={<Pulse20Regular />} title="Ripple control" description="Reduces jitter at high DPI, with slightly less responsiveness.">
          <Switch checked={s.rippleControl} onChange={(_, d) => update({ rippleControl: d.checked })} aria-label="Ripple control" />
        </SettingCard>
      </Group>

      <Group title="Clicks and power">
        <SettingCard
          icon={<Timer20Regular />}
          title="Click debounce"
          description={
            s.debounceMs < 4
              ? "Below 4 ms, some switches register one press as two."
              : "Ignores switch bounce for this long after each press."
          }
        >
          <NumberStepper label="Click debounce" unit="ms" min={0} max={20} value={s.debounceMs} onChange={(debounceMs) => update({ debounceMs })} />
        </SettingCard>
        <SettingCard
          icon={<Flash20Regular />}
          title="Peak performance timeout"
          description="How long the sensor stays at full power after you stop moving."
        >
          {s.peakTimeoutEnabled && (
            <Dropdown
              aria-label="Peak performance timeout"
              value={formatDuration(s.peakTimeoutS)}
              selectedOptions={[String(s.peakTimeoutS)]}
              onOptionSelect={(_, d) => update({ peakTimeoutS: Number(d.optionValue) })}
              style={{ minWidth: 120 }}
            >
              {caps.peakTimeoutsS.map((t) => (
                <Option key={t} value={String(t)}>
                  {formatDuration(t)}
                </Option>
              ))}
            </Dropdown>
          )}
          <Switch
            checked={s.peakTimeoutEnabled}
            onChange={(_, d) => update({ peakTimeoutEnabled: d.checked })}
            aria-label="Peak performance timeout"
          />
        </SettingCard>
      </Group>
    </fieldset>
  );
}
