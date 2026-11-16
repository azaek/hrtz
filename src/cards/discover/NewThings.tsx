import { useState } from "react";
import { Button, Link } from "@fluentui/react-components";
import { CalendarLtr20Regular, EyeOff20Regular, TargetArrow20Regular } from "@fluentui/react-icons";
import { Group, SettingCard } from "@/components";
import { openTest } from "@/cards/lab/tests";
import { pairsFor } from "@/cards/lab/blind";
import { distance, useWeek } from "@/cards/week/week";
import { goTo, useStore } from "@/store";

const HIDE_KEY = "hrtz.newThingsHidden";

/** "New in hrtz": the three new things at the top of the Sensor page, each a button away, until
 *  hidden. (The features lab's "front" direction.) */
export function NewThings() {
  const device = useStore((x) => x.device);
  const week = useWeek();
  const [hidden, setHidden] = useState(() => localStorage.getItem(HIDE_KEY) === "1");
  if (hidden) return null;
  const pair = pairsFor(device?.capabilities, device?.settings)[0];
  const hide = () => {
    localStorage.setItem(HIDE_KEY, "1");
    setHidden(true);
  };
  return (
    <>
      <Group title="New in hrtz">
        {pair && (
          <SettingCard icon={<EyeOff20Regular />} title={`Can you feel ${pair.feel}?`} description="A blind test: hrtz switches it without telling you, and you guess. About 2 minutes.">
            <Button onClick={() => openTest("blind")}>Take the test</Button>
          </SettingCard>
        )}
        <SettingCard
          icon={<CalendarLtr20Regular />}
          title="Your week with the mouse"
          description={week && week.cm > 0 ? `${distance(week.cm)} this week, your fastest flick, clicks and games.` : "Distance, your fastest flick, clicks and games, counted as you play."}
        >
          <Button onClick={() => goTo("week")}>See your week</Button>
        </SettingCard>
        <SettingCard icon={<TargetArrow20Regular />} title="Sensitivity in your games" description="One feel in every game: hrtz reads each game's settings and says what to set.">
          <Button onClick={() => document.getElementById("games-card")?.scrollIntoView({ behavior: "smooth", block: "center" })}>Show</Button>
        </SettingCard>
      </Group>
      <div className="tw -mt-2 flex justify-end px-1 text-sm">
        <Link as="button" onClick={hide}>
          Hide these
        </Link>
      </div>
    </>
  );
}
