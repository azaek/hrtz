import { Body1, Button, Caption1, Switch } from "@fluentui/react-components";
import {
  Add16Regular,
  ArrowMove20Regular,
  CircleHalfFill20Regular,
  Color20Regular,
  Delete16Regular,
  Desktop20Regular,
  Eye20Regular,
  EyeOff20Regular,
  FullScreenMaximize20Regular,
  PictureInPicture20Regular,
  TextBulletList20Regular,
  TextFontSize20Regular,
} from "@fluentui/react-icons";
import type { HudPrefs, HudShow, Prefs } from "@/api";
import { AppIcon, Choice, Group, SettingCard } from "@/components";
import { AppPicker } from "@/cards/profiles/AppPicker";
import { AnchorPicker, CardList, CardListRow, CardNote, IconButton, NumberStepper, UndoNote } from "@/kit";
import { savePrefs, useStore } from "@/store";
import { useHiddenGames } from "./useHiddenGames";
import { useHudPreview } from "./useHudPreview";

/** The contract's defaults, for a prefs file from before the HUD (the backend fills these in too). */
const DEFAULT_HUD: HudPrefs = {
  show: "change",
  fields: { battery: true, timeLeft: true, polling: true, dpi: true, profile: false, labels: true },
  anchor: "top-right",
  size: "medium",
  plate: 90,
  valueColor: "teal",
  oledCare: true,
  hiddenGames: [],
  blocked: [],
};

/** Valorant's stat colour, the HUD's default value colour. */
const TEAL = "#62e3e8";

