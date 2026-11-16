import { Body1, Dropdown, Link, Option, Switch } from "@fluentui/react-components";
import {
  ArrowRotateClockwise20Regular,
  ArrowSync20Regular,
  BrightnessHigh20Regular,
  Flash20Regular,
  HeartPulse20Regular,
  Lightbulb20Regular,
  Line20Regular,
  Pulse20Regular,
  Ruler20Regular,
  Rocket20Regular,
  Timer20Regular,
  TopSpeed20Regular,
} from "@fluentui/react-icons";
import { Choice, Group, SettingCard } from "../components";
import { commit, useStore } from "../store";
import { lodName, sensorModeName, topRate, type Lod, type SensorMode, type Settings } from "../api";
import { formatDuration } from "../format";
import { DpiStagesCard } from "../cards/dpi-stages/DpiStagesCard";
import { GamesCard } from "../cards/games/GamesCard";
import { NewThings } from "../cards/discover/NewThings";
import { openTest } from "../cards/lab/tests";
import { isZaopin } from "../models";
import { SignalRgbCard, useSetBySignalRgb } from "../cards/signalrgb/SignalRgbCard";
import { hzText } from "../rates";
import { NumberStepper } from "../kit";

/** Names for a light that offers three steps of brightness or speed (an ATK's), lowest first. */
const THREE_BRIGHTNESS = ["Low", "Medium", "High"];
const THREE_SPEEDS = ["Slow", "Medium", "Fast"];

