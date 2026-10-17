import { useEffect, useState } from "react";
import { Body1Strong, Caption1, makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { Stack20Regular, TopSpeed20Regular, Pulse20Regular } from "@fluentui/react-icons";
import { listen } from "@tauri-apps/api/event";

/** How long the pop-up stays before it fades; the app hides the window just after. */
const SHOW_MS = 1200;

type Message = { kind: "dpi" | "polling" | "profile"; title: string; detail: string };

const ICONS = { dpi: <TopSpeed20Regular />, polling: <Pulse20Regular />, profile: <Stack20Regular /> };

const useStyles = makeStyles({
  stage: { height: "100%", display: "grid", placeItems: "center" },
  // A Windows 11 flyout: the theme's own surface, a hairline edge, 8 px corners.
  pill: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalM,
    height: "44px",
    padding: `0 ${tokens.spacingHorizontalL}`,
    borderRadius: "8px",
    backgroundColor: "var(--flyout)",
    boxShadow: `0 0 0 1px var(--flyout-stroke), ${tokens.shadow16}`,
    color: tokens.colorNeutralForeground1,
    animationName: { from: { opacity: 0, transform: "translateY(4px)" }, to: { opacity: 1, transform: "none" } },
    animationDuration: "150ms",
    animationTimingFunction: "cubic-bezier(0, 0, 0, 1)",
    "@media (prefers-reduced-motion: reduce)": { animationName: { from: { opacity: 0 }, to: { opacity: 1 } } },
  },
  leaving: {
    opacity: 0,
    transitionProperty: "opacity",
    transitionDuration: "200ms",
    transitionTimingFunction: "ease-in",
  },
  icon: { display: "flex", fontSize: "20px", color: tokens.colorNeutralForeground2 },
  value: { fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap" },
  detail: { color: tokens.colorNeutralForeground3, whiteSpace: "nowrap" },
});

/** The DPI, polling and profile pop-up near the bottom of the screen. */
export default function Osd() {
  const s = useStyles();
  const [msg, setMsg] = useState<(Message & { n: number }) | null>(null);
  const [leaving, setLeaving] = useState(false);

  useEffect(() => {
    let n = 0;
    const un = listen<Message>("osd", (e) => {
      setLeaving(false);
      setMsg({ ...e.payload, n: ++n });
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // Each new value restarts the clock, so pressing through DPI stages keeps it up.
  useEffect(() => {
    if (!msg) return;
    const t = setTimeout(() => setLeaving(true), SHOW_MS);
    return () => clearTimeout(t);
  }, [msg?.n]);

  if (!msg) return null;
  return (
    <div className={s.stage}>
      <div key={msg.n} className={mergeClasses(s.pill, leaving && s.leaving)} role="status">
        <span className={s.icon} aria-hidden>
          {ICONS[msg.kind]}
        </span>
        <Body1Strong className={s.value}>{msg.title}</Body1Strong>
        <Caption1 className={s.detail}>{msg.detail}</Caption1>
      </div>
    </div>
  );
}
