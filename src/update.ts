import { useEffect, useRef, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { api, on } from "./api";
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

/** One download event from the updater plugin, typed here so a rename there can't break the build. */
type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** Checked quietly a little after launch and every 6 hours after; a newer version downloads by itself. */
const FIRST_MS = 5_000;
const EVERY_MS = 6 * 60 * 60_000;
/** How long "Up to date" or "Couldn't check" stays after a check you asked for. */
const SAY_MS = 4_000;

const reasonOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * The self-update, as HEAD does it: Tauri's updater reads the signed `latest.json` on the newest
 * GitHub release (in Rust, so the page needs no network access), downloads a newer version in the
 * background, then waits for a click to install it and restart. A check you didn't ask for stays
 * quiet when it fails; one you asked for says so.
 */
export function useUpdate() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });
  const update = useRef<Update | null>(null);
  const busy = useRef(false);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);

  const run = async (asked: boolean) => {
    // One check at a time, and none once a version is downloading or downloaded.
    if (busy.current || update.current) return;
    busy.current = true;
    clearTimeout(settle.current);
    if (asked) setState({ status: "checking" });
    try {
      const found = await check();
      if (!found) {
        if (asked) say({ status: "current" });
        return;
      }
      update.current = found;
      let received = 0;
      let total: number | null = null;
      setState({ status: "downloading", version: found.version, received, total });
      await found.download((e: DownloadEvent) => {
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
      // On Windows the installer exits hrtz, past Quit's saving.
      await api.prepareUpdate().catch(() => {});
      await found.install();
      await relaunch();
    } catch (e) {
      // A click deserves an answer: say it failed, and keep the download for another try.
      setState({ status: "ready", version: found.version });
      throw new Error(`Couldn't install ${found.version}: ${reasonOf(e)}`);
    }
  };

  // The tray's "Restart to update" lands here, as the sidebar's button does.
  useEffect(() => {
    const off = on.updateInstall(() => void install().catch((e: Error) => toast(e.message, "error")));
    return () => void off.then((f) => f());
  }, []);

  return { state, check: () => run(true), install };
}
