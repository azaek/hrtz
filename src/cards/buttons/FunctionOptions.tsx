import type { ReactNode } from "react";
import { Body1, Caption1, Dropdown, Option } from "@fluentui/react-components";
import type { ButtonAction } from "@/api";
import { Choice } from "@/components";
import { buttonNames, MEDIA } from "@/hid";
import { t } from "@/i18n";
import { NumberStepper } from "@/kit";
import { functionOf, type Catalog } from "./functions";
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
  /** What this mouse's buttons can do. */
  catalog: Catalog;
  maxDpi: number;
  disabled?: boolean;
  onChange: (a: ButtonAction) => void;
  /** The Keys tile was picked and is waiting for a binding. */
  waitingForKeys?: boolean;
  onCancelKeys?: () => void;
}) {
  const a = props.action;
  const off = props.disabled;
  const set = props.onChange;
  const cat = props.catalog;
  switch (functionOf(a)) {
    case "mouse": {
      const names = buttonNames();
      return (
        <Choice
          label={t("Mouse button")}
          disabled={off}
          value={a.kind === "mouse" ? a.button : "left"}
          options={[
            { value: "left", label: names.left },
            { value: "right", label: names.right },
            { value: "middle", label: names.middle },
            { value: "back", label: names.back },
            { value: "forward", label: names.forward },
          ]}
          onChange={(button) => set({ kind: "mouse", button })}
        />
      );
    }
    case "dpi":
      return (
        <>
          <Choice
            label={t("DPI action")}
            disabled={off}
            value={a.kind === "dpiLock" ? "hold" : a.kind === "dpi" ? a.action : "cycle"}
            options={(
              [
                { value: "cycle", label: t("Cycle stages") },
                { value: "up", label: t("Up", { context: "direction" }) },
                { value: "down", label: t("Down", { context: "direction" }) },
                { value: "default", label: t("Default stage") },
                { value: "hold", label: t("Hold for") },
              ] as const
            ).filter((o) => cat.dpi.includes(o.value))}
            onChange={(v) => set(v === "hold" ? { kind: "dpiLock", dpi: 400 } : { kind: "dpi", action: v })}
          />
          {a.kind === "dpiLock" && (
            <Field label={t("DPI while held")} hint={t("Returns to the previous stage on release.")}>
              <NumberStepper
                className="w-36"
                label={t("DPI while held")}
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
          label={t("Scroll direction")}
          disabled={off}
          value={a.kind === "scroll" ? a.direction : cat.scroll[0]}
          options={(
            [
              { value: "up", label: t("Up", { context: "direction" }) },
              { value: "down", label: t("Down", { context: "direction" }) },
              { value: "left", label: t("Left", { context: "direction" }) },
              { value: "right", label: t("Right", { context: "direction" }) },
            ] as const
          ).filter((o) => cat.scroll.includes(o.value))}
          onChange={(direction) => set({ kind: "scroll", direction })}
        />
      );
    case "media":
      return (
        <Dropdown
          disabled={off}
          aria-label={t("Media key")}
          style={{ minWidth: 240 }}
          value={a.kind === "media" ? (MEDIA.find((m) => m.usage === a.usage)?.name ?? t("Media key {{code}}", { code: `0x${a.usage.toString(16)}` })) : ""}
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
        <Field label={t("Keys")} hint={cat.oneKey ? t("One key and up to three modifiers.") : t("Up to three keys, including modifiers.")}>
          <KeyRecorder
            value={a}
            autoFocus={props.waitingForKeys}
            disabled={off}
            oneKey={cat.oneKey}
            onCommit={(v) => set({ kind: "keys", ...v })}
            onCancel={props.onCancelKeys}
          />
        </Field>
      ) : null;
    case "fire":
      return a.kind === "fire" ? (
        <>
          <Field label={t("Time between clicks")}>
            <NumberStepper label={t("Time between clicks")} unit="ms" min={10} max={255} value={a.intervalMs} disabled={off} onChange={(intervalMs) => set({ ...a, intervalMs })} />
          </Field>
          <Field label={t("Clicks each press")}>
            <Choice
              label={t("Clicks each press")}
              disabled={off}
              value={a.times}
              options={[
                { value: 0, label: t("While held") },
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
      return <Caption1 style={muted}>{t("Each press switches to the next polling rate.")}</Caption1>;
    case "profile":
      return (
        <Choice
          label={t("Profile action")}
          disabled={off}
          value={a.kind === "profile" ? a.action : "next"}
          options={[
            { value: "next", label: t("Next") },
            { value: "previous", label: t("Previous") },
            { value: "cycle", label: t("Cycle") },
          ]}
          onChange={(action) => set({ kind: "profile", action })}
        />
      );
    case "disabled":
      return <Caption1 style={muted}>{t("This button is disabled.")}</Caption1>;
    case null:
      return <Caption1 style={muted}>{t("Set in {{app}} and not editable here yet. Choosing a function replaces it.", { app: t(cat.maker) })}</Caption1>;
  }
}
