import { useEffect, useState } from "react";
import { Button } from "@fluentui/react-components";
import { Cursor20Regular, Flash20Regular, Warning20Regular } from "@fluentui/react-icons";
import { api, message, type PointerSettings } from "@/api";
import { Group, SettingCard } from "@/components";
import { toast } from "@/store";

/** A pointer speed the user chose to keep; the card comes back if it changes. */
const KEEP_KEY = "hrtz.keepPointerSpeed";

/** Pointer movement per count at each speed with acceleration off (Windows' own table). */
const SPEED_SCALE = [0, 1 / 32, 1 / 16, 1 / 8, 2 / 8, 3 / 8, 4 / 8, 5 / 8, 6 / 8, 7 / 8, 1, 1.25, 1.5, 1.75, 2, 2.25, 2.5, 2.75, 3, 3.25, 3.5];

/** Where a speed sits on the Settings slider (1 to 11); the slider writes the even values. */
const sliderStep = (speed: number) => (speed <= 2 ? speed : Math.round(speed / 2) + 1);

function perCount(speed: number) {
  const k = SPEED_SCALE[speed] ?? 1;
  if (k === 1) return "one pixel";
  if (k === 0.5) return "half a pixel";
  if (k < 1) return `${Math.round(k * 100)}% of a pixel`;
  return `${k.toString().replace(/\.?0+$/, "")} pixels`;
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
        title="Windows pointer acceleration is on"
        description="Moves the pointer further when you move faster. Games using raw input ignore it."
      >
        <Button onClick={() => fix(true, false)}>Turn off</Button>
      </SettingCard>,
    );
  }
  if (p.speed !== 10 && kept !== p.speed) {
    const step = sliderStep(p.speed);
    cards.push(
      <SettingCard
        key="speed"
        icon={<Cursor20Regular />}
        title={`Windows pointer speed is ${step} of 11`}
        description={`Each count moves the pointer ${perCount(p.speed)}. 6 of 11 is one-to-one.`}
      >
        <Button appearance="subtle" onClick={keep}>
          Keep {step} of 11
        </Button>
        <Button onClick={() => fix(false, true)}>Set to 6 of 11</Button>
      </SettingCard>,
    );
  }
  if (p.rawAccel) {
    cards.push(
      <SettingCard
        key="rawaccel"
        icon={<Warning20Regular />}
        title="Raw Accel is installed"
        description="It changes what every app receives, so tests measure its output."
      />,
    );
  }
  return cards.length ? <Group title="Windows">{cards}</Group> : null;
}
