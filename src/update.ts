import { useEffect, useRef, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { api, on, type UpdateDownloadEvent } from "./api";
import { t } from "./i18n";
import { toast } from "./store";

/** Where hrtz lives. */
export const REPO_URL = "https://github.com/azaek/hrtz";

export type UpdateState =
  | { status: "idle" | "checking" | "current" }
  | { status: "failed"; reason: string }
  /** A newer version downloading in the background; `total` is null until the size is known. */
  | { status: "downloading"; version: string; received: number; total: number | null }
  /** Downloaded, waiting for the restart that installs it. */
  | { status: "ready"; version: string }
  | { status: "installing"; version: string };

/** A newer version found, and how to download and install it. */
type Found = {
  version: string;
  download: (onEvent: (e: UpdateDownloadEvent) => void) => Promise<void>;
  install: () => Promise<void>;
};

/**
 * The installed copy updates through Tauri's updater: the signed NSIS installer from `latest.json`,
 * which exits hrtz, installs and restarts it. The portable copy has its own signed manifest and is
 * replaced in place by hrtz itself (`portable.rs`), which then restarts into the new version.
 */
async function findUpdate(): Promise<Found | null> {
  if (await api.isPortable()) {
    const version = await api.portableCheck();
    return version ? { version, download: api.portableDownload, install: api.portableInstall } : null;
  }
  const found = await check();
  if (!found) return null;
  return {
    version: found.version,
    download: (onEvent) => found.download(onEvent),
    install: async () => {
      // On Windows the installer exits hrtz, past Quit's saving.
      await api.prepareUpdate().catch(() => {});
      await found.install();
      await relaunch();
    },
  };
}

/** Checked quietly a little after launch and every 6 hours after; a newer version downloads by itself. */
const FIRST_MS = 5_000;
const EVERY_MS = 6 * 60 * 60_000;
/** How long "Up to date" or "Couldn't check" stays after a check you asked for. */
const SAY_MS = 4_000;

const reasonOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * The self-update, as HEAD does it: a signed manifest on the newest GitHub release, read in Rust (so
 * the page needs no network access), a newer version downloaded in the background, then a click to
 * install it and restart. A check you didn't ask for stays quiet when it fails; one you asked for
 * says so.
 */
export function useUpdate() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });
  const update = useRef<Found | null>(null);
  const busy = useRef(false);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);

  const run = async (asked: boolean) => {
    // One check at a time, and none once a version is downloading or downloaded.
    if (busy.current || update.current) return;
    busy.current = true;
    clearTimeout(settle.current);
    if (asked) setState({ status: "checking" });
    try {
      const found = await findUpdate();
      if (!found) {
        if (asked) say({ status: "current" });
        return;
      }
      update.current = found;
      let received = 0;
      let total: number | null = null;
      setState({ status: "downloading", version: found.version, received, total });
      await found.download((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? null;
        if (e.event === "Progress") received += e.data.chunkLength;
        setState({ status: "downloading", version: found.version, received, total });
      });
      setState({ status: "ready", version: found.version });
      // For someone who keeps hrtz in the tray: a notification, and the item in the tray menu.
      void api.updateReady(found.version).catch(() => {});
    } catch (e) {
      update.current = null;
      if (asked) say({ status: "failed", reason: reasonOf(e) });
      else setState({ status: "idle" });
    } finally {
      busy.current = false;
    }
  };

  /** Shows a result for a few seconds, then the button goes back to "Check for updates". */
  const say = (s: UpdateState) => {
    setState(s);
    settle.current = setTimeout(() => setState({ status: "idle" }), SAY_MS);
  };

  useEffect(() => {
    // A dev build would download each release and offer to install it over itself: only a click
    // checks there.
    if (import.meta.env.DEV) return;
    const first = setTimeout(() => void run(false), FIRST_MS);
    const every = setInterval(() => void run(false), EVERY_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
      clearTimeout(settle.current);
    };
  }, []);

  /** Installs the downloaded version and restarts into it. */
  const install = async () => {
    const found = update.current;
    if (!found) return;
    setState({ status: "installing", version: found.version });
    try {
      await found.install();
    } catch (e) {
      // A click deserves an answer: say it failed, and keep the download for another try.
      setState({ status: "ready", version: found.version });
      throw new Error(t("Couldn't install {{version}}: {{reason}}", { version: found.version, reason: reasonOf(e) }));
    }
  };

  // The tray's "Restart to update" lands here, as the sidebar's button does.
  useEffect(() => {
    const off = on.updateInstall(() => void install().catch((e: Error) => toast(e.message, "error")));
    return () => void off.then((f) => f());
  }, []);

  return { state, check: () => run(true), install };
}