/** Each card shows only if the mouse has that setting; a group with none is left out. */
export function SensorPage() {
  const device = useStore((x) => x.device!);
  const busy = useStore((x) => x.busy);
  const s = device.settings!;
  const caps = device.capabilities!;
  const update = (patch: Partial<Settings>) => commit({ ...s, ...patch });
  const bySignalRgb = useSetBySignalRgb(device.key);
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
        <>
          {top !== undefined && s.pollingHz > top
            ? `Set to ${hzText(s.pollingHz)} for the receiver. Wired, it runs at up to ${hzText(top)}.`
            : caps.wired && top !== undefined
              ? `How often the mouse reports to your PC. Up to ${hzText(top)} when wired.`
              : "How often the mouse reports to your PC."}
        </>
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
      {caps.lod.length > 3 ? (
        // A sensor with a scale of them (a PAW3950 Ultra's eleven): too many to lay side by side.
        <Dropdown aria-label="Lift-off distance" value={lodName(s.lod)} selectedOptions={[s.lod]} onOptionSelect={(_, d) => d.optionValue && update({ lod: d.optionValue as Lod })} style={{ minWidth: 120 }}>
          {caps.lod.map((lod) => (
            <Option key={lod} value={lod}>
              {lodName(lod)}
            </Option>
          ))}
        </Dropdown>
      ) : (
        <Choice label="Lift-off distance" value={s.lod} options={caps.lod.map((value) => ({ value, label: lodName(value) }))} onChange={(lod) => update({ lod })} />
      )}
    </SettingCard>
  );
  // Where the top mode is chosen (an ATK's), it's one of the choices; a Zaopin enters its own by
  // itself, so it shows as High performance until it's locked in.
  const byChoice = !!mode?.cordedByChoice;
  const modes = (mode?.modes ?? []).filter((m) => byChoice || m !== "corded");
  const sensorMode = mode && s.sensorMode && (
    <SettingCard
      icon={<Rocket20Regular />}
      title="Sensor mode"
      description={
        mode.cordedWhenWired && caps.wired
          ? "Always Corded while wired."
          : lockedCorded && mode.cordedFromHz !== undefined
            ? `Always Corded at ${hzText(mode.cordedFromHz)} and above.`
            : modes.length > 2
              ? "Higher modes track faster and use more battery."
              : "Low power saves battery. High performance tracks faster and uses more."
      }
    >
      {lockedCorded ? (
        <Body1>Corded</Body1>
      ) : modes.length > 2 ? (
        // Three names are too wide to lay side by side.
        <Dropdown
          aria-label="Sensor mode"
          value={sensorModeName(s.sensorMode, byChoice)}
          selectedOptions={[s.sensorMode]}
          onOptionSelect={(_, d) => d.optionValue && update({ sensorMode: d.optionValue as SensorMode })}
          style={{ minWidth: 120 }}
        >
          {modes.map((m) => (
            <Option key={m} value={m}>
              {sensorModeName(m, byChoice)}
            </Option>
          ))}
        </Dropdown>
      ) : (
        <Choice
          label="Sensor mode"
          value={!byChoice && s.sensorMode === "corded" ? "hp" : s.sensorMode}
          options={modes.map((value) => ({ value, label: sensorModeName(value, byChoice) }))}
          onChange={(sensorMode) => update({ sensorMode })}
        />
      )}
    </SettingCard>
  );
  const angle = s.sensorAngle;
  const sensorAngle = caps.sensorAngleDeg !== undefined && angle && (
    <SettingCard icon={<ArrowRotateClockwise20Regular />} title="Sensor angle" description="Turns the sensor's direction to match how you hold the mouse.">
      {/* The Lab measures the angle a grip needs. */}
      <Link as="button" onClick={() => openTest("angle")}>
        Find yours
      </Link>
      {angle.on && (
        <NumberStepper
          label="Sensor angle"
          unit="°"
          min={-caps.sensorAngleDeg}
          max={caps.sensorAngleDeg}
          value={angle.degrees}
          onChange={(degrees) => update({ sensorAngle: { on: true, degrees } })}
        />
      )}
      <Switch checked={angle.on} onChange={(_, d) => update({ sensorAngle: { ...angle, on: d.checked } })} aria-label="Sensor angle" />
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
      {caps.debounceChoicesMs ? (
        <Dropdown
          aria-label="Click debounce"
          value={`${s.debounceMs} ms`}
          selectedOptions={[String(s.debounceMs)]}
          onOptionSelect={(_, d) => update({ debounceMs: Number(d.optionValue) })}
          style={{ minWidth: 120 }}
        >
          {caps.debounceChoicesMs.map((ms) => (
            <Option key={ms} value={String(ms)}>
              {`${ms} ms`}
            </Option>
          ))}
        </Dropdown>
      ) : (
        <NumberStepper
          label="Click debounce"
          unit="ms"
          min={caps.debounceMs.min}
          max={caps.debounceMs.max}
          value={s.debounceMs}
          onChange={(debounceMs) => update({ debounceMs })}
        />
      )}
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
  const dpi = caps.dpi && s.stages && <DpiStagesCard s={s} caps={caps} coloursLocked={bySignalRgb} />;
  const light = caps.dpi?.light ? s.dpiLight : undefined;
  const lightEffect = light && (
    <SettingCard
      icon={<Lightbulb20Regular />}
      title="DPI light"
      description={
        bySignalRgb ? "Set by SignalRGB." : light.effect === "off" ? "Off, so the stage colours don't show." : "Shows the colour of the DPI stage in use."
      }
    >
      <Choice
        label="DPI light"
        value={light.effect}
        disabled={bySignalRgb}
        options={[
          { value: "off" as const, label: "Off" },
          { value: "steady" as const, label: "Steady" },
          { value: "breathing" as const, label: "Breathing" },
        ]}
        onChange={(effect) => update({ dpiLight: { ...light, effect } })}
      />
    </SettingCard>
  );
  const steps = caps.dpi?.brightnessSteps;
  const speeds = caps.dpi?.speeds;
  const brightness = light?.effect === "steady" && (
    <SettingCard icon={<BrightnessHigh20Regular />} title="Brightness" description="How bright the DPI light is.">
      {steps ? (
        <Choice
          label="Brightness"
          value={light.brightness}
          options={steps.map((value, i) => ({ value, label: (steps.length === 3 ? THREE_BRIGHTNESS[i] : undefined) ?? String(value) }))}
          onChange={(brightness) => update({ dpiLight: { ...light, brightness } })}
        />
      ) : (
        <NumberStepper label="Brightness" min={1} max={10} value={light.brightness} onChange={(brightness) => update({ dpiLight: { ...light, brightness } })} />
      )}
    </SettingCard>
  );
  const speed = light?.effect === "breathing" && (
    <SettingCard icon={<HeartPulse20Regular />} title="Breathing speed" description="How fast the DPI light breathes.">
      {speeds ? (
        <Choice
          label="Breathing speed"
          value={light.speed}
          disabled={bySignalRgb}
          options={speeds.map((value, i) => ({ value, label: (speeds.length === 3 ? THREE_SPEEDS[i] : undefined) ?? String(value) }))}
          onChange={(speed) => update({ dpiLight: { ...light, speed } })}
        />
      ) : (
        <NumberStepper
          label="Breathing speed"
          min={1}
          max={5}
          value={light.speed}
          disabled={bySignalRgb}
          onChange={(speed) => update({ dpiLight: { ...light, speed } })}
        />
      )}
    </SettingCard>
  );
  const games = <GamesCard />;
  const tracking = [dpi, polling, liftOff, sensorMode, sensorAngle, motionSync, angleSnapping, ripple].some(Boolean);
  const clicks = [debounce, peak].some(Boolean);

  return (
    <fieldset disabled={disabled || busy} style={{ border: 0, margin: 0, padding: 0, minWidth: 0, opacity: disabled ? 0.6 : 1 }}>
      {/* The features lab's pick (DECISIONS.md, "Three things to try"). */}
      <NewThings />
      {tracking && (
        <Group title="Tracking">
          {dpi}
          <div id="games-card">{games}</div>
          {polling}
          {liftOff}
          {sensorMode}
          {sensorAngle}
          {motionSync}
          {angleSnapping}
          {ripple}
        </Group>
      )}
      {lightEffect && (
        <Group title="Light">
          {lightEffect}
          {brightness}
          {speed}
          {isZaopin(device) && <SignalRgbCard mouseKey={device.key} />}
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
