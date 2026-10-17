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

/** What restoring `then` (a backup) over `now` (the mouse) changes, as label, now and then. */
export function changes(now: Settings, then: Settings, buttonNames: [number, string][]): [string, string, string][] {
  const out: [string, string, string][] = [];
  const cmp = (label: string, a: string, b: string) => a !== b && out.push([label, a, b]);
  cmp("Polling rate", hzText(now.pollingHz), hzText(then.pollingHz));
  cmp("DPI stages", String(now.dpiStageCount), String(then.dpiStageCount));
  cmp("Stage in use", String(now.dpiStage + 1), String(then.dpiStage + 1));
  // Only the stages the backup cycles through: the ones past its count go unused once it's back.
  for (let i = 0; i < then.dpiStageCount; i++) {
    cmp(`Stage ${i + 1} DPI`, fmtDpi(now.stages[i].dpi), fmtDpi(then.stages[i].dpi));
    cmp(`Stage ${i + 1} colour`, swatchName(LED_PALETTE, now.stages[i].color), swatchName(LED_PALETTE, then.stages[i].color));
  }
  cmp("Lift-off distance", LOD[now.lod], LOD[then.lod]);
  cmp("Sensor mode", MODE[now.sensorMode], MODE[then.sensorMode]);
  cmp("Motion sync", onOff(now.motionSync), onOff(then.motionSync));
  cmp("Angle snapping", onOff(now.angleSnapping), onOff(then.angleSnapping));
  cmp("Ripple control", onOff(now.rippleControl), onOff(then.rippleControl));
  cmp("Click debounce", `${now.debounceMs} ms`, `${then.debounceMs} ms`);
  cmp("Peak performance timeout", onOff(now.peakTimeoutEnabled), onOff(then.peakTimeoutEnabled));
  cmp("Peak performance after", formatDuration(now.peakTimeoutS), formatDuration(then.peakTimeoutS));
  cmp("Sleep after", formatDuration(now.sleepS), formatDuration(then.sleepS));
  for (const [i, name] of buttonNames) {
    cmp(`${name} button`, actionLabel(now.buttons[i]), actionLabel(then.buttons[i]));
  }
  return out;
}
