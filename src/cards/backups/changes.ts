import { lodName, sensorAngleName, type Settings } from "@/api";
import { LED_PALETTE } from "@/cards/dpi-stages/palette";
import { fmtDpi } from "@/dpi";
import { formatDuration } from "@/format";
import { actionLabel } from "@/hid";
import { msg, num, t } from "@/i18n";
import { swatchName } from "@/kit";
import { hzText } from "@/rates";

const MODE = { lp: msg("Low power"), hp: msg("High performance"), corded: msg("Corded") };
const onOff = (b: boolean) => (b ? t("On") : t("Off"));
const EFFECT = { off: msg("Off"), steady: msg("Steady"), breathing: msg("Breathing") };

/** `f(v)`, or nothing when the setting isn't there. */
const show = <T,>(v: T | undefined, f: (v: T) => string) => (v === undefined ? undefined : f(v));

/** What restoring `then` (a backup) over `now` (the mouse) changes, as label, now and then. A
 *  setting either side lacks isn't compared. */
export function changes(now: Settings, then: Settings, buttonNames: [number, string][]): [string, string, string][] {
  const out: [string, string, string][] = [];
  const cmp = (label: string, a: string | undefined, b: string | undefined) => a !== undefined && b !== undefined && a !== b && out.push([label, a, b]);
  cmp(t("Polling rate"), show(now.pollingHz, hzText), show(then.pollingHz, hzText));
  cmp(t("DPI stages"), show(now.dpiStageCount, num), show(then.dpiStageCount, num));
  cmp(t("Stage in use"), show(now.dpiStage, (i) => num(i + 1)), show(then.dpiStage, (i) => num(i + 1)));
  // Only the stages the backup cycles through: the ones past its count go unused once it's back.
  for (let i = 0; i < (then.dpiStageCount ?? 0); i++) {
    const a = now.stages?.[i];
    const b = then.stages?.[i];
    cmp(t("Stage {{n}} DPI", { n: i + 1 }), show(a?.dpi, fmtDpi), show(b?.dpi, fmtDpi));
    cmp(t("Stage {{n}} colour", { n: i + 1 }), show(a?.color, (c) => swatchName(LED_PALETTE, c)), show(b?.color, (c) => swatchName(LED_PALETTE, c)));
  }
  cmp(t("DPI light"), show(now.dpiLight?.effect, (v) => t(EFFECT[v])), show(then.dpiLight?.effect, (v) => t(EFFECT[v])));
  cmp(t("DPI light brightness"), show(now.dpiLight?.brightness, num), show(then.dpiLight?.brightness, num));
  cmp(t("DPI light speed"), show(now.dpiLight?.speed, num), show(then.dpiLight?.speed, num));
  cmp(t("Lift-off distance"), show(now.lod, lodName), show(then.lod, lodName));
  cmp(t("Sensor mode"), show(now.sensorMode, (v) => t(MODE[v])), show(then.sensorMode, (v) => t(MODE[v])));
  cmp(t("Sensor angle"), show(now.sensorAngle, sensorAngleName), show(then.sensorAngle, sensorAngleName));
  cmp(t("Motion sync"), show(now.motionSync, onOff), show(then.motionSync, onOff));
  cmp(t("Angle snapping"), show(now.angleSnapping, onOff), show(then.angleSnapping, onOff));
  cmp(t("Ripple control"), show(now.rippleControl, onOff), show(then.rippleControl, onOff));
  cmp(t("Click debounce"), show(now.debounceMs, (v) => t("{{n}} ms", { n: num(v) })), show(then.debounceMs, (v) => t("{{n}} ms", { n: num(v) })));
  cmp(t("Peak performance timeout"), show(now.peakTimeoutEnabled, onOff), show(then.peakTimeoutEnabled, onOff));
  cmp(t("Peak performance after"), show(now.peakTimeoutS, formatDuration), show(then.peakTimeoutS, formatDuration));
  cmp(t("Sleep after"), show(now.sleepS, formatDuration), show(then.sleepS, formatDuration));
  for (const [i, name] of buttonNames) {
    cmp(t("{{name}} button", { name }), show(now.buttons?.[i], actionLabel), show(then.buttons?.[i], actionLabel));
  }
  return out;
}
