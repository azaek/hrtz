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
import { topRate, type Lod, type Settings } from "../api";
import { formatDuration } from "../format";
import { DpiStagesCard } from "../cards/dpi-stages/DpiStagesCard";
import { hzText } from "../rates";
import { NumberStepper } from "../kit";

const LOD_LABEL: Record<Lod, string> = { "0.7mm": "0.7 mm", "1mm": "1 mm", "2mm": "2 mm" };

/** Each card shows only if the mouse has that setting; a group with none is left out. */
export function SensorPage() {
  const device = useStore((x) => x.device!);
  const busy = useStore((x) => x.busy);
  const s = device.settings!;
  const caps = device.capabilities!;
  const update = (patch: Partial<Settings>) => commit({ ...s, ...patch });
  const disabled = !device.online;
  const top = topRate(caps);
  const mode = caps.sensorMode;
  const lockedCorded =
    !!mode && ((mode.cordedWhenWired && caps.wired) || (mode.cordedFromHz !== undefined && (s.pollingHz ?? 0) >= mode.cordedFromHz));

  const polling = caps.polling && s.pollingHz !== undefined && (
    <SettingCard
      stacked
      icon={<TopSpeed20Regular />}
      title="Polling rate"
      description={
        top !== undefined && s.pollingHz > top
          ? `Set to ${hzText(s.pollingHz)} for the receiver. Wired, it runs at up to ${hzText(top)}.`
          : caps.wired && top !== undefined
            ? `How often the mouse reports to your PC. Up to ${hzText(top)} when wired.`
            : "How often the mouse reports to your PC."
      }
      below={
        <div style={{ marginTop: 12 }}>
          <Choice
            label="Polling rate"
            value={s.pollingHz}
            options={caps.polling.rates.map((hz) => ({
              value: hz,
              label: hz >= 1000 ? `${hz / 1000}K Hz` : `${hz} Hz`,
            }))}
            onChange={(pollingHz) => update({ pollingHz })}
          />
        </div>
      }
    />
  );
  const liftOff = caps.lod && s.lod && (
    <SettingCard icon={<Ruler20Regular />} title="Lift-off distance" description="How high the mouse can lift before the sensor stops tracking.">
      <Choice label="Lift-off distance" value={s.lod} options={caps.lod.map((value) => ({ value, label: LOD_LABEL[value] }))} onChange={(lod) => update({ lod })} />
    </SettingCard>
  );
  const sensorMode = mode && s.sensorMode && (
    <SettingCard
      icon={<Rocket20Regular />}
      title="Sensor mode"
      description={
        mode.cordedWhenWired && caps.wired
          ? "Always Corded while wired."
          : lockedCorded && mode.cordedFromHz !== undefined
            ? `Always Corded at ${hzText(mode.cordedFromHz)} and above.`
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
          ].filter((o) => mode.modes.includes(o.value))}
          onChange={(sensorMode) => update({ sensorMode })}
        />
      )}
    </SettingCard>
  );
  const motionSync = caps.motionSync && s.motionSync !== undefined && (
    <SettingCard icon={<ArrowSync20Regular />} title="Motion sync" description="Aligns sensor reads with USB reports. Smoother, with slightly more latency.">
      <Switch checked={s.motionSync} onChange={(_, d) => update({ motionSync: d.checked })} aria-label="Motion sync" />
    </SettingCard>
  );
  const angleSnapping = caps.angleSnapping && s.angleSnapping !== undefined && (
    <SettingCard icon={<Line20Regular />} title="Angle snapping" description="Straightens near-horizontal and near-vertical movement. Usually off for gaming.">
      <Switch checked={s.angleSnapping} onChange={(_, d) => update({ angleSnapping: d.checked })} aria-label="Angle snapping" />
    </SettingCard>
  );
  const ripple = caps.rippleControl && s.rippleControl !== undefined && (
    <SettingCard icon={<Pulse20Regular />} title="Ripple control" description="Reduces jitter at high DPI, with slightly less responsiveness.">
      <Switch checked={s.rippleControl} onChange={(_, d) => update({ rippleControl: d.checked })} aria-label="Ripple control" />
    </SettingCard>
  );
  const debounce = caps.debounceMs && s.debounceMs !== undefined && (
    <SettingCard
      icon={<Timer20Regular />}
      title="Click debounce"
      description={s.debounceMs < 4 ? "Below 4 ms, some switches register one press as two." : "Ignores switch bounce for this long after each press."}
    >
      <NumberStepper
        label="Click debounce"
        unit="ms"
        min={caps.debounceMs.min}
        max={caps.debounceMs.max}
        value={s.debounceMs}
        onChange={(debounceMs) => update({ debounceMs })}
      />
    </SettingCard>
  );
  const peak = caps.peakTimeoutsS && s.peakTimeoutEnabled !== undefined && s.peakTimeoutS !== undefined && (
    <SettingCard icon={<Flash20Regular />} title="Peak performance timeout" description="How long the sensor stays at full power after you stop moving.">
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
      <Switch checked={s.peakTimeoutEnabled} onChange={(_, d) => update({ peakTimeoutEnabled: d.checked })} aria-label="Peak performance timeout" />
    </SettingCard>
  );
  const dpi = caps.dpi && s.stages && <DpiStagesCard s={s} caps={caps} />;
  const tracking = [dpi, polling, liftOff, sensorMode, motionSync, angleSnapping, ripple].some(Boolean);
  const clicks = [debounce, peak].some(Boolean);

  return (
    <fieldset disabled={disabled || busy} style={{ border: 0, margin: 0, padding: 0, minWidth: 0, opacity: disabled ? 0.6 : 1 }}>
      {tracking && (
        <Group title="Tracking">
          {dpi}
          {polling}
          {liftOff}
          {sensorMode}
          {motionSync}
          {angleSnapping}
          {ripple}
        </Group>
      )}
      {clicks && (
        <Group title="Clicks and power">
          {debounce}
          {peak}
        </Group>
      )}
    </fieldset>
  );
}
