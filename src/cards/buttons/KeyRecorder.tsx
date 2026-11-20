import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Button, Input } from "@fluentui/react-components";
import { Keyboard16Regular } from "@fluentui/react-icons";
import { comboLabel, KEY_USAGE, MODIFIER_BIT } from "@/hid";
import { t } from "@/i18n";

type Combo = { modifiers: number; keys: number[] };

function physicalCode({ code, key, location }: KeyboardEvent): string {
  if (code && code !== "Unidentified") return code;
  if (key === "AltGraph") return "AltRight";
  if (key === "Alt" || key === "Control" || key === "Shift" || key === "Meta") {
    return key + (location === 2 ? "Right" : "Left");
  }
  if (/^[a-z]$/i.test(key)) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return (location === 3 ? "Numpad" : "Digit") + key;
  return key === " " ? "Space" : key;
}

export function KeyRecorder({ value, autoFocus, disabled, oneKey, onCommit, onCancel }: {
  value: Combo;
  autoFocus?: boolean;
  disabled?: boolean;
  oneKey: boolean;
  onCommit: (value: Combo) => void;
  onCancel?: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState<Combo | null>(null);
  const recording = draft !== null && !disabled;
  const shown = recording ? draft : value;

  useEffect(() => {
    if (disabled) setDraft(null);
  }, [disabled]);

  const add = (modifier: number, usage: number) => {
    if (!modifier && !usage) return;
    setDraft((current) => {
      if (!current) return current;
      const next = {
        modifiers: current.modifiers | modifier,
        keys: usage && !current.keys.includes(usage) ? [...current.keys, usage] : current.keys,
      };
      const modifiers = next.modifiers.toString(2).replace(/0/g, "").length;
      const fits = oneKey ? modifiers <= 3 && next.keys.length <= 1 : modifiers + next.keys.length <= 3;
      return fits ? next : current;
    });
  };

  const toggle = () => {
    if (!recording) {
      setDraft({ modifiers: 0, keys: [] });
      input.current?.focus();
      return;
    }
    setDraft(null);
    if (draft.modifiers || draft.keys.length) onCommit(draft);
    else onCancel?.();
  };

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <Button autoFocus={autoFocus} disabled={disabled} onClick={toggle}>
          {recording ? t("Stop recording") : t("Start recording")}
        </Button>
        <Input
          ref={input}
          readOnly
          disabled={disabled}
          tabIndex={recording ? 0 : -1}
          aria-label={recording ? t("Keys being recorded") : t("Assigned keys")}
          contentBefore={<Keyboard16Regular />}
          value={shown.modifiers || shown.keys.length ? comboLabel(shown.modifiers, shown.keys) : ""}
          placeholder={recording ? t("Tap keys one at a time") : t("No keys set")}
          style={{ width: 240 }}
          onKeyDown={(e) => {
            if (!recording || (e.key === "Tab" && e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey)) return;
            e.preventDefault();
            e.stopPropagation();
            if (!e.repeat) add(0, KEY_USAGE[physicalCode(e)] ?? 0);
          }}
          onKeyUp={(e) => {
            if (!recording) return;
            e.preventDefault();
            e.stopPropagation();
            // Releasing Shift outside this field after Shift+Tab leaves it out of the binding.
            const modifier = MODIFIER_BIT[physicalCode(e)];
            if (modifier) add(modifier, 0);
          }}
        />
        {(recording || autoFocus) && (
          <Button appearance="subtle" disabled={disabled} onClick={() => { setDraft(null); onCancel?.(); }}>
            {t("Cancel")}
          </Button>
        )}
      </div>
    </>
  );
}
