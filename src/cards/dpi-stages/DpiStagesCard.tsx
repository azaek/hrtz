import { useEffect, useState } from "react";
import { Button as FluentButton } from "@fluentui/react-components";
import { Add16Regular, ArrowDown16Regular, ArrowUp16Regular, Delete16Regular, Gauge20Regular } from "@fluentui/react-icons";
import { api, type Capabilities, type Settings } from "@/api";
import { SettingCard } from "@/components";
import { Button } from "@/components/ui/button";
import { dpiRange, fmtDpi } from "@/dpi";
import { AccentBadge, CardList, CardListRow, CardNote, ColorMenu, HoverSwap, MoreMenu, UndoNote } from "@/kit";
import { useStore } from "@/store";
import { DpiField } from "./DpiField";
import { LED_PALETTE } from "./palette";
import { Tape } from "./Tape";
import { useStages } from "./useStages";

/** The mouse holds up to eight stages. */

/** The first of your listed games whose settings file gives its sensitivity, for eDPI. */
function useGameSens() {
  const games = useStore((x) => x.prefs?.padGames);
  const [found, setFound] = useState<{ game: string; sens: number } | null>(null);
  useEffect(() => {
    let live = true;
    (async () => {
      for (const g of games ?? []) {
        const s = await api.gameSensitivity(g.exe).catch(() => null);
        if (s && live) return setFound({ game: s.game, sens: s.sens });
      }
      if (live) setFound(null);
    })();
    return () => {
      live = false;
    };
  }, [games?.map((g) => g.exe).join("|")]);
  return found;
}

/**
 * DPI stages, one row each in the order the DPI button cycles them. Type a value, or point at a
 * row and drag its tape. The rows settle back into order when a value changes, if they were in
 * order. Decided in the DPI card lab (DECISIONS.md, "DPI stages card").
 */
/** Shown only for a mouse with DPI stages (`caps.dpi`). */
export function DpiStagesCard({ s, caps }: { s: Settings; caps: Capabilities }) {
  const limits = caps.dpi!;
  const range = dpiRange(limits);
  const e = useStages(s, range);
  const sens = useGameSens();
  const count = e.stages.length;

  const footer = e.removed ? (
    <UndoNote onUndo={e.undo}>
      Removed stage {e.removed.index + 1} ({fmtDpi(e.removed.stage.dpi)})
    </UndoNote>
  ) : (
    <CardNote note={e.note} hint="Drag a row's ruler or type a value. Hold Shift for fine steps." />
  );

  return (
    <SettingCard
      stacked
      icon={<Gauge20Regular />}
      title="DPI stages"
      description={
        caps.dpiButton
          ? count === 1
            ? "Add a stage to switch DPI with the DPI button."
            : "The DPI button cycles through these, top to bottom."
          : count === 1
            ? "Add a stage, then switch to it with Use."
            : "Switch between them with Use."
      }
      below={
        <CardList footer={footer}>
          {e.stages.map((x, i) => {
            const n = i + 1;
            return (
              <CardListRow key={x.id} onFocusCapture={() => e.setSelectedId(x.id)} onPointerDown={() => e.setSelectedId(x.id)}>
                <span className="w-4 text-center text-xs text-muted-foreground tabular-nums">{n}</span>
                {limits.colors && <ColorMenu color={x.color} label={`Stage ${n} light`} palette={LED_PALETTE} onChange={(hex) => e.setColour(x.id, hex)} />}
                <DpiField className="w-36" value={x.dpi} range={range} label={`Stage ${n} DPI`} taken={e.taken(x.id)} onChange={(v) => e.setValue(x.id, v)} onNote={e.setNote} />
                <HoverSwap
                  rest={sens ? `eDPI ${fmtDpi(Math.round(x.dpi * sens.sens))} in ${sens.game}` : null}
                  active={(onActive) => (
                    <Tape value={x.dpi} range={range} taken={e.taken(x.id)} label={`Stage ${n} DPI dial`} onDrag={(v) => e.drag(x.id, v)} onDragEnd={e.dragEnd} onActive={onActive} />
                  )}
                />
                <div className="ml-auto flex items-center gap-1">
                  {x.id === e.activeId ? (
                    <AccentBadge>In use</AccentBadge>
                  ) : (
                    <Button variant="ghost" className="text-muted-foreground" onClick={() => e.use(x.id)}>
                      Use
                    </Button>
                  )}
                  <MoreMenu
                    label={`Stage ${n} actions`}
                    items={[
                      { label: "Move up", icon: <ArrowUp16Regular />, disabled: i === 0, onClick: () => e.move(x.id, -1) },
                      { label: "Move down", icon: <ArrowDown16Regular />, disabled: i === count - 1, onClick: () => e.move(x.id, 1) },
                      "divider",
                      { label: "Remove", icon: <Delete16Regular />, disabled: count < 2, onClick: () => e.remove(x.id) },
                    ]}
                  />
                </div>
              </CardListRow>
            );
          })}
        </CardList>
      }
    >
      {count < limits.maxStages && (
        <FluentButton icon={<Add16Regular />} onClick={e.add}>
          Add stage
        </FluentButton>
      )}
    </SettingCard>
  );
}
