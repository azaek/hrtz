import { currentDpi, type Capabilities, type ProfileSnapshot, type Settings } from "@/api";
import { DEFAULT_ACTIONS, sameAction } from "@/cards/buttons/functions";
import { fmtDpi } from "@/dpi";
import { hzText } from "@/rates";

/** What a profile holds, for the Profiles page. */
export type Summary = { dpi: number; stages: number; hz: number; buttonsChanged: number };

/**
 * What profile `i` holds: read live for the profile in use, otherwise what the app remembered
 * the last time it was in use (nothing if it never was, or it was another model's).
 */
export function summaryOf(i: number, inUse: number | null, live: Settings | null, snaps: (ProfileSnapshot | null)[] | undefined, mid: number, caps: Capabilities | null): Summary | null {
  const physical = caps?.buttons ?? [];
  const changed = (buttons: Settings["buttons"] = []) => physical.filter(([b]) => DEFAULT_ACTIONS[b] && !sameAction(buttons[b], DEFAULT_ACTIONS[b])).length;
  if (i === inUse && live) {
    return { dpi: currentDpi(live) ?? 0, stages: live.dpiStageCount ?? 0, hz: live.pollingHz ?? 0, buttonsChanged: changed(live.buttons) };
  }
  const s = snaps?.[i];
  if (!s || s.mid !== mid) return null;
  return { dpi: s.dpi, stages: s.stages, hz: s.pollingHz, buttonsChanged: changed(s.buttons) };
}

/** "1,600 DPI · 8,000 Hz · 2 buttons changed", or "Not seen yet". */
export function summaryText(s: Summary | null) {
  if (!s) return "Not seen yet";
  const buttons = s.buttonsChanged ? ` · ${s.buttonsChanged} button${s.buttonsChanged > 1 ? "s" : ""} changed` : "";
  return `${fmtDpi(s.dpi)} DPI · ${hzText(s.hz)}${buttons}`;
}

/** Why a profile has no summary. */
export const UNSEEN = "Switch to this profile once to see its settings. The mouse only reports the active profile.";
