import type { Settings } from "@/api";
import { LED_PALETTE } from "@/cards/dpi-stages/palette";
import { fmtDpi } from "@/dpi";
import { formatDuration } from "@/format";
import { actionLabel } from "@/hid";
import { swatchName } from "@/kit";
import { hzText } from "@/rates";

const LOD = { "0.7mm": "0.7 mm", "1mm": "1 mm", "2mm": "2 mm" };
const MODE = { lp: "Low power", hp: "High performance", corded: "Corded" };
const onOff = (b: boolean) => (b ? "On" : "Off");

/** `f(v)`, or nothing when the setting isn't there. */
const show = <T,>(v: T | undefined, f: (v: T) => string) => (v === undefined ? undefined : f(v));

/** What restoring `then` (a backup) over `now` (the mouse) changes, as label, now and then. A
 *  setting either side lacks isn't compared. */
export function changes(now: Settings, then: Settings, buttonNames: [number, string][]): [string, string, string][] {
  const out: [string, string, string][] = [];
  const cmp = (label: string, a: string | undefined, b: string | undefined) => a !== undefined && b !== undefined && a !== b && out.push([label, a, b]);
  cmp("Polling rate", show(now.pollingHz, hzText), show(then.pollingHz, hzText));
  cmp("DPI stages", show(now.dpiStageCount, String), show(then.dpiStageCount, String));
  cmp("Stage in use", show(now.dpiStage, (i) => String(i + 1)), show(then.dpiStage, (i) => String(i + 1)));
  // Only the stages the backup cycles through: the ones past its count go unused once it's back.
  for (let i = 0; i < (then.dpiStageCount ?? 0); i++) {
    const a = now.stages?.[i];
    const b = then.stages?.[i];
    cmp(`Stage ${i + 1} DPI`, show(a?.dpi, fmtDpi), show(b?.dpi, fmtDpi));
    cmp(`Stage ${i + 1} colour`, show(a?.color, (c) => swatchName(LED_PALETTE, c)), show(b?.color, (c) => swatchName(LED_PALETTE, c)));
  }
  cmp("Lift-off distance", show(now.lod, (v) => LOD[v]), show(then.lod, (v) => LOD[v]));
  cmp("Sensor mode", show(now.sensorMode, (v) => MODE[v]), show(then.sensorMode, (v) => MODE[v]));
  cmp("Motion sync", show(now.motionSync, onOff), show(then.motionSync, onOff));
  cmp("Angle snapping", show(now.angleSnapping, onOff), show(then.angleSnapping, onOff));
  cmp("Ripple control", show(now.rippleControl, onOff), show(then.rippleControl, onOff));
  cmp("Click debounce", show(now.debounceMs, (v) => `${v} ms`), show(then.debounceMs, (v) => `${v} ms`));
  cmp("Peak performance timeout", show(now.peakTimeoutEnabled, onOff), show(then.peakTimeoutEnabled, onOff));
  cmp("Peak performance after", show(now.peakTimeoutS, formatDuration), show(then.peakTimeoutS, formatDuration));
  cmp("Sleep after", show(now.sleepS, formatDuration), show(then.sleepS, formatDuration));
  for (const [i, name] of buttonNames) {
    cmp(`${name} button`, show(now.buttons?.[i], actionLabel), show(then.buttons?.[i], actionLabel));
  }
  return out;
}
