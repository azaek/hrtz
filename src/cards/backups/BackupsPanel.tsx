import { useState } from "react";
import { Body1, Button, Caption1, Input } from "@fluentui/react-components";
import {
  ArrowDownload16Regular,
  ArrowUpload16Regular,
  Bookmark16Regular,
  Delete16Regular,
  FolderOpen16Regular,
  History20Regular,
  Rename16Regular,
  Save16Regular,
  ShieldCheckmark20Regular,
} from "@fluentui/react-icons";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, type BackupMeta } from "@/api";
import { Group, SettingCard } from "@/components";
import { afterMenu, CardList, CardListRow, CardNote, MoreMenu, ProgressLine, UndoNote, type MoreMenuItem } from "@/kit";
import { run, useStore } from "@/store";
import { changes } from "./changes";
import { lineOf, nameOf, shortWhen } from "./names";
import { useBackups } from "./useBackups";

const muted = { color: "var(--colorNeutralForeground3)" };
const truncate = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } as const;

type Card = "yours" | "auto";
/** What a restore just did, said under the card it was started from; Undo when hrtz saved first. */
type Restored = { card: Card; text: string; saved: string | null };
/** A backup or restore running, shown at the top of the card it was started from. */
type Working = { card: Card; text: string };

/**
 * Backups, a group on the Settings page: yours, kept until you delete them, and the ones hrtz saves
 * by itself before anything rewrites the mouse (a restore, factory settings) and when it first
 * meets a mouse, of which it keeps the last ten. With a backup saved first, Restore writes at once
 * and offers Undo instead of asking. Decided in the Backups lab, moved into Settings on 2026-10-17
 * (DECISIONS.md, "Backups").
 */
