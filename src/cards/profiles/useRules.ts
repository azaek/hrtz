import { useEffect, useRef, useState } from "react";
import { api, type AppRule, type KnownMouse } from "@/api";
import { saveMousePrefs } from "@/store";
import type { PickedApp } from "./AppPicker";

/** How long Undo stays offered after a removal. */
const UNDO_MS = 6000;

export const appName = (r: AppRule) => r.name ?? r.exe;

/**
 * The per-app rules and their icons. Adding, moving and removing save at once; a removal offers
 * Undo. Icons come from each rule's exe (the one Windows shows), so they show while the app is closed.
 */
export function useRules(mouse: KnownMouse) {
  const prefs = mouse.prefs;
  const [undo, setUndo] = useState<{ text: string; prev: AppRule[] } | null>(null);
  const [icons, setIcons] = useState<Record<string, string | null>>({});
  const timer = useRef<number | null>(null);
  useEffect(() => () => clearTimeout(timer.current ?? 0), []);

  useEffect(() => {
    for (const r of prefs.rules) {
      if (r.path && !(r.path in icons)) {
        const path = r.path;
        api.appIcon(path).then(
          (icon) => setIcons((m) => ({ ...m, [path]: icon })),
          () => setIcons((m) => ({ ...m, [path]: null })),
        );
      }
    }
  }, [prefs.rules]);

  const save = (rules: AppRule[]) => saveMousePrefs(mouse.key, { ...prefs, rules });
  const add = (app: PickedApp, profile: number) => {
    if (app.path && app.icon !== undefined) setIcons((m) => ({ ...m, [app.path!]: app.icon ?? null }));
    save([...prefs.rules.filter((r) => r.exe.toLowerCase() !== app.exe.toLowerCase()), { exe: app.exe, profile, name: app.name, path: app.path }]);
  };
  const move = (r: AppRule, profile: number) => save(prefs.rules.map((x) => (x.exe === r.exe ? { ...x, profile } : x)));
  const remove = (r: AppRule) => {
    setUndo({ text: `Removed ${appName(r)}`, prev: prefs.rules });
    clearTimeout(timer.current ?? 0);
    timer.current = window.setTimeout(() => setUndo(null), UNDO_MS);
    save(prefs.rules.filter((x) => x.exe !== r.exe));
  };
  const revert = () => {
    if (!undo) return;
    setUndo(null);
    save(undo.prev);
  };
  const iconOf = (r: AppRule) => (r.path ? icons[r.path] : null);
  return { add, move, remove, undo, revert, iconOf };
}
