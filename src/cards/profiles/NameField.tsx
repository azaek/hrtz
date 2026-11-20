import { Input } from "@fluentui/react-components";
import { t } from "@/i18n";

/**
 * A profile's name as a field, opened by Rename: the name starts selected, Enter or leaving the
 * field saves, Escape cancels. An empty name goes back to "Profile N".
 */
export function NameField({
  value,
  index,
  maxLength,
  onSave,
  onDone,
}: {
  value: string;
  index: number;
  /** As long as the mouse keeps a name (a Logitech's 23 characters); no limit for names kept here. */
  maxLength?: number;
  onSave: (name: string) => void;
  onDone: () => void;
}) {
  const save = (v: string) => {
    // hrtz's stand-in name, kept in English: the backend and a Logitech's driver know it by that.
    const name = v.trim() || `Profile ${index + 1}`;
    if (name !== value) onSave(name);
    onDone();
  };
  return (
    <Input
      autoFocus
      defaultValue={value}
      maxLength={maxLength}
      aria-label={t("Name for profile {{n}}", { n: index + 1 })}
      style={{ width: "100%", maxWidth: 280 }}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={(e) => save(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") save(e.currentTarget.value);
        if (e.key === "Escape") onDone();
      }}
    />
  );
}
