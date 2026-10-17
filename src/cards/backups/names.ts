import type { AutoReason, BackupMeta } from "@/api";

/** "15 Oct, 01:42", with the year only when it isn't this one. */
export function shortWhen(created: string) {
  const d = new Date(created);
  if (Number.isNaN(d.getTime())) return created;
  const sameYear = d.getFullYear() === new Date().getFullYear();
  const day = d.toLocaleDateString(undefined, { day: "numeric", month: "short", ...(sameYear ? {} : { year: "numeric" }) });
  return `${day}, ${d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}`;
}

/** Why hrtz saved a backup by itself, in a few words. */
export function autoText(a: AutoReason) {
  switch (a.kind) {
    case "first":
      return "First connected to hrtz";
    case "restore":
      return `Before restoring ${a.note || shortWhen(a.created)}`;
    case "factory":
      return "Before factory reset";
  }
}

/** What a backup is called: its name, else why hrtz saved it, else when. */
export const nameOf = (b: BackupMeta) => b.note || (b.auto ? autoText(b.auto) : shortWhen(b.created));

/**
 * The line under a backup's name: when (unless the name is the date), the model when another one
 * is connected, the firmware, and the profile it came from.
 */
export function lineOf(b: BackupMeta, connectedModel: string | null) {
  return [
    b.note || b.auto ? shortWhen(b.created) : null,
    connectedModel && b.model !== connectedModel ? b.model : null,
    `firmware ${b.firmware}`,
    b.profile !== null ? `profile ${b.profile + 1}` : null,
  ]
    .filter(Boolean)
    .join(" · ");
}
