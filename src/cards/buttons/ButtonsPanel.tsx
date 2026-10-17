import { useState } from "react";
import { Body1, Caption1 } from "@fluentui/react-components";
import { ArrowCounterclockwise16Regular } from "@fluentui/react-icons";
import type { Capabilities, Settings } from "@/api";
import { CardNote, MoreMenu, Pane, PaneFooter, PaneHeader, TileChoice, UndoNote } from "@/kit";
import type { ModelArt } from "@/models";
import { FunctionOptions } from "./FunctionOptions";
import { DPI_BUTTON, firstOf, FUNCTIONS, functionOf, shortLabel } from "./functions";
import { Chip, MouseArt } from "./MouseArt";
import { useButtons } from "./useButtons";

const muted = { color: "var(--colorNeutralForeground3)" };

/**
 * The Buttons page: the buttons listed under the mouse's picture on the left, and the one you
 * picked open on the right, its functions as tiles with their options beneath. Nothing pops up;
 * every change writes to the mouse at once, with Undo. Below 700px the editor drops under the
 * list. Decided in the Buttons lab (DECISIONS.md, "Buttons page").
 */
export function ButtonsPanel({ s, caps, art, online }: { s: Settings; caps: Capabilities; art: ModelArt | undefined; online: boolean }) {
  // Shown only for a mouse with buttons, so these are there.
  const physical = caps.buttons ?? [];
  const actions = s.buttons ?? [];
  const e = useButtons(s, physical);
  const [lit, setLit] = useState<number | null>(null);
  // Opens on the first button you can change (not the only left click).
  const [sel, setSelState] = useState(() => physical.find(([i]) => !e.onlyLeft(i))?.[0] ?? physical[0]?.[0] ?? 0);
  // Leaving a button that's waiting for keys leaves it as it was.
  const pick = (i: number) => (e.cancelWaiting(), setSelState(i));

  const n = physical.findIndex(([b]) => b === sel) + 1;
  const name = physical.find(([b]) => b === sel)?.[1] ?? "";
  const shown = e.shown(sel);
  const onlyLeft = e.onlyLeft(sel);
  const off = !online;

  const footer = e.undo ? (
    <UndoNote onUndo={e.revert}>{e.undo.text}</UndoNote>
  ) : (
    <CardNote
      note={e.note ?? (online ? null : { text: "The mouse is asleep. Move it to wake it." })}
      hint="Select a button in the list or on the picture. Changes save to the mouse instantly."
    />
  );

  return (
    <div className="tw @container/buttons">
      <div className="grid items-start gap-2 @min-[700px]/buttons:grid-cols-[264px_minmax(0,1fr)]">
        {/* The buttons. */}
        <Pane>
          <PaneHeader
            title="What each button does"
            description="Saved on the mouse."
            actions={
              <MoreMenu
                label="Buttons"
                items={[{ label: "Restore default buttons", icon: <ArrowCounterclockwise16Regular />, disabled: e.isDefault || off, onClick: e.restore }]}
              />
            }
          />
          <div className="flex items-center gap-2 px-1 pb-1 @min-[700px]/buttons:flex-col @min-[700px]/buttons:items-stretch">
            {art && (
              <div className="flex shrink-0 justify-center px-3 pb-2">
                <MouseArt art={art} buttons={physical} width={212} lit={lit ?? sel} onLight={setLit} onPick={pick} />
              </div>
            )}
            <div role="list" aria-label="Buttons" className="flex min-w-0 flex-1 flex-col gap-0.5">
              {physical.map(([i, nm], k) => (
                <button
                  key={i}
                  role="listitem"
                  className="st-row"
                  data-selected={sel === i}
                  aria-current={sel === i ? "true" : undefined}
                  style={{ height: 48, gap: 12 }}
                  onPointerEnter={() => setLit(i)}
                  onPointerLeave={() => setLit(null)}
                  onClick={() => pick(i)}
                >
                  <Chip n={k + 1} lit={lit === i} />
                  <span className="flex min-w-0 flex-col">
                    <Body1>{nm}</Body1>
                    <Caption1 className="truncate" style={muted}>
                      {i === DPI_BUTTON ? `Underneath · ${shortLabel(actions[i])}` : shortLabel(actions[i])}
                    </Caption1>
                  </span>
                </button>
              ))}
            </div>
          </div>
        </Pane>

        {/* The one picked. */}
        <Pane aria-label={`${name} button`}>
          <PaneHeader
            strong
            lead={<Chip n={n} lit />}
            title={name}
            description={onlyLeft ? "Your only left click, so it can't be reassigned." : shortLabel(actions[sel])}
          />
          <div className="px-4">
            <TileChoice
              label={`What ${name} does`}
              value={functionOf(shown)}
              disabled={off}
              options={FUNCTIONS.map((f) => ({ ...f, disabled: onlyLeft && f.value !== "mouse" }))}
              onChange={(fn) => e.set(sel, firstOf(fn))}
            />
          </div>
          <div className="flex flex-col items-start gap-4 px-4 pt-5 pb-4">
            <FunctionOptions
              key={sel}
              action={shown}
              maxDpi={caps.dpi?.max ?? 12800}
              disabled={off || onlyLeft}
              onChange={(a) => e.set(sel, a)}
              recordNow={e.waiting === sel}
              onLeaveKeys={e.cancelWaiting}
            />
          </div>
          <PaneFooter>{footer}</PaneFooter>
        </Pane>
      </div>
    </div>
  );
}
