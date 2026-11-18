import { useState } from "react";
import { Button, Link } from "@fluentui/react-components";
import { CalendarLtr20Regular, EyeOff20Regular, TargetArrow20Regular } from "@fluentui/react-icons";
import { Group, SettingCard } from "@/components";
import { openTest } from "@/cards/lab/tests";
import { pairsFor } from "@/cards/lab/blind";
import { distance, useWeek } from "@/cards/week/week";
import { t } from "@/i18n";
import { goTo, useStore } from "@/store";
import { NewBadge } from "@/kit";
import { useGames } from "@/cards/games/GamesCard";

const HIDE_KEY = "hrtz.newThingsHidden";

/** "New in hrtz": the three new things at the top of the Sensor page, the page hrtz opens on, each
 *  a button away, until hidden (the user's pick in the features lab, DECISIONS.md "Three things to
 *  try"). */
export function NewThings() {
  const device = useStore((x) => x.device);
  const seen = useStore((x) => x.prefs?.seen);
  const week = useWeek();
  const games = useGames();
  const [hidden, setHidden] = useState(() => localStorage.getItem(HIDE_KEY) === "1");
  if (hidden) return null;
  // Each keeps its New until it's been seen where it lives; seeing it here doesn't count, so the
  // badge is still there to find once you get to it.
  const title = (text: string, id: string) => (
    <span className="flex items-center gap-2">
      {text} {seen && !seen.includes(id) && <NewBadge />}
    </span>
  );
  const pair = pairsFor(device?.capabilities, device?.settings)[0];
  const hide = () => {
    localStorage.setItem(HIDE_KEY, "1");
    setHidden(true);
  };
  return (
    <Group
      title={t("New in hrtz")}
      action={
        <Link as="button" onClick={hide}>
          {t("Hide these")}
        </Link>
      }
    >
      {pair && (
        <SettingCard icon={<EyeOff20Regular />} title={title(t("Can you feel {{feel}}?", { feel: pair.feel }), "blindtest")} description={t("A blind test: hrtz switches it without telling you, and you guess. About 2 minutes.")}>
          <Button onClick={() => openTest("blind")}>{t("Take the test")}</Button>
        </SettingCard>
      )}
      <SettingCard
        icon={<CalendarLtr20Regular />}
        title={title(t("Your week with the mouse"), "week")}
        description={week && week.cm > 0 ? t("{{distance}} this week, your fastest flick, clicks and games.", { distance: distance(week.cm) }) : t("Distance, your fastest flick, clicks and games, counted as you play.")}
      >
        <Button onClick={() => goTo("week")}>{t("See your week")}</Button>
      </SettingCard>
      {/* Only once a game's settings are read: the card it opens shows only then. */}
      {games.length > 0 && (
        <SettingCard
          icon={<TargetArrow20Regular />}
          title={title(t("Sensitivity in your games"), "gamesens")}
          description={t("One feel in {{games}}: hrtz reads each game's settings and says what to set.", { games: games.map((g) => g.game).join(", ") })}
        >
          <Button onClick={() => document.getElementById("games-card")?.scrollIntoView({ behavior: "smooth", block: "center" })}>{t("Show")}</Button>
        </SettingCard>
      )}
    </Group>
  );
}
