import { Body1, Link, Switch } from "@fluentui/react-components";
import { CalendarLtr20Regular, Games20Regular, ShieldLock20Regular } from "@fluentui/react-icons";
import { Group, SettingCard } from "@/components";
import { CardList, CardListRow, NewBadge, StatTile } from "@/kit";
import { useSeen } from "@/news";
import { savePrefs, toast, useStore } from "@/store";
import { dayName, distance, pitches, summaryText, timeText, useWeek, versus, type Week } from "./week";

const fmt = (n: number) => Math.round(n).toLocaleString("en-US");
const BUTTONS = ["Left", "Right", "Middle", "Back", "Forward"];

/** A game's name, as the pad map's list has it, else its exe without ".exe". */
function useGameName() {
  const games = useStore((x) => x.prefs?.padGames);
  return (exe: string) => games?.find((g) => g.exe.toLowerCase() === exe)?.name ?? exe.replace(/\.exe$/i, "");
}

/**
 * Your week with the mouse: how far it travelled, how long it moved, its fastest moment and the
 * clicks, then each day, then each game. Counted in the background on this PC, totals only, from
 * the mouse hrtz runs.
 */
export function WeekOverview() {
  const w = useWeek();
  const news = useSeen("week");
  const prefs = useStore((x) => x.prefs);
  const name = useGameName();
  const counting = prefs?.activity ?? true;
  const empty = !!w && w.cm === 0;
  const unknown = <StatTile.Unknown />;
  const copy = () =>
    w &&
    void navigator.clipboard.writeText(summaryText(w, name)).then(
      () => toast("Summary copied"),
      () => {},
    );
  const busiest = w?.buttons.indexOf(Math.max(...w.buttons)) ?? 0;

  return (
    <>
      <div className="tw @container/week">
        <div className="grid grid-cols-2 gap-2 @min-[680px]/week:grid-cols-4">
          <StatTile label="Distance" value={w && !empty ? distance(w.cm) : unknown} detail={w && !empty ? pitches(w.cm) : "As you use the mouse"} />
          <StatTile label="Time moving" value={w && !empty ? timeText(w.movingS) : unknown} detail={w && !empty ? `${timeText(w.movingS / 7)} a day` : "As you use the mouse"} />
          <StatTile
            label="Fastest"
            value={w && w.topMps > 0 ? `${w.topMps.toFixed(1)} m/s` : unknown}
            detail={w?.topDate ? `${dayName(w.topDate)}${w.topGame ? `, in ${name(w.topGame)}` : ""}` : "Your quickest flick"}
          />
          <StatTile label="Clicks" value={w && w.clicks > 0 ? fmt(w.clicks) : unknown} detail={w && w.clicks > 0 ? `${fmt(w.buttons[busiest])} with ${BUTTONS[busiest]}` : "As you use the mouse"} />
        </div>
      </div>

      <Group>
        <SettingCard
          stacked
          icon={<CalendarLtr20Regular />}
          title={
            <span className="flex items-center gap-2">
              Each day {news.isNew && <NewBadge ref={news.ref} />}
            </span>
          }
          description={empty ? "Fills in as you use the mouse. Nothing to set up." : (w && versus(w.cm, w.prevCm)) ?? "How far the mouse moved each day."}
          below={w && <Days w={w} />}
        />
      </Group>

      {w && w.games.length > 0 && (
        <Group title="In your games">
          <SettingCard
            stacked
            icon={<Games20Regular />}
            title="Games"
            description="Full turns where hrtz reads the game's sensitivity."
            below={
              <div className="tw pl-9">
                <CardList>
                  {w.games.map((g) => (
                    <CardListRow key={g.exe}>
                      <Body1 className="min-w-0 flex-1 truncate">{name(g.exe)}</Body1>
                      <span className="text-sm tabular-nums text-muted-foreground">
                        {[distance(g.cm), timeText(g.movingS), g.turns >= 1 && `${fmt(g.turns)} full turns`, g.topMps > 0 && `fastest ${g.topMps.toFixed(1)} m/s`].filter(Boolean).join(" · ")}
                      </span>
                    </CardListRow>
                  ))}
                </CardList>
              </div>
            }
          />
        </Group>
      )}

      <Group>
        <SettingCard
          icon={<ShieldLock20Regular />}
          title="Count while you use the PC"
          description="Totals only, kept on this PC: no positions, and no app named but your games."
        >
          {w && !empty && (
            <Link as="button" onClick={copy}>
              Copy summary
            </Link>
          )}
          <Switch checked={counting} disabled={!prefs} aria-label="Count while you use the PC" onChange={(_, d) => prefs && savePrefs({ ...prefs, activity: d.checked })} />
        </SettingCard>
      </Group>
    </>
  );
}

/** A bar for each day of the week, today last. */
function Days({ w }: { w: Week }) {
  const top = Math.max(1, ...w.days.map((d) => d.cm));
  return (
    <figure className="tw m-0 flex flex-col gap-1 pl-9" aria-label="Distance each day">
      <div className="grid grid-cols-7 items-end gap-2" style={{ height: 96 }}>
        {w.days.map((d, i) => (
          <div key={d.date} className="flex flex-col items-center justify-end gap-1" title={`${dayName(d.date)}: ${distance(d.cm)}`}>
            <span className="text-[11px] leading-3 tabular-nums text-muted-foreground">{d.cm > 0 ? distance(d.cm) : ""}</span>
            <div
              className="w-3/5 rounded-[3px]"
              style={{ height: d.cm > 0 ? 4 + (d.cm / top) * 64 : 0, background: "var(--colorNeutralForeground3)", opacity: i === 6 ? 0.8 : 0.45 }}
            />
          </div>
        ))}
      </div>
      <div className="grid grid-cols-7 gap-2 border-t pt-1">
        {w.days.map((d, i) => (
          <span key={d.date} className="text-center text-[11px] leading-3 text-muted-foreground">
            {i === 6 ? "Today" : dayName(d.date).slice(0, 3)}
          </span>
        ))}
      </div>
    </figure>
  );
}
