import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

/** Where hrtz lives. */
export const REPO_URL = "https://github.com/azaek/hrtz";
const LATEST_RELEASE = "https://api.github.com/repos/azaek/hrtz/releases/latest";

export type Release = { version: string; url: string };

/** "v0.2.0" → [0, 2, 0]. */
const parts = (v: string) =>
  v
    .replace(/^v/i, "")
    .split(/[.+-]/)
    .slice(0, 3)
    .map((x) => Number.parseInt(x, 10) || 0);

/** Whether version `a` is newer than `b`. */
export function newer(a: string, b: string) {
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return (x[i] ?? 0) > (y[i] ?? 0);
  return false;
}

/**
 * The latest release on GitHub when it's newer than this copy, else null. No release yet (or the
 * repository still private) answers 404, which is up to date. Throws when GitHub can't be reached.
 * Until releases are signed for Tauri's updater, an update opens its release page.
 */
export async function checkForUpdate(): Promise<Release | null> {
  const current = await getVersion();
  if (!current) throw new Error("This copy doesn't know its version.");
  const res = await fetch(LATEST_RELEASE, { headers: { Accept: "application/vnd.github+json" } });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`GitHub answered ${res.status}.`);
  const r = (await res.json()) as { tag_name: string; html_url: string };
  return newer(r.tag_name, current) ? { version: r.tag_name.replace(/^v/i, ""), url: r.html_url } : null;
}

export type UpdateState =
  | { status: "idle" | "checking" | "current" }
  | { status: "failed"; reason: string }
  | { status: "available"; release: Release };

/** Checked quietly a little after launch and every 6 hours after; only a newer release shows. */
const FIRST_MS = 5_000;
const EVERY_MS = 6 * 60 * 60_000;
/** How long "Up to date" or "Couldn't check" stays after a check you asked for. */
const SAY_MS = 4_000;

/** The update check: quiet on its own, and saying how it went when you ask. */
export function useUpdate() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => {
    const quiet = () =>
      checkForUpdate().then(
        (release) => release && setState({ status: "available", release }),
        () => {},
      );
    const first = setTimeout(quiet, FIRST_MS);
    const every = setInterval(quiet, EVERY_MS);
    return () => {
      clearTimeout(first);
      clearInterval(every);
      clearTimeout(settle.current);
    };
  }, []);

  const check = async () => {
    clearTimeout(settle.current);
    setState({ status: "checking" });
    try {
      const release = await checkForUpdate();
      if (release) return setState({ status: "available", release });
      setState({ status: "current" });
    } catch (e) {
      setState({ status: "failed", reason: e instanceof Error ? e.message : String(e) });
    }
    settle.current = setTimeout(() => setState({ status: "idle" }), SAY_MS);
  };

  return { state, check };
}