const muted = { color: "var(--colorNeutralForeground3)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;

const FIELDS: { key: keyof HudPrefs["fields"]; title: string; text: string }[] = [
  { key: "battery", title: "Battery", text: "Green while charging, amber when low." },
  { key: "timeLeft", title: "Time left", text: "Shown in the battery tab once estimated." },
  { key: "polling", title: "Polling rate", text: "The current rate, and when battery saver lowers it." },
  { key: "dpi", title: "DPI", text: "With the stage number when there are several." },
  { key: "profile", title: "Profile", text: "The active profile's name." },
  { key: "labels", title: "Labels", text: "Names before each value. Without them: 82%, 8K Hz, 800 DPI." },
];

/** Why a game is on the can't-show list, in plain words. Past tense: the list keeps a game until
 *  it's removed, and by then its setting may have changed. */
const BLOCKED_WHY: Record<string, string> = {
  exclusive: "Reported as exclusive fullscreen. If the HUD doesn't appear, switch the game to borderless or windowed.",
};

/**
 * The in-game HUD: the preview (the real HUD on screen while this page is open), then when it
 * shows and what, how it looks, and the games it can't show over or stays off in. The backend
 * decides what shows when; this page edits prefs.hud and asks for the preview. Decided in the
 * HUD lab (DECISIONS.md, "In-game HUD").
 */
export function HudPanel({ prefs }: { prefs: Prefs }) {
  const hud = prefs.hud ?? DEFAULT_HUD;
  const device = useStore((x) => x.device);
  const showing = useStore((x) => x.status.hudShowing);
  const preview = useHudPreview(hud.show);
  const hidden = useHiddenGames(prefs, hud);

  const set = (patch: Partial<HudPrefs>) => savePrefs({ ...prefs, hud: { ...hud, ...patch } });
  const setField = (key: keyof HudPrefs["fields"], on: boolean) => set({ fields: { ...hud.fields, [key]: on } });

  const showText: Record<HudShow, string> = {
    off: "Never shows over games.",
    change: "Shows a value for 3 s when it changes.",
    always: hud.oledCare ? "Stays on screen during games. OLED care dims it when idle." : "Stays on screen during games.",
  };
  // Without a mouse connected the preview shows sample values.
  const sample = !device?.connected;
  const chosen = hud.fields.battery || hud.fields.polling || hud.fields.dpi || hud.fields.profile;
  const previewText =
    hud.show === "off"
      ? "Set Show to On change or Always to preview."
      : !preview.wanted
        ? "Shows the HUD on your screen while this page is open."
        : !chosen
          ? "Turn on at least one value below."
          : preview.on && showing
            ? sample
              ? "Previewing now, with sample values until a mouse connects."
              : "Previewing now. Press the DPI button to see it update."
            : "Shows the HUD on your screen while this page is open.";
  const fieldsNote =
    hud.show === "off"
      ? "The HUD is off."
      : `${hud.fields.battery ? "Low battery stays on screen. " : ""}All values show for 3 s when a game comes into focus. A yellow dot on the hrtz mark means the mouse is asleep or out of range.`;
  // Undo sits under the list the removal came off, or under the card's text once it's empty.
  const undoFor = (list: "hidden" | "blocked", empty: boolean) => {
    if (hidden.undo?.list !== list) return undefined;
    const undo = <UndoNote onUndo={hidden.revert}>{hidden.undo.text}</UndoNote>;
    return empty ? <div className="tw pl-9 text-xs">{undo}</div> : undo;
  };

  return (
    <>
      <Group>
        {/* The real HUD on screen while this page is open: it starts with the page and stops
            when the page or the window goes away. The switch is for someone who doesn't want
            it, and is remembered. */}
        <SettingCard icon={<PictureInPicture20Regular />} title="Preview" description={previewText}>
          <Switch checked={preview.wanted} disabled={hud.show === "off"} onChange={(_, d) => preview.set(d.checked)} aria-label="Preview" />
        </SettingCard>
        {/* The choice goes under the text, so each mode's line has the card's width. */}
        <SettingCard
          stacked
          icon={<Eye20Regular />}
          title="Show"
          description={showText[hud.show]}
          below={
            <div className="tw pl-9">
              <Choice
                label="Show"
                value={hud.show}
                options={[
                  { value: "off", label: "Off" },
                  { value: "change", label: "On change" },
                  { value: "always", label: "Always" },
                ]}
                onChange={(show) => set({ show })}
              />
            </div>
          }
        />
        <SettingCard
          stacked
          icon={<TextBulletList20Regular />}
          title="What to show"
          description="Each value has its own tab, which keeps its position."
          below={
            <CardList
              footer={
                // In line with the rows' text, which starts under the card's title.
                <div className="pl-9">
                  <CardNote hint={fieldsNote} />
                </div>
              }
            >
              {FIELDS.map((f) => {
                const off = f.key === "timeLeft" && !hud.fields.battery;
                return (
                  <CardListRow key={f.key} className="pl-13">
                    <div className="flex min-w-0 flex-1 flex-col">
                      <Body1 style={off ? { color: "var(--colorNeutralForegroundDisabled)" } : undefined}>{f.title}</Body1>
                      <Caption1 style={{ ...muted, ...truncate }}>{off ? "Needs Battery on." : f.text}</Caption1>
                    </div>
                    <Switch checked={hud.fields[f.key]} disabled={off} onChange={(_, d) => setField(f.key, d.checked)} aria-label={f.title} />
                  </CardListRow>
                );
              })}
            </CardList>
          }
        />
      </Group>

      <Group title="Look">
        <SettingCard icon={<ArrowMove20Regular />} title="Position" description="The screen edge or corner the HUD sits in.">
          <AnchorPicker label="Position" value={hud.anchor} onChange={(anchor) => set({ anchor })} />
        </SettingCard>
        <SettingCard icon={<TextFontSize20Regular />} title="Size" description="The size of each tab.">
          <Choice
            label="Size"
            value={hud.size}
            options={[
              { value: "small", label: "Small" },
              { value: "medium", label: "Medium" },
              { value: "large", label: "Large" },
            ]}
            onChange={(size) => set({ size })}
          />
        </SettingCard>
        <SettingCard icon={<Color20Regular />} title="Value colour" description="Teal matches Valorant's own stats.">
          <Choice
            label="Value colour"
            value={hud.valueColor}
            options={[
              { value: "teal", label: "Teal", swatch: TEAL },
              { value: "white", label: "White", swatch: "#ffffff" },
            ]}
            onChange={(valueColor) => set({ valueColor })}
          />
        </SettingCard>
        <SettingCard icon={<CircleHalfFill20Regular />} title="Background" description="How dark the tabs are. Below 30%, text is outlined instead.">
          <NumberStepper label="Tab darkness" unit="%" min={0} max={100} step={10} value={hud.plate} onChange={(plate) => set({ plate })} />
        </SettingCard>
        <SettingCard
          icon={<Desktop20Regular />}
          title="OLED care"
          description={hud.show === "always" ? "Dims the HUD when idle and shifts it slightly to prevent burn-in." : "Applies when Show is set to Always."}
        >
          <Switch checked={hud.oledCare} onChange={(_, d) => set({ oledCare: d.checked })} aria-label="OLED care" />
        </SettingCard>
      </Group>

      <Group title="In games">
        {/* Written by the backend when Windows reports a game in front as exclusive fullscreen. The
            HUD still tries to show there (the report also fires for games Windows runs borderless
            behind the scenes), so this says "may", and the user takes a game off once it's fine. */}
        {(hud.blocked.length > 0 || hidden.undo?.list === "blocked") && (
          <SettingCard
            stacked
            icon={<FullScreenMaximize20Regular />}
            title="May not show in these games"
            description={hud.blocked.length ? "Exclusive fullscreen blocks overlays. Remove a game once the HUD shows in it." : "None left."}
            below={
              hud.blocked.length > 0 ? (
                <CardList footer={undoFor("blocked", false)}>
                  {hud.blocked.map((b) => (
                    <CardListRow key={b.exe} className="items-start">
                      <AppIcon src={hidden.iconOf(b.exe)} className="mt-0.5 size-6 shrink-0 object-contain" />
                      <div className="flex min-w-0 flex-1 flex-col">
                        <Body1 style={truncate}>{b.name || b.exe}</Body1>
                        <Caption1 style={muted}>{BLOCKED_WHY[b.reason] ?? BLOCKED_WHY.exclusive}</Caption1>
                      </div>
                      {/* Raised so its middle lines up with the name, not the two lines under it. */}
                      <IconButton
                        appearance="subtle"
                        icon={<Delete16Regular />}
                        label={`Remove ${b.name || b.exe}`}
                        style={{ marginTop: -6 }}
                        onClick={() => hidden.unblock(b.exe)}
                      />
                    </CardListRow>
                  ))}
                </CardList>
              ) : (
                undoFor("blocked", true)
              )
            }
          />
        )}

        <SettingCard
          stacked
          icon={<EyeOff20Regular />}
          title="Hidden in these games"
          description={hidden.games.length ? "The HUD never shows over these." : "None yet. Add a game to hide the HUD in it."}
          below={
            hidden.games.length > 0 ? (
              <CardList footer={undoFor("hidden", false)}>
                {hidden.games.map((g) => (
                  <CardListRow key={g.id}>
                    <AppIcon src={hidden.iconOf(g.id)} className="size-6 shrink-0 object-contain" />
                    <div className="flex min-w-0 flex-1 flex-col">
                      <Body1 style={truncate}>{g.name}</Body1>
                      {g.name.toLowerCase() !== g.id && <Caption1 style={{ ...muted, ...truncate }}>{g.exe}</Caption1>}
                    </div>
                    <IconButton appearance="subtle" icon={<Delete16Regular />} label={`Remove ${g.name}`} onClick={() => hidden.remove(g.id)} />
                  </CardListRow>
                ))}
              </CardList>
            ) : (
              undoFor("hidden", true)
            )
          }
        >
          <AppPicker
            rules={hud.hiddenGames.map((exe) => ({ exe, profile: 0 }))}
            games={prefs.padGames}
            takenNote={(name) => `${name} is on the list already.`}
            onPick={hidden.add}
          >
            <Button icon={<Add16Regular />}>Add game</Button>
          </AppPicker>
        </SettingCard>
      </Group>

      <Caption1 style={{ ...muted, padding: "8px 2px 0" }}>
        hrtz draws the HUD in its own window and never injects into games. It shows over windowed, borderless and most fullscreen games.
      </Caption1>
    </>
  );
}
