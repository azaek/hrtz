import { useEffect, useState } from "react";
import { Button } from "@fluentui/react-components";
import { Cursor20Regular, Flash20Regular, Warning20Regular } from "@fluentui/react-icons";
import { api, message, type PointerSettings } from "@/api";
import { Group, SettingCard } from "@/components";
import { num, t } from "@/i18n";
import { toast } from "@/store";

/** A pointer speed the user chose to keep; the card comes back if it changes. */
const KEEP_KEY = "hrtz.keepPointerSpeed";

/** Pointer movement per count at each speed with acceleration off (Windows' own table). */
const SPEED_SCALE = [0, 1 / 32, 1 / 16, 1 / 8, 2 / 8, 3 / 8, 4 / 8, 5 / 8, 6 / 8, 7 / 8, 1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 2.75, 3, 3.25, 3.5];

/** Where a speed sits on the Settings slider (1 to 11); the slider writes the even values. */
const sliderStep = (speed: number) => (speed <= 2 ? speed : Math.round(speed / 2) + 1);

/** How far each count moves the pointer at `speed`, in a sentence. */
function perCount(speed: number) {
  const k = SPEED_SCALE[speed] ?? 1;
  if (k === 1) return t("Each count moves the pointer one pixel.");
  if (k === 0.5) return t("Each count moves the pointer half a pixel.");
  if (k < 1) return t("Each count moves the pointer {{percent}}% of a pixel.", { percent: Math.round(k * 100) });
  return t("Each count moves the pointer {{pixels}} pixels.", { count: k, pixels: num(k) });
}

/**
 * Windows settings that change what the pointer does with the mouse's counts. Shown only when one
 * of them is off; games that read raw input ignore them, the desktop and some games don't.
 */
export function WindowsCheck() {
  const [p, setP] = useState<PointerSettings | null>(null);
  const [kept, setKept] = useState(() => Number(localStorage.getItem(KEEP_KEY)) || 0);

  // They can change in Windows Settings while this is open, so read again on coming back.
  useEffect(() => {
    const read = () => api.pointerSettings().then(setP, () => {});
    read();
    const again = () => !document.hidden && read();
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
    return () => {
      window.removeEventListener("focus", again);
      document.removeEventListener("visibilitychange", again);
    };
  }, []);

  if (!p) return null;
  const fix = (acceleration: boolean, speed: boolean) =>
    api.fixPointer(acceleration, speed).then((x) => x && setP(x), (e) => toast(message(e), "error"));
  const keep = () => {
    localStorage.setItem(KEEP_KEY, String(p.speed));
    setKept(p.speed);
  };

  const cards = [];
  if (p.acceleration) {
    cards.push(
      <SettingCard
        key="acceleration"
        icon={<Flash20Regular />}
        title={t("Windows pointer acceleration is on")}
        description={t("Moves the pointer further when you move faster. Games using raw input ignore it.")}
      >
        <Button onClick={() => fix(true, false)}>{t("Turn off")}</Button>
      </SettingCard>,
    );
  }
  if (p.speed !== 10 && kept !== p.speed) {
    const step = sliderStep(p.speed);
    cards.push(
      <SettingCard
        key="speed"
        icon={<Cursor20Regular />}
        title={t("Windows pointer speed is {{step}} of 11", { step })}
        description={`${perCount(p.speed)} ${t("6 of 11 is one-to-one.")}`}
      >
        <Button appearance="subtle" onClick={keep}>
          {t("Keep {{step}} of 11", { step })}
        </Button>
        <Button onClick={() => fix(false, true)}>{t("Set to 6 of 11")}</Button>
      </SettingCard>,
    );
  }
  if (p.rawAccel) {
    cards.push(
      <SettingCard
        key="rawaccel"
        icon={<Warning20Regular />}
        title={t("Raw Accel is installed")}
        description={t("It changes what every app receives, so tests measure its output.")}
      />,
    );
  }
  return cards.length ? <Group title="Windows">{cards}</Group> : null;
}
