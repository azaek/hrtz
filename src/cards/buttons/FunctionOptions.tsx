import type { ReactNode } from "react";
import { Body1, Caption1, Dropdown, Option } from "@fluentui/react-components";
import type { ButtonAction } from "@/api";
import { Choice } from "@/components";
import { MEDIA } from "@/hid";
import { NumberStepper } from "@/kit";
import { functionOf } from "./functions";
import { KeyRecorder } from "./KeyRecorder";

const muted = { color: "var(--colorNeutralForeground3)" };

/** A label over its control, with an optional line under it. */
function Field({ label, hint, children }: { label?: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-1.5">
      {label && <Body1>{label}</Body1>}
      {children}
      {hint && <Caption1 style={muted}>{hint}</Caption1>}
    </div>
  );
}

/**
 * The options under the function tiles: which mouse button, DPI action or scroll direction, which
 * media key, and the settings rapid fire, hold-for-DPI and key combinations take. Each change goes
 * straight to `onChange` (the steppers settle first).
 */
export function FunctionOptions(props: {
  action: ButtonAction;
  maxDpi: number;
  disabled?: boolean;
  onChange: (a: ButtonAction) => void;
  /** Focus the key recorder: the Keys tile was just picked. */
  recordNow?: boolean;
  /** Left the key recorder without pressing anything. */
  onLeaveKeys?: () => void;
}) {
  const a = props.action;
  const off = props.disabled;
  const set = props.onChange;
  switch (functionOf(a)) {
    case "mouse":
      return (
        <Choice
          label="Mouse button"
          disabled={off}
          value={a.kind === "mouse" ? a.button : "left"}
          options={[
            { value: "left", label: "Left" },
            { value: "right", label: "Right" },
            { value: "middle", label: "Middle" },
            { value: "back", label: "Back" },
            { value: "forward", label: "Forward" },
          ]}
          onChange={(button) => set({ kind: "mouse", button })}
        />
      );
    case "dpi":
      return (
        <>
          <Choice
            label="DPI action"
            disabled={off}
            value={a.kind === "dpiLock" ? "hold" : a.kind === "dpi" ? a.action : "cycle"}
            options={[
              { value: "cycle", label: "Cycle stages" },
              { value: "up", label: "Up" },
              { value: "down", label: "Down" },
              { value: "hold", label: "Hold for" },
            ]}
            onChange={(v) => set(v === "hold" ? { kind: "dpiLock", dpi: 400 } : { kind: "dpi", action: v })}
          />
          {a.kind === "dpiLock" && (
            <Field label="DPI while held" hint="Returns to the previous stage on release.">
              <NumberStepper
                className="w-36"
                label="DPI while held"
                unit="DPI"
                min={50}
                max={Math.min(12800, props.maxDpi)}
                step={50}
                value={a.dpi}
                disabled={off}
                onChange={(dpi) => set({ ...a, dpi })}
              />
            </Field>
          )}
        </>
      );
    case "scroll":
      return (
        <Choice
          label="Scroll direction"
          disabled={off}
          value={a.kind === "scroll" ? a.direction : "up"}
          options={[
            { value: "up", label: "Up" },
            { value: "down", label: "Down" },
            { value: "left", label: "Left" },
            { value: "right", label: "Right" },
          ]}
          onChange={(direction) => set({ kind: "scroll", direction })}
        />
      );
    case "media":
      return (
        <Dropdown
          disabled={off}
          aria-label="Media key"
          style={{ minWidth: 240 }}
          value={a.kind === "media" ? (MEDIA.find((m) => m.usage === a.usage)?.name ?? `Media key 0x${a.usage.toString(16)}`) : ""}
          selectedOptions={a.kind === "media" ? [String(a.usage)] : []}
          onOptionSelect={(_, d) => set({ kind: "media", usage: Number(d.optionValue) })}
        >
          {MEDIA.map((m) => (
            <Option key={m.usage} value={String(m.usage)}>
              {m.name}
            </Option>
          ))}
        </Dropdown>
      );
    case "keys":
      return a.kind === "keys" ? (
        <Field label="Keys" hint="Up to three keys, including modifiers. Saves on release.">
          <KeyRecorder label="Keys" value={a} autoFocus={props.recordNow} disabled={off} onCommit={(v) => set({ kind: "keys", ...v })} onLeaveEmpty={props.onLeaveKeys} />
        </Field>
      ) : null;
    case "fire":
      return a.kind === "fire" ? (
        <>
          <Field label="Time between clicks">
            <NumberStepper label="Time between clicks" unit="ms" min={10} max={255} value={a.intervalMs} disabled={off} onChange={(intervalMs) => set({ ...a, intervalMs })} />
          </Field>
          <Field label="Clicks each press">
            <Choice
              label="Clicks each press"
              disabled={off}
              value={a.times}
              options={[
                { value: 0, label: "While held" },
                { value: 1, label: "1" },
                { value: 2, label: "2" },
                { value: 3, label: "3" },
              ]}
              onChange={(times) => set({ ...a, times })}
            />
          </Field>
        </>
      ) : null;
    case "pollingSwitch":
      return <Caption1 style={muted}>Each press switches to the next polling rate.</Caption1>;
    case "disabled":
      return <Caption1 style={muted}>This button is disabled.</Caption1>;
    case null:
      return <Caption1 style={muted}>Set in Zaopin's app and not editable here yet. Choosing a function replaces it.</Caption1>;
  }
}
