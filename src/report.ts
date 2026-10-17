import { getVersion } from "@tauri-apps/api/app";
import { api, type DeviceState } from "./api";
import { linkName } from "./links";
import { REPO_URL } from "./update";


export type ReportForm = "bug_report" | "mouse_report";

/**
 * A new GitHub issue on one of the forms in .github/ISSUE_TEMPLATE, its fields filled in with what
 * hrtz knows: its version, the mouse, its firmware, the connection and Windows. GitHub fills a
 * form's field from the query parameter named as the field's `id`.
 */
export async function reportUrl(form: ReportForm, device: DeviceState | null): Promise<string> {
  const [version, windows] = await Promise.all([getVersion().catch(() => ""), api.windowsVersion().catch(() => "")]);
  const fields: [string, string | null | undefined][] = [
    ["template", `${form}.yml`],
    ["version", version],
    ["mouse", device?.connected ? device.model : null],
    ["firmware", device?.connected ? device.firmware : null],
    ["connection", device?.connected ? linkName(device) : null],
    ["windows", windows],
  ];
  const query = fields
    .filter((f): f is [string, string] => !!f[1])
    .map(([k, v]) => `${k}=${encodeURIComponent(v)}`)
    .join("&");
  return `${REPO_URL}/issues/new?${query}`;
}
