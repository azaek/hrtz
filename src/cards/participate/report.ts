import { getVersion } from "@tauri-apps/api/app";
import { api, type DeviceState } from "@/api";
import { linkName } from "@/links";
import { familyOf, modelKeyOf } from "@/models";
import type { Checks } from "./useParticipate";

const fmt = (n: number) => n.toLocaleString("en-US");

/**
 * The report a tester sends: what hrtz knows of the mouse (its model, firmware, capabilities and
 * settings), what the check found, and the mouse's memory and its family's facts. Nothing that
 * tells this mouse or this PC apart: the key is the model's (a Logitech's unit ID goes), and the
 * port isn't kept.
 */
export async function buildReport(device: DeviceState, checks: Checks, dump: unknown) {
  const [hrtz, windows] = await Promise.all([getVersion().catch(() => ""), api.windowsVersion().catch(() => "")]);
  return {
    format: "hrtz-mouse-report/1",
    created: new Date().toISOString(),
    hrtz,
    windows,
    mouse: {
      model: device.model,
      modelKey: device.key ? modelKeyOf(device.key) : null,
      family: familyOf(device.key) ?? null,
      firmware: device.firmware,
      variant: device.variant ?? null,
      link: linkName(device),
      cid: device.cid,
      mid: device.mid,
      deviceType: device.deviceType,
    },
    capabilities: device.capabilities,
    settings: device.settings,
    battery: device.battery,
    hostMode: device.hostMode ?? false,
    checks,
    device: dump,
  };
}

/** What the check found, a line each, for the card and the GitHub issue. */
export function summaryLines(c: Checks): { ok: boolean | null; text: string }[] {
  const lines: { ok: boolean | null; text: string }[] = [];
  lines.push(c.read.ok ? { ok: true, text: `Settings and memory read (${fmt(c.read.bytes ?? 0)} bytes)` } : { ok: false, text: `Memory not read: ${c.read.error ?? "no answer"}` });
  if (c.polling) {
    const { set, measured, stutters } = c.polling;
    const steady = stutters <= 2;
    lines.push({
      ok: measured !== null && steady,
      text: measured === null ? "Polling not measured" : `${fmt(measured)} reports a second${set ? ` at ${fmt(set)} Hz` : ""}, ${steady ? "steady" : `${stutters} stutters`}`,
    });
  }
  const seeable = c.buttons.filter((b) => b.seen !== null);
  if (seeable.length) {
    const seen = seeable.filter((b) => b.seen);
    lines.push({ ok: seen.length === seeable.length, text: `Buttons: ${seen.length} of ${seeable.length} seen` });
    for (const b of seeable.filter((x) => !x.seen)) lines.push({ ok: false, text: `${b.name}: no press seen (set to ${b.does})` });
  }
  for (const b of c.buttons.filter((x) => x.seen === null)) lines.push({ ok: null, text: `${b.name} is set to ${b.does}, which hrtz can't see` });
  if (c.write) {
    const w = c.write;
    lines.push(
      w.error
        ? { ok: false, text: `Change failed: ${w.error}` }
        : { ok: w.kept && w.restored, text: `${w.what} ${w.from} → ${w.to}: ${w.kept ? "kept" : "not kept"}, ${w.restored ? "put back" : "not put back"}` },
    );
  }
  return lines;
}

/** The summary as plain text, for the issue's "hrtz's check" field. */
export const summaryText = (c: Checks) => summaryLines(c).map((l) => `${l.ok === null ? "-" : l.ok ? "OK" : "FAILED"}  ${l.text}`).join("\n");

/** The file's name: the model and the day. */
export const reportName = (device: DeviceState) =>
  `hrtz-report-${(device.model ?? "mouse").toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${new Date().toISOString().slice(0, 10)}.json`;
