import { useState, type ReactElement } from "react";
import { Caption1, Input, Popover, PopoverSurface, PopoverTrigger, type PositioningShorthand } from "@fluentui/react-components";
import { Add16Regular, FolderOpen16Regular, Search16Regular } from "@fluentui/react-icons";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type AppRule, type InstalledGame, type PadGame, type RunningApp } from "@/api";
import { AppIcon } from "@/components";
import { FLYOUT_PADDING } from "@/kit";
import { toast } from "@/store";

const muted = { color: "var(--colorNeutralForeground3)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;

export type PickedApp = { exe: string; name?: string; path?: string; icon?: string | null };

/** A pickable app, from any of the sources. `detail` is the line under its name. */
type Entry = { exe: string; name: string; path: string; icon: string | null; detail: string };

// Installed games are looked up once a session (Steam, Epic and Riot don't change while the app
// is open often enough to matter), and icons once per exe.
let installed: Promise<InstalledGame[]> | null = null;
const iconCache: Record<string, string | null> = {};

/**
 * Pick an app for a rule: a flyout with a search field over the apps open now and your games
 * (installed through Steam, Epic or Riot, or played with hrtz running), each with its own icon,
 * "Browse for an app" to pick any exe from disk, and "Add name.exe" for a typed name. Apps that
 * already have a rule aren't listed. Enter takes the first match. The flyout fits the room it
 * has, to the left of its button and below the title bar; only its list scrolls.
 */
export function AppPicker(props: {
  rules: AppRule[];
  games: PadGame[];
  onPick: (app: PickedApp) => void;
  children: ReactElement;
  positioning?: PositioningShorthand;
  /** What to say when the exe browsed for is already listed. */
  takenNote?: (name: string) => string;
}) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const [running, setRunning] = useState<RunningApp[] | null>(null);
  const [games, setGames] = useState<InstalledGame[]>([]);
  const [icons, setIcons] = useState<Record<string, string | null>>(iconCache);

  const taken = new Set(props.rules.map((r) => r.exe.toLowerCase()));
  const query = q.trim().toLowerCase();
  const matches = (e: Entry) => !taken.has(e.exe.toLowerCase()) && (!query || e.name.toLowerCase().includes(query) || e.exe.toLowerCase().includes(query));

  const openNow: Entry[] = (running ?? []).map((a) => ({ exe: a.exe, name: a.name, path: a.path, icon: a.icon, detail: a.exe })).filter(matches);
  const listed = new Set(openNow.map((a) => a.exe.toLowerCase()));
  const yours: Entry[] = [
    ...games.map((g) => ({ exe: g.exe, name: g.name, path: g.path, icon: icons[g.path] ?? null, detail: `${g.source} · ${g.exe}` })),
    ...props.games.map((g) => ({ exe: g.exe, name: g.name, path: g.path, icon: icons[g.path] ?? null, detail: g.exe })),
  ]
    .filter((g) => matches(g) && !listed.has(g.exe.toLowerCase()) && (listed.add(g.exe.toLowerCase()), true))
    .sort((a, b) => a.name.localeCompare(b.name));
  const first = yours[0] ?? openNow[0];
  const typed = query ? (query.endsWith(".exe") ? q.trim() : `${q.trim()}.exe`) : "";
  const offerTyped = !!typed && !first && !taken.has(typed.toLowerCase());

  const loadIcons = (paths: string[]) => {
    for (const path of paths) {
      if (!path || path in iconCache) continue;
      iconCache[path] = null;
      api.appIcon(path).then(
        (icon) => ((iconCache[path] = icon), setIcons({ ...iconCache })),
        () => {},
      );
    }
  };
  const show = (d: boolean) => {
    setOpen(d);
    if (!d) return setQ("");
    api.runningApps().then(setRunning, () => setRunning([]));
    installed ??= api.installedGames().catch(() => []);
    installed.then((list) => (setGames(list), loadIcons(list.map((g) => g.path))));
    loadIcons(props.games.map((g) => g.path));
  };
  const pick = (a: PickedApp) => {
    props.onPick(a);
    setOpen(false);
    setQ("");
  };
  const browse = async () => {
    setOpen(false);
    const path = await openDialog({ title: "Pick an app", multiple: false, directory: false, filters: [{ name: "Apps", extensions: ["exe"] }] });
    if (typeof path !== "string") return;
    const app = await api.appInfo(path).catch(() => null);
    if (!app) return toast("That isn't an app. Choose an .exe file.", "error");
    if (taken.has(app.exe.toLowerCase())) return toast(props.takenNote?.(app.name) ?? `${app.name} already has a rule.`, "info");
    props.onPick(app);
  };

  const row = (a: Entry) => (
    <button key={a.exe} className="st-row" style={{ height: 44, gap: 12, flexShrink: 0 }} onClick={() => pick(a)}>
      <AppIcon src={a.icon} className="size-6 shrink-0 object-contain" />
      <span className="flex min-w-0 flex-col">
        <span style={truncate}>{a.name}</span>
        <Caption1 style={{ ...muted, ...truncate }}>{a.detail}</Caption1>
      </span>
    </button>
  );
  const heading = (text: string) => <Caption1 style={{ ...muted, padding: "6px 8px 2px", flexShrink: 0 }}>{text}</Caption1>;

  return (
    <Popover
      open={open}
      onOpenChange={(_, d) => show(d.open)}
      positioning={props.positioning ?? { position: "before", align: "top", offset: 8, autoSize: "height", overflowBoundaryPadding: FLYOUT_PADDING }}
      trapFocus
    >
      <PopoverTrigger disableButtonEnhancement>{props.children}</PopoverTrigger>
      {/* overflow-y is set so Fluent's auto-size doesn't scroll the whole surface: the list does. */}
      <PopoverSurface style={{ width: 360, padding: 8, display: "flex", flexDirection: "column", overflowY: "hidden" }}>
        <div className="tw flex min-h-0 flex-1 flex-col gap-2">
          <Input
            autoFocus
            contentBefore={<Search16Regular />}
            placeholder="Search apps and games, or type an exe name"
            aria-label="App"
            value={q}
            onChange={(_, d) => setQ(d.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              if (first) pick(first);
              else if (offerTyped) pick({ exe: typed });
            }}
          />
          <div className="flex max-h-72 min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto">
            {yours.length > 0 && heading("Your games")}
            {yours.map(row)}
            {openNow.length > 0 && heading("Open now")}
            {openNow.map(row)}
            {offerTyped && (
              <button className="st-row" style={{ height: 40, gap: 12, flexShrink: 0 }} onClick={() => pick({ exe: typed })}>
                <Add16Regular />
                <span style={truncate}>Add {typed}</span>
              </button>
            )}
            {running && !first && !typed && <Caption1 style={{ ...muted, padding: 8 }}>No apps found. Browse for the .exe or type its name.</Caption1>}
          </div>
          <div className="shrink-0 border-t pt-1">
            <button className="st-row w-full" style={{ height: 40, gap: 12 }} onClick={browse}>
              <FolderOpen16Regular />
              <span>Browse for an app…</span>
            </button>
          </div>
        </div>
      </PopoverSurface>
    </Popover>
  );
}
