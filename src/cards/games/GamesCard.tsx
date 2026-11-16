import { useEffect, useState } from "react";
import { Body1, Dropdown, Option } from "@fluentui/react-components";
import { TargetArrow20Regular } from "@fluentui/react-icons";
import { api, currentDpi, type GameSensitivity } from "@/api";
import { SettingCard } from "@/components";
import { CardList, CardListRow, NewBadge } from "@/kit";
import { useSeen } from "@/news";
import { useStore } from "@/store";
import { useWeek } from "@/cards/week/week";

/** The games whose settings files hrtz reads, by one of their exes. */
const READ = ["valorant-win64-shipping.exe", "r5apex_dx12.exe", "cs2.exe"];
/** Turns within this much of each other feel the same. */
const SAME = 0.03;
/** The game the others are matched to, as last picked here. */
const MAIN_KEY = "hrtz.mainGame";

/** How far the mouse travels for a full turn: 914.4 / (yaw × sensitivity × DPI) cm. */
export const cm360 = (g: GameSensitivity, dpi: number) => 914.4 / (g.yaw * g.sens * dpi);
/** The sensitivity that gives game `g` a full turn of `cm`, at `dpi`. */
export const sensFor = (g: GameSensitivity, cm: number, dpi: number) => 914.4 / (g.yaw * cm * dpi);
/** A sensitivity as you'd type it: three decimals at most. */
const sensText = (s: number) => String(Number(s.toFixed(3)));
const cmText = (cm: number) => `${cm >= 100 ? Math.round(cm) : cm.toFixed(1)} cm`;

/** Each game's sensitivity as its settings file has it now, read again every 30 s while shown. */
export function useGames(): (GameSensitivity & { exe: string })[] {
  const [games, setGames] = useState<(GameSensitivity & { exe: string })[]>([]);
  useEffect(() => {
    const read = () =>
      Promise.all(READ.map((exe) => api.gameSensitivity(exe).then((g) => (g && !g.acceleration ? { ...g, exe } : null), () => null))).then((all) =>
        setGames(all.filter((g): g is GameSensitivity & { exe: string } => !!g)),
      );
    void read();
    const t = setInterval(() => !document.hidden && void read(), 30_000);
    return () => clearInterval(t);
  }, []);
  return games;
}

/**
 * Sensitivity in your games: how far the mouse travels for a full turn in each game whose
 * settings hrtz reads, at the DPI in use, and the sensitivity that would make the others feel like
 * the one you pick (your most played, unless you choose). Shown once a game is found.
 */
export function GamesCard() {
  const dpi = useStore((x) => currentDpi(x.device?.settings));
  const games = useGames();
  const week = useWeek();
  const news = useSeen("gamesens");
  const played = week?.games.find((g) => games.some((x) => sameGame(x.exe, g.exe)))?.exe;
  const [picked, setPicked] = useState(() => localStorage.getItem(MAIN_KEY));
  if (!dpi || games.length === 0) return null;
  const main = games.find((g) => g.game === picked) ?? games.find((g) => played && sameGame(g.exe, played)) ?? games[0];
  const target = cm360(main, dpi);
  const pick = (game: string) => {
    setPicked(game);
    localStorage.setItem(MAIN_KEY, game);
  };

  return (
    <SettingCard
      stacked
      icon={<TargetArrow20Regular />}
      title={
        <span className="flex items-center gap-2">
          Sensitivity in your games {news.isNew && <NewBadge ref={news.ref} />}
        </span>
      }
      description={`How far the mouse travels for a full turn in each game, at ${dpi.toLocaleString("en-US")} DPI.`}
      below={
        <div className="tw pl-9">
          <CardList>
            {games.map((g) => {
              const cm = cm360(g, dpi);
              const off = Math.abs(cm - target) / target;
              const to = sensFor(g, target, dpi);
              return (
                <CardListRow key={g.game}>
                  <Body1 className="min-w-0 flex-1 truncate">{g.game}</Body1>
                  <span className="text-sm tabular-nums text-muted-foreground">
                    {sensText(g.sens)} · {cmText(cm)} a turn
                  </span>
                  <span className="w-44 text-right text-sm tabular-nums">
                    {g === main ? <span className="text-muted-foreground">Matched to this</span> : off <= SAME ? "Feels the same" : `Set ${sensText(to)} to match`}
                  </span>
                </CardListRow>
              );
            })}
          </CardList>
        </div>
      }
    >
      {games.length > 1 && (
        <Dropdown aria-label="Match to" value={`Match to ${main.game}`} selectedOptions={[main.game]} onOptionSelect={(_, d) => d.optionValue && pick(d.optionValue)} style={{ minWidth: 180 }}>
          {games.map((g) => (
            <Option key={g.game} value={g.game} text={`Match to ${g.game}`}>
              {g.game}
            </Option>
          ))}
        </Dropdown>
      )}
    </SettingCard>
  );
}

/** Apex has two exes; the activity log keeps whichever ran. */
const sameGame = (a: string, b: string) => a === b || (a.startsWith("r5apex") && b.startsWith("r5apex"));
