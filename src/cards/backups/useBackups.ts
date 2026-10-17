import { useEffect, useRef, useState } from "react";
import { api, on, type BackupMeta } from "@/api";
import { run } from "@/store";

const UNDO_MS = 6000;

/**
 * The backups on this PC, newest first, kept fresh when hrtz saves one by itself. Removing takes
 * a backup off the list at once and deletes it for good once its Undo has had 6 s; leaving the
 * page doesn't bring it back.
 */
export function useBackups() {
  const [list, setList] = useState<BackupMeta[] | null>(null);
  const [gone, setGone] = useState<BackupMeta | null>(null);
  const goneRef = useRef<BackupMeta | null>(null);
  const timer = useRef<number | undefined>(undefined);
  goneRef.current = gone;

  const refresh = () => api.listBackups().then(setList, () => setList([]));

  const finish = () => {
    window.clearTimeout(timer.current);
    const g = goneRef.current;
    goneRef.current = null;
    setGone(null);
    if (g) void api.deleteBackup(g.id).then(refresh, refresh);
  };

  useEffect(() => {
    refresh();
    const off = on.backups(refresh);
    return () => {
      void off.then((f) => f());
      window.clearTimeout(timer.current);
      if (goneRef.current) void api.deleteBackup(goneRef.current.id);
    };
  }, []);

  return {
    list: list?.filter((b) => b.id !== gone?.id) ?? null,
    refresh,
    /** The backup just removed, while its Undo lasts. */
    gone,
    remove: (b: BackupMeta) => {
      if (goneRef.current) finish();
      goneRef.current = b;
      setGone(b);
      timer.current = window.setTimeout(finish, UNDO_MS);
    },
    undo: () => {
      window.clearTimeout(timer.current);
      goneRef.current = null;
      setGone(null);
    },
    /** Saves the mouse now with no name yet; the page opens its name field. */
    backUpNow: async () => {
      const m = await run(() => api.createBackup(""));
      await refresh();
      return m;
    },
    rename: async (b: BackupMeta, note: string) => {
      await run(() => api.renameBackup(b.id, note));
      await refresh();
    },
  };
}
