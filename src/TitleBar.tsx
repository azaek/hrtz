import { useEffect, useState } from "react";
import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { DocumentSearch16Regular } from "@fluentui/react-icons";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Wordmark } from "./brand";
import { setInspecting, useStore } from "./store";

const useStyles = makeStyles({
  // The window's own title bar, as tall as Windows draws one. Dragging anywhere but the buttons
  // moves the window, and a double-click maximizes it.
  bar: {
    display: "flex",
    alignItems: "stretch",
    height: "32px",
    flexShrink: 0,
    color: tokens.colorNeutralForeground1,
    userSelect: "none",
  },
  // Out of focus, Windows dims the title and the caption glyphs.
  inactive: { color: tokens.colorNeutralForegroundDisabled },
  brand: { alignSelf: "center", marginLeft: "16px" },
  caption: { display: "flex" },
  // Tools reachable from every screen, left of the caption buttons: a soft wash on hover, like them.
  tools: { display: "flex", alignItems: "stretch", marginLeft: "auto" },
  tool: {
    display: "flex",
    alignItems: "center",
    gap: "6px",
    padding: "0 12px",
    border: "none",
    backgroundColor: "transparent",
    color: tokens.colorNeutralForeground2,
    fontFamily: "inherit",
    fontSize: tokens.fontSizeBase200,
    transitionProperty: "background-color",
    transitionDuration: "100ms",
    transitionTimingFunction: "ease-out",
    ":hover": { backgroundColor: "var(--nav-hover)", color: tokens.colorNeutralForeground1 },
    ":active": { backgroundColor: "var(--nav-pressed)" },
  },
  // Windows 11 caption buttons: 46 x 32, Segoe Fluent Icons at 10 px, a soft wash on hover.
  button: {
    display: "grid",
    placeItems: "center",
    width: "46px",
    padding: 0,
    border: "none",
    backgroundColor: "transparent",
    color: "inherit",
    fontFamily: '"Segoe Fluent Icons", "Segoe MDL2 Assets"',
    fontSize: "10px",
    transitionProperty: "background-color, color",
    transitionDuration: "100ms",
    transitionTimingFunction: "ease-out",
    ":hover": { backgroundColor: "var(--nav-hover)" },
    ":active": { backgroundColor: "var(--nav-pressed)", color: tokens.colorNeutralForeground2 },
  },
  close: {
    ":hover": { backgroundColor: "#c42b1c", color: "#ffffff" },
    ":active": { backgroundColor: "rgba(196, 43, 28, 0.9)", color: "rgba(255, 255, 255, 0.7)" },
  },
});

/** The title bar: the wordmark on the left; Inspect a mouse, then minimize, maximize and close on
 *  the right. Closing hides the window to the tray, as the native button did. */
export function TitleBar() {
  const s = useStyles();
  const [active, setActive] = useState(true);
  const [maximized, setMaximized] = useState(false);
  const inspecting = useStore((x) => x.inspecting);

  useEffect(() => {
    const win = getCurrentWindow();
    const sync = () => win.isMaximized().then(setMaximized, () => {});
    sync();
    const unlisten = [win.onResized(sync), win.onFocusChanged(({ payload }) => setActive(payload))];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  }, []);

  const win = getCurrentWindow();
  const restoreLabel = maximized ? "Restore" : "Maximize";
  return (
    <div className={mergeClasses(s.bar, !active && s.inactive)} data-tauri-drag-region="deep">
      <Wordmark size={16} className={s.brand} />
      <div className={s.tools}>
        {!inspecting && (
          <button className={s.tool} title="Save a report to ask for a mouse hrtz doesn't run yet" onClick={() => setInspecting(true)}>
            <DocumentSearch16Regular />
            Inspect a mouse
          </button>
        )}
      </div>
      <div className={s.caption}>
        <button className={s.button} tabIndex={-1} title="Minimize" aria-label="Minimize" onClick={() => win.minimize()}>
          {""}
        </button>
        <button className={s.button} tabIndex={-1} title={restoreLabel} aria-label={restoreLabel} onClick={() => win.toggleMaximize()}>
          {maximized ? "" : ""}
        </button>
        <button className={mergeClasses(s.button, s.close)} tabIndex={-1} title="Close" aria-label="Close" onClick={() => win.close()}>
          {""}
        </button>
      </div>
    </div>
  );
}
