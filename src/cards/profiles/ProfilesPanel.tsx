import { useState } from "react";
import { Body1, Button, Caption1, Switch } from "@fluentui/react-components";
import { Add16Regular, Apps20Regular, ArrowRight16Regular, Delete16Regular, Rename16Regular, Stack20Regular } from "@fluentui/react-icons";
import { api, type AppRule, type KnownMouse, type MousePrefs, type Prefs } from "@/api";
import { AppIcon, Group, SettingCard } from "@/components";
import { fmtDpi } from "@/dpi";
import { AccentBadge, afterMenu, CardNote, MoreMenu, Pane, PaneFooter, PaneHeader, UndoNote, type MoreMenuItem } from "@/kit";
import { hzText } from "@/rates";
import { run, saveMousePrefs, useStore } from "@/store";
import { AppPicker } from "./AppPicker";
import { NameField } from "./NameField";
import { summaryOf, summaryText, UNSEEN } from "./summary";
import { appName, useRules } from "./useRules";

const muted = { color: "var(--colorNeutralForeground3)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;

/**
 * The Profiles page: "Switch profiles by app" on top, then the four profiles listed on the left
 * and the one you picked open on the right, with what it holds, the apps that switch to it, and
 * whether it catches every other app. "Use this profile" is the one primary button. Decided in the
 * Profiles lab (DECISIONS.md, "Profiles page").
 */
export function ProfilesPanel({ prefs, mouse }: { prefs: Prefs; mouse: KnownMouse }) {
  // The mouse's own: its profiles' names, the apps that switch them.
  const m = mouse.prefs;
  const device = useStore((x) => x.device!);
  const busy = useStore((x) => x.busy);
  const foreground = useStore((x) => x.status.foregroundExe);
  const r = useRules(mouse);
  const [sel, setSel] = useState(device.profile ?? 0);
  const [renaming, setRenaming] = useState(false);

  if (device.profile === null) {
    return (
      <Group>
        <SettingCard icon={<Stack20Regular />} title="Profiles" description="This mouse doesn't report onboard profiles." />
      </Group>
    );
  }

  const names = m.profileNames;
  const name = names[sel];
  const set = (patch: Partial<MousePrefs>) => saveMousePrefs(mouse.key, { ...m, ...patch });
  const rename = (v: string) => set({ profileNames: names.map((n, j) => (j === sel ? v : n)) });
  const summary = (i: number) => summaryOf(i, device.profile, device.settings, m.profileSnapshots, device.mid, device.capabilities);
  const held = summary(sel);
  const apps = m.rules.filter((x) => x.profile === sel);
  const isFocused = (x: AppRule) => !!foreground && x.exe.toLowerCase() === foreground.toLowerCase();
  const focused = foreground ? m.rules.find(isFocused) : undefined;
  const focusName = focused ? appName(focused) : foreground;
  const canUse = device.online && !busy;

  const appMenu = (x: AppRule): MoreMenuItem[] => [
    ...names.flatMap((n, i) => (i === x.profile ? [] : [{ label: `Move to ${n}`, icon: <ArrowRight16Regular />, onClick: () => r.move(x, i) }])),
    "divider",
    { label: "Remove", icon: <Delete16Regular />, onClick: () => r.remove(x) },
  ];

  return (
    <>
      <Group>
        <SettingCard
          icon={<Apps20Regular />}
          title="Switch profiles by app"
          description={
            m.rulesEnabled
              ? focusName
                ? `Switches profile when an app comes into focus. Now in focus: ${focusName}.`
                : "Switches profile when an app comes into focus."
              : "Profiles change only when you switch them."
          }
        >
          <Switch checked={m.rulesEnabled} onChange={(_, d) => set({ rulesEnabled: d.checked })} aria-label="Switch profiles by app" />
        </SettingCard>
      </Group>

      <div className="tw @container/profiles">
        <div className="grid items-start gap-2 @min-[700px]/profiles:grid-cols-[264px_minmax(0,1fr)]">
          {/* The profiles. */}
          <Pane sticky>
            <PaneHeader title="Profiles on the mouse" description="Each keeps its own settings." />
            <div role="list" aria-label="Profiles" className="flex flex-col gap-0.5 px-1 pb-1">
              {names.map((n, i) => (
                <button
                  key={i}
                  role="listitem"
                  className="st-row"
                  data-selected={sel === i}
                  aria-current={sel === i ? "true" : undefined}
                  style={{ height: 52, gap: 12 }}
                  onClick={() => (setSel(i), setRenaming(false))}
                >
                  <span className="flex min-w-0 flex-1 flex-col">
                    <Body1 style={truncate}>{n}</Body1>
                    <Caption1 style={{ ...muted, ...truncate }}>{device.profile === i ? `In use · ${summaryText(summary(i))}` : summaryText(summary(i))}</Caption1>
                  </span>
                  {device.profile === i && <span aria-hidden className="size-2 shrink-0 rounded-full bg-brand" />}
                </button>
              ))}
            </div>
          </Pane>

          {/* The one picked. */}
          <Pane aria-label={name}>
            <PaneHeader
              strong
              title={renaming ? <NameField value={name} index={sel} onSave={rename} onDone={() => setRenaming(false)} /> : <span style={truncate}>{name}</span>}
              description={`Profile ${sel + 1} on the mouse`}
              actions={
                <div className="flex shrink-0 items-center gap-1">
                  {device.profile === sel ? (
                    <AccentBadge>In use</AccentBadge>
                  ) : (
                    <Button appearance="primary" disabled={!canUse} onClick={() => run(() => api.setProfile(sel))}>
                      Use this profile
                    </Button>
                  )}
                  <MoreMenu label={`${name} actions`} items={[{ label: "Rename", icon: <Rename16Regular />, onClick: () => afterMenu(() => setRenaming(true)) }]} />
                </div>
              }
            />

            <section className="flex flex-col gap-2 px-4 pt-2 pb-4">
              <Body1 style={{ fontWeight: 600 }}>Settings</Body1>
              {held ? (
                <dl className="grid grid-cols-[112px_1fr] gap-y-1.5 text-sm tabular-nums">
                  <dt className="text-muted-foreground">DPI</dt>
                  <dd>
                    {fmtDpi(held.dpi)} <span className="text-muted-foreground">({held.stages === 1 ? "one stage" : `${held.stages} stages`})</span>
                  </dd>
                  <dt className="text-muted-foreground">Polling rate</dt>
                  <dd>{hzText(held.hz)}</dd>
                  <dt className="text-muted-foreground">Buttons</dt>
                  <dd>{held.buttonsChanged ? `${held.buttonsChanged} changed` : "Unchanged"}</dd>
                </dl>
              ) : (
                <Caption1 style={muted}>{UNSEEN}</Caption1>
              )}
            </section>

            <section className="flex flex-col gap-1 border-t pt-3 pb-3">
              <div className="flex items-center gap-3 px-4 pb-1">
                <Body1 style={{ fontWeight: 600, flex: 1 }}>Apps that use this profile</Body1>
                <AppPicker rules={m.rules} games={prefs.padGames} onPick={(a) => r.add(a, sel)}>
                  <Button icon={<Add16Regular />}>Add app</Button>
                </AppPicker>
              </div>
              <ul className="flex flex-col">
                {apps.map((x) => (
                  <li key={x.exe} className="flex h-12 items-center gap-3 pr-2 pl-4 transition-colors hover:bg-muted/30">
                    <AppIcon src={r.iconOf(x)} className="size-6 shrink-0 object-contain" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <Body1 style={truncate}>{appName(x)}</Body1>
                      <Caption1 style={{ ...muted, ...truncate }}>{isFocused(x) ? `In focus now · ${x.exe}` : x.exe}</Caption1>
                    </span>
                    <MoreMenu label={`${appName(x)} actions`} items={appMenu(x)} />
                  </li>
                ))}
                {!apps.length && <li className="px-4 py-2 text-xs text-muted-foreground">None yet.</li>}
              </ul>
              <div className="flex items-center gap-3 px-4 pt-2">
                <div className="flex min-w-0 flex-1 flex-col">
                  <Body1>Use for other apps</Body1>
                  <Caption1 style={muted}>
                    {m.defaultProfile === null
                      ? "Apps without a rule keep the current profile."
                      : m.defaultProfile === sel
                        ? "Apps without a rule switch to this profile."
                        : `${names[m.defaultProfile]} is the default now.`}
                  </Caption1>
                </div>
                <Switch checked={m.defaultProfile === sel} onChange={(_, d) => set({ defaultProfile: d.checked ? sel : null })} aria-label={`Use ${name} for any other app`} />
              </div>
            </section>

            <PaneFooter>
              {r.undo ? (
                <UndoNote onUndo={r.revert}>{r.undo.text}</UndoNote>
              ) : (
                <CardNote note={device.online ? null : { text: "Wake the mouse to switch profiles." }} hint="You can also switch profiles from the tray icon." />
              )}
            </PaneFooter>
          </Pane>
        </div>
      </div>
    </>
  );
}
