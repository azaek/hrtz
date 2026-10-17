import { useEffect, useRef, useState } from "react";
import { api, type HudPrefs, type InstalledGame, type Prefs } from "@/api";
import type { PickedApp } from "@/cards/profiles/AppPicker";
import { savePrefs } from "@/store";

/** How long Undo stays offered after a removal. */
const UNDO_MS = 6000;

type Known = { name: string; exe: string; path?: string };

/** A removal that can still be undone, and which list it came off. */
type Undo = { text: string; list: "hidden" | "blocked"; prev: HudPrefs };

/**
 * The games the HUD stays off in, and the ones it couldn't show over. Prefs keep only lower-case
 * exe names for the first, so names, the exe's own spelling and icons come from what hrtz already
 * knows about each exe (launcher installs, the pad map's games, profile rules), or from the pick
 * itself this session. Adding and removing save at once; a removal offers Undo. A game taken off
 * the can't-show list comes back on its own if it still runs in exclusive fullscreen.
 */
export function useHiddenGames(prefs: Prefs, hud: HudPrefs) {
  const [installed, setInstalled] = useState<InstalledGame[]>([]);
  const [picked, setPicked] = useState<Record<string, Known>>({});
  const [icons, setIcons] = useState<Record<string, string | null>>({});
  const [undo, setUndo] = useState<Undo | null>(null);
  const timer = useRef<number | null>(null);
  useEffect(() => () => clearTimeout(timer.current ?? 0), []);
  useEffect(() => {
    api.installedGames().then(setInstalled, () => {});
  }, []);

  const known: Record<string, Known> = {};
  for (const r of prefs.rules) known[r.exe.toLowerCase()] = { name: r.name ?? r.exe, exe: r.exe, path: r.path };
  for (const g of prefs.padGames) known[g.exe.toLowerCase()] = { name: g.name, exe: g.exe, path: g.path };
  for (const g of installed) known[g.exe.toLowerCase()] = { name: g.name, exe: g.exe, path: g.path };
  Object.assign(known, picked);

  const info = (exe: string): Known => known[exe.toLowerCase()] ?? { name: exe, exe };
  // `id` is the name as stored, `exe` the way the game spells it.
  const games = hud.hiddenGames.map((id) => ({ ...info(id), id }));
  // Icons for these and for the games the HUD can't show over.
  const paths = [...hud.hiddenGames, ...hud.blocked.map((b) => b.exe)].map((exe) => info(exe).path).filter((p): p is string => !!p);
  useEffect(() => {
    for (const path of paths) {
      if (path in icons) continue;
      api.appIcon(path).then(
        (icon) => setIcons((m) => ({ ...m, [path]: icon })),
        () => setIcons((m) => ({ ...m, [path]: null })),
      );
    }
  }, [paths.join("|")]);

  const save = (patch: Partial<HudPrefs>) => savePrefs({ ...prefs, hud: { ...hud, ...patch } });
  const offerUndo = (next: Undo) => {
    setUndo(next);
    clearTimeout(timer.current ?? 0);
    timer.current = window.setTimeout(() => setUndo(null), UNDO_MS);
  };
  const add = (app: PickedApp) => {
    const exe = app.exe.toLowerCase();
    if (app.name || app.path) setPicked((m) => ({ ...m, [exe]: { name: app.name ?? app.exe, exe: app.exe, path: app.path } }));
    if (app.path && app.icon !== undefined) setIcons((m) => ({ ...m, [app.path!]: app.icon ?? null }));
    if (!hud.hiddenGames.includes(exe)) save({ hiddenGames: [...hud.hiddenGames, exe] });
  };
  const remove = (id: string) => {
    offerUndo({ text: `Removed ${info(id).name}`, list: "hidden", prev: hud });
    save({ hiddenGames: hud.hiddenGames.filter((x) => x !== id) });
  };
  const unblock = (exe: string) => {
    const b = hud.blocked.find((x) => x.exe === exe);
    offerUndo({ text: `Removed ${b?.name || info(exe).name}`, list: "blocked", prev: hud });
    save({ blocked: hud.blocked.filter((x) => x.exe !== exe) });
  };
  // Only the list the removal came off goes back, so a change made since to anything else stays.
  const revert = () => {
    if (!undo) return;
    setUndo(null);
    save(undo.list === "hidden" ? { hiddenGames: undo.prev.hiddenGames } : { blocked: undo.prev.blocked });
  };
  const iconOf = (exe: string) => {
    const path = info(exe).path;
    return path ? icons[path] : null;
  };
  return { games, add, remove, unblock, undo, revert, iconOf };
}
