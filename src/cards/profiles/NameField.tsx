import { Input } from "@fluentui/react-components";

/**
 * A profile's name as a field, opened by Rename: the name starts selected, Enter or leaving the
 * field saves, Escape cancels. An empty name goes back to "Profile N".
 */
export function NameField({ value, index, onSave, onDone }: { value: string; index: number; onSave: (name: string) => void; onDone: () => void }) {
  const save = (v: string) => {
    const name = v.trim() || `Profile ${index + 1}`;
    if (name !== value) onSave(name);
    onDone();
  };
  return (
    <Input
      autoFocus
      defaultValue={value}
      aria-label={`Name for profile ${index + 1}`}
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
