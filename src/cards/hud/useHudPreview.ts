import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type HudShow } from "@/api";

/** Whether the page's preview is wanted. Remembered here rather than in the prefs: it's only about
 *  this page. */
const KEY = "hrtz.hudPreview";
/** The page asks again this often while the preview is on. The backend ends it 5 minutes after the
 *  last ask, so a page that's gone (or a window that went away without the page hearing) can't
 *  leave the HUD on screen. */
const KEEP_ALIVE_MS = 60_000;

/**
 * The HUD page's preview: the real HUD on screen while the page is open, unless it's switched off.
 * It stops when the page goes away and while the window is hidden or minimised, and comes back
 * with the window. It does nothing while Show is Off. The backend also gives way to a game in
 * front, and ends the preview when the window is hidden or the app quits.
 */
export function useHudPreview(show: HudShow) {
  const [wanted, setWanted] = useState(() => localStorage.getItem(KEY) !== "off");
  const [up, setUp] = useState(() => document.visibilityState === "visible");
  const on = wanted && up && show !== "off";

  // Can anyone see the page? WebView2 doesn't always tell it when the window is minimised or
  // hidden to the tray, so the window is asked too.
  useEffect(() => {
    const win = getCurrentWindow();
    const check = () =>
      Promise.all([win.isVisible(), win.isMinimized()]).then(
        ([visible, minimised]) => setUp(visible && !minimised && document.visibilityState === "visible"),
        () => {},
      );
    check();
    document.addEventListener("visibilitychange", check);
    const unlisten = [win.onResized(check), win.onFocusChanged(check)];
    return () => {
      document.removeEventListener("visibilitychange", check);
      unlisten.forEach((p) => p.then((off) => off(), () => {}));
    };
  }, []);

  useEffect(() => {
    if (!on) return;
    const ask = () => api.hudTest(true).catch(() => {});
    ask();
    const keepAlive = window.setInterval(ask, KEEP_ALIVE_MS);
    return () => {
      clearInterval(keepAlive);
      api.hudTest(false).catch(() => {});
    };
  }, [on]);

  const set = (v: boolean) => {
    setWanted(v);
    localStorage.setItem(KEY, v ? "on" : "off");
  };
  return { on, wanted, set };
}
