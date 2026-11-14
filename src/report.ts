import { getVersion } from "@tauri-apps/api/app";
import { api, type DeviceState } from "./api";
import { linkName } from "./links";
import { REPO_URL } from "./update";

/** A new GitHub issue on one of the forms in .github/ISSUE_TEMPLATE. GitHub fills a form's field
 *  from the query parameter named as the field's `id`, and the issue's title from `title`. */
function issueUrl(form: string, fields: [string, string | null | undefined][]): string {
  const query = [["template", `${form}.yml`] as [string, string], ...fields]
    .filter((f): f is [string, string] => !!f[1])
    .map(([k, v]) => `${k}=${encodeURIComponent(v)}`)
    .join("&");
  return `${REPO_URL}/issues/new?${query}`;
}

/**
 * "Report a problem": the bug form, its fields filled in with what hrtz knows: its version, the
 * mouse, its firmware, the connection and Windows.
 */
export async function reportUrl(device: DeviceState | null): Promise<string> {
  const [version, windows] = await Promise.all([getVersion().catch(() => ""), api.windowsVersion().catch(() => "")]);
  return issueUrl("bug_report", [
    ["version", version],
    ["mouse", device?.connected ? device.model : null],
    ["firmware", device?.connected ? device.firmware : null],
    ["connection", device?.connected ? linkName(device) : null],
    ["windows", windows],
  ]);
}

/**
 * Inspect a mouse's issue: a test report for a model hrtz knows, or a request to add one it
 * doesn't. `mouse` is its name, where there's one to give; `report` goes into the form's one box.
 * Its caller keeps the link short enough for GitHub to open (`issueOf`).
 */
export function mouseIssueUrl(form: "mouse_report" | "mouse_request", mouse: string, report: string): string {
  const title = mouse && `${form === "mouse_report" ? "Mouse report" : "Mouse request"}: ${mouse}`;
  return issueUrl(form, [
    ["title", title],
    ["mouse", mouse],
    ["report", report],
  ]);
}