export function BackupsPanel() {
  const device = useStore((x) => x.device);
  const busy = useStore((x) => x.busy);
  const b = useBackups();
  const [naming, setNaming] = useState<string | null>(null);
  const [restored, setRestored] = useState<Restored | null>(null);
  const [working, setWorking] = useState<Working | null>(null);
  const progress = useStore((x) => x.progress);
  /** Run `f`, its progress shown under `card` meanwhile. */
  const showing = async <T,>(card: Card, text: string, f: () => Promise<T>): Promise<T> => {
    setWorking({ card, text });
    try {
      return await f();
    } finally {
      setWorking(null);
    }
  };

  const online = !!device?.online;
  const can = online && !busy;
  const model = device?.connected ? (device.model ?? null) : null;
  const list = b.list;
  const yours = list?.filter((x) => !x.auto) ?? [];
  const auto = list?.filter((x) => x.auto) ?? [];

  const backUp = async () => {
    setRestored(null);
    const m = await showing("yours", "Backing up", b.backUpNow);
    if (m) setNaming(m.id);
  };

  const restore = async (m: BackupMeta, card: Card) => {
    setRestored(null);
    const before = device?.settings;
    const buttons = device?.capabilities?.buttons;
    const then = await api.backupSettings(m.id).catch(() => null);
    const name = nameOf(m);
    const r = await showing(card, `Restoring ${name}`, () => run(() => api.restoreBackup(m.id)));
    if (!r) return;
    await b.refresh();
    if (r.written === 0) return setRestored({ card, text: `The mouse already matches ${name}.`, saved: null });
    const c = before && then && buttons ? changes(before, then, buttons) : [];
    const what =
      c.length === 0 ? "Settings already matched. Macros and shortcuts were rewritten." : c.length === 1 ? `${c[0][0]}: ${c[0][1]} → ${c[0][2]}.` : `${c.length} settings changed.`;
    setRestored({ card, text: `Restored ${name}. ${what}`, saved: r.saved });
  };
  const undoRestore = async () => {
    const id = restored?.saved;
    const card = restored?.card ?? "yours";
    setRestored(null);
    if (id && (await showing(card, "Undoing the restore", () => run(() => api.undoRestore(id), "Restore undone"))) !== undefined) await b.refresh();
  };

  const exportOne = async (m: BackupMeta) => {
    const path = await saveDialog({ defaultPath: m.id, filters: [{ name: "Mouse backup", extensions: ["json"] }] });
    if (path) await run(() => api.exportBackup(m.id, path), "Backup exported");
  };
  const importOne = async () => {
    const path = await openDialog({ multiple: false, filters: [{ name: "Mouse backup", extensions: ["json"] }] });
    if (typeof path === "string" && (await run(() => api.importBackup(path), "Backup imported"))) await b.refresh();
  };
  const remove = (m: BackupMeta) => {
    setRestored(null);
    b.remove(m);
  };
  // Named, the backup a restore saved is yours now: Undo would put it back and drop it, so it goes.
  const rename = (m: BackupMeta, note: string) => {
    if (note && restored?.saved === m.id) setRestored(null);
    void b.rename(m, note);
  };

  const row = (m: BackupMeta, card: Card) => {
    const menu: MoreMenuItem[] = [
      m.auto
        ? { label: "Name and keep", icon: <Bookmark16Regular />, onClick: () => afterMenu(() => setNaming(m.id)) }
        : { label: "Rename", icon: <Rename16Regular />, onClick: () => afterMenu(() => setNaming(m.id)) },
      { label: "Export…", icon: <ArrowDownload16Regular />, onClick: () => exportOne(m) },
      "divider",
      { label: "Delete", icon: <Delete16Regular />, onClick: () => remove(m) },
    ];
    return (
      <CardListRow key={m.id} style={{ minHeight: 56 }}>
        <span className="flex min-w-0 flex-1 flex-col">
          {naming === m.id ? (
            <NameField b={m} onSave={(note) => rename(m, note)} onDone={() => setNaming(null)} />
          ) : (
            <Body1 style={truncate}>{nameOf(m)}</Body1>
          )}
          <Caption1 style={{ ...muted, ...truncate }}>{lineOf(m, model)}</Caption1>
        </span>
        <Button disabled={!can} onClick={() => restore(m, card)}>
          Restore
        </Button>
        <MoreMenu label={`${nameOf(m)} actions`} items={menu} />
      </CardListRow>
    );
  };

  // Right under the card's header, near the button that started it.
  const workingRow = working && (
    <CardListRow>
      <WorkingNote text={working.text} value={progress ? progress[0] / progress[1] : 0} />
    </CardListRow>
  );

  /** The line under a card: Undo for what was just done there, else a hint. */
  const footer = (card: Card, hint: string) => {
    if (b.gone && !!b.gone.auto === (card === "auto")) return <UndoNote onUndo={b.undo}>Deleted {nameOf(b.gone)}.</UndoNote>;
    if (restored?.card === card) return restored.saved ? <UndoNote onUndo={undoRestore}>{restored.text}</UndoNote> : <CardNote note={{ text: restored.text }} />;
    if (card === "yours" && !online) return <CardNote note={{ text: device?.connected ? "Wake the mouse to back up or restore." : "Connect the receiver to back up or restore." }} />;
    return <CardNote hint={hint} />;
  };

  return (
    <Group title="Backups">
      <SettingCard
        stacked
        icon={<History20Regular />}
        title="Your backups"
        description="Kept until you delete them."
        below={
          <CardList footer={footer("yours", "Restoring writes only what changed.")}>
            {working?.card === "yours" && workingRow}
            {yours.map((m) => row(m, "yours"))}
            {list && yours.length === 0 && (
              <CardListRow>
                <Caption1 style={muted}>No backups yet. Back up now to save your current setup.</Caption1>
              </CardListRow>
            )}
          </CardList>
        }
      >
        <MoreMenu
          label="More backup actions"
          items={[
            { label: "Import a backup…", icon: <ArrowUpload16Regular />, onClick: importOne },
            { label: "Open the backups folder", icon: <FolderOpen16Regular />, onClick: () => void api.openBackupsFolder() },
          ]}
        />
        <Button icon={<Save16Regular />} disabled={!can} onClick={backUp}>
          Back up now
        </Button>
      </SettingCard>
      <SettingCard
        stacked
        icon={<ShieldCheckmark20Regular />}
        title="Automatic backups"
        description="Saved before a restore or factory reset, and when a mouse first connects. The last 10 are kept."
        below={
          <CardList
            footer={auto.length || b.gone?.auto || restored?.card === "auto" ? footer("auto", "Name a backup to keep it permanently.") : undefined}
          >
            {working?.card === "auto" && workingRow}
            {auto.map((m) => row(m, "auto"))}
            {list && auto.length === 0 && (
              <CardListRow>
                <Caption1 style={muted}>None yet. One is saved before anything overwrites the mouse.</Caption1>
              </CardListRow>
            )}
          </CardList>
        }
      />
    </Group>
  );
}

/** What's running, and how far it's got. */
function WorkingNote({ text, value }: { text: string; value: number }) {
  return (
    <div role="status" className="flex flex-1 flex-col gap-2 py-1 text-xs text-muted-foreground">
      <span className="tabular-nums">
        {text} · {Math.round(value * 100)}%
      </span>
      <ProgressLine value={value} label={text} />
    </div>
  );
}

/**
 * A backup's name as a field: it starts selected, Enter or leaving it saves, Escape keeps what
 * was there. Left empty, the backup goes by its date (or why hrtz saved it); named, an automatic
 * backup becomes yours.
 */
function NameField({ b, onSave, onDone }: { b: BackupMeta; onSave: (name: string) => void; onDone: () => void }) {
  const save = (v: string) => {
    if (v.trim() !== b.note) onSave(v.trim());
    onDone();
  };
  return (
    <Input
      autoFocus
      defaultValue={b.note}
      placeholder={b.auto ? "Name to keep" : shortWhen(b.created)}
      aria-label="Backup name"
      style={{ width: "100%", maxWidth: 280 }}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={(e) => save(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") save(e.currentTarget.value);
        if (e.key === "Escape") onDone();
      }}
    />
  );
}
