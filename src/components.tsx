import { createContext, useContext, useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  Body1,
  Body1Strong,
  Caption1,
  makeStyles,
  mergeClasses,
  ProgressBar,
  Toast,
  ToastTitle,
  Toaster,
  ToggleButton,
  tokens,
  useId,
  useToastController,
} from "@fluentui/react-components";
import { Apps20Regular } from "@fluentui/react-icons";
import { useStore } from "./store";

const useStyles = makeStyles({
  card: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalL,
    minHeight: "64px",
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    backgroundColor: "var(--card)",
    border: "1px solid var(--card-stroke)",
    borderRadius: tokens.borderRadiusLarge,
    boxSizing: "border-box",
  },
  stacked: {
    flexDirection: "column",
    alignItems: "stretch",
  },
  icon: {
    display: "flex",
    fontSize: "20px",
    color: tokens.colorNeutralForeground2,
    flexShrink: 0,
  },
  text: {
    display: "flex",
    flexDirection: "column",
    minWidth: 0,
    flexGrow: 1,
  },
  description: {
    color: tokens.colorNeutralForeground3,
  },
  control: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalS,
    flexShrink: 0,
  },
  header: {
    display: "flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalL,
  },
  group: {
    display: "flex",
    flexDirection: "column",
    gap: "4px",
  },
  groupTitle: {
    marginTop: tokens.spacingVerticalL,
    marginBottom: tokens.spacingVerticalXS,
  },
  swatch: {
    width: "8px",
    height: "8px",
    borderRadius: tokens.borderRadiusCircular,
    boxShadow: `inset 0 0 0 1px ${tokens.colorNeutralStroke1}`,
  },
  // Callouts: a card washed with the tone colour, edged in it, and a tinted glyph disc.
  toneWarning: { "--tone": tokens.colorStatusWarningForeground1 },
  toneAsleep: { "--tone": tokens.colorPaletteMarigoldForeground1 },
  notice: {
    display: "flex",
    alignItems: "flex-start",
    gap: tokens.spacingHorizontalM,
    padding: `${tokens.spacingVerticalM} ${tokens.spacingHorizontalL}`,
    borderRadius: tokens.borderRadiusLarge,
    backgroundColor: "color-mix(in srgb, var(--tone) 8%, var(--card))",
    boxShadow: "inset 0 0 0 1px color-mix(in srgb, var(--tone) 22%, transparent)",
    animationName: { from: { opacity: 0, transform: "translateY(-4px)" } },
    animationDuration: "200ms",
    animationTimingFunction: "cubic-bezier(0, 0, 0, 1)",
  },
  disc: {
    position: "relative",
    display: "grid",
    placeItems: "center",
    width: "24px",
    height: "24px",
    flexShrink: 0,
    borderRadius: tokens.borderRadiusCircular,
    backgroundColor: "color-mix(in srgb, var(--tone) 18%, transparent)",
    color: "var(--tone)",
    fontSize: "16px",
  },
  noticeText: {
    display: "flex",
    flexDirection: "column",
    gap: "2px",
    minWidth: 0,
    // Optically centre one-line notices on the 24px disc.
    paddingTop: "2px",
  },
  noticeBody: { color: tokens.colorNeutralForeground2 },
  pill: {
    display: "inline-flex",
    alignItems: "center",
    gap: tokens.spacingHorizontalS,
    height: "32px",
    padding: `0 ${tokens.spacingHorizontalL} 0 ${tokens.spacingHorizontalXS}`,
    boxSizing: "border-box",
    borderRadius: tokens.borderRadiusCircular,
    backgroundColor: "color-mix(in srgb, var(--tone) 10%, var(--card))",
    boxShadow: "inset 0 0 0 1px color-mix(in srgb, var(--tone) 24%, transparent)",
    whiteSpace: "nowrap",
    animationName: { from: { opacity: 0, transform: "translateY(-4px) scale(0.98)" } },
    animationDuration: "200ms",
    animationTimingFunction: "cubic-bezier(0, 0, 0, 1)",
  },
  // A slow ripple off the disc: waiting for the mouse, not an error.
  breathing: {
    "::after": {
      content: '""',
      position: "absolute",
      inset: 0,
      borderRadius: tokens.borderRadiusCircular,
      boxShadow: "0 0 0 1.5px var(--tone)",
      opacity: 0,
      pointerEvents: "none",
      animationName: {
        "0%": { opacity: 0.6, transform: "scale(1)" },
        "60%, 100%": { opacity: 0, transform: "scale(1.75)" },
      },
      animationDuration: "2.8s",
      animationTimingFunction: "cubic-bezier(0, 0, 0.2, 1)",
      animationIterationCount: "infinite",
    },
  },
  pillDetail: { color: tokens.colorNeutralForeground2 },
  // Reveal: the body of a stacked SettingCard that opens with its switch. Closed, the negative
  // margin cancels the card's 16px gap so the card is an ordinary settings row.
  reveal: {
    display: "grid",
    gridTemplateRows: "0fr",
    marginTop: `calc(-1 * ${tokens.spacingHorizontalL})`,
    opacity: 0,
    transitionProperty: "grid-template-rows, opacity",
    transitionDuration: tokens.durationNormal,
    transitionTimingFunction: tokens.curveDecelerateMid,
  },
  revealOpen: { gridTemplateRows: "1fr", opacity: 1 },
  // Clip while it opens and closes, with room for focus rings at the edges.
  revealClip: { minHeight: 0, overflow: "clip", overflowClipMargin: "8px" },
  // Lines up with the card's title text (20px icon + 16px gap).
  revealBody: { paddingTop: tokens.spacingHorizontalL, paddingLeft: "36px" },
});

/** The body of a stacked SettingCard that opens and closes with the card's switch. */
/** An app's own icon, or a generic one while it loads or when it has none. */
export function AppIcon({ src, className }: { src: string | null | undefined; className?: string }) {
  return src ? <img src={src} alt="" className={className} /> : <Apps20Regular className={className} />;
}

export function Reveal({ open, children }: { open: boolean; children: ReactNode }) {
  const s = useStyles();
  return (
    <div className={mergeClasses(s.reveal, open && s.revealOpen)} inert={!open}>
      <div className={s.revealClip}>
        <div className={s.revealBody}>{children}</div>
      </div>
    </div>
  );
}

type Tone = "warning" | "asleep";

/** Inline callout for something the user has to fix: tinted card, glyph, title and one line of help. */
export function Notice(props: { tone: Tone; icon: ReactNode; title: string; children: ReactNode }) {
  const s = useStyles();
  return (
    <div className={mergeClasses(s.notice, props.tone === "warning" ? s.toneWarning : s.toneAsleep)} role="status">
      <span className={s.disc} aria-hidden>
        {props.icon}
      </span>
      <div className={s.noticeText}>
        <Body1Strong>{props.title}</Body1Strong>
        <Body1 className={s.noticeBody}>{props.children}</Body1>
      </div>
    </div>
  );
}

/** Compact status chip for a passing state that should not push the page around. */
export function StatusPill(props: { tone: Tone; icon: ReactNode; title: string; detail: string; hint?: string; breathing?: boolean }) {
  const s = useStyles();
  return (
    <div className={mergeClasses(s.pill, props.tone === "warning" ? s.toneWarning : s.toneAsleep)} title={props.hint}>
      <span className={mergeClasses(s.disc, props.breathing && s.breathing)} aria-hidden>
        {props.icon}
      </span>
      <Body1Strong>{props.title}</Body1Strong>
      <Body1 className={s.pillDetail}>{props.detail}</Body1>
    </div>
  );
}

/** A Windows Settings-style row: icon, title, description, and a control on the right. */
export function SettingCard(props: {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  /** Put the control below the text instead of on the right. */
  stacked?: boolean;
  below?: ReactNode;
}) {
  const s = useStyles();
  const head = (
    <>
      {props.icon && <span className={s.icon}>{props.icon}</span>}
      <div className={s.text}>
        <Body1>{props.title}</Body1>
        {props.description && <Caption1 className={s.description}>{props.description}</Caption1>}
      </div>
    </>
  );
  if (props.stacked) {
    return (
      <div className={mergeClasses(s.card, s.stacked)}>
        <div className={s.header}>
          {head}
          {props.children && <div className={s.control}>{props.children}</div>}
        </div>
        {props.below}
      </div>
    );
  }
  return (
    <div className={s.card}>
      {head}
      <div className={s.control}>{props.children}</div>
    </div>
  );
}

export function Group(props: { title?: string; children: ReactNode }) {
  const s = useStyles();
  return (
    <section className={s.group}>
      {/* 14px semibold, like section headers in Windows Settings and PowerToys. */}
      {props.title && <Body1Strong className={s.groupTitle} as="h2">{props.title}</Body1Strong>}
      {props.children}
    </section>
  );
}

/** Mutually exclusive choice as a row of toggle buttons. The selected look comes from states.css. */
export function Choice<T extends string | number>(props: {
  label: string;
  value: T;
  disabled?: boolean;
  options: { value: T; label: ReactNode; swatch?: string; title?: string }[];
  onChange: (v: T) => void;
}) {
  const s = useStyles();
  return (
    <div className="st-seg-group" role="group" aria-label={props.label}>
      {props.options.map((o) => (
        <ToggleButton
          key={String(o.value)}
          className="st-seg"
          title={o.title}
          disabled={props.disabled}
          checked={o.value === props.value}
          icon={o.swatch ? <span className={s.swatch} style={{ background: o.swatch }} aria-hidden /> : undefined}
          onClick={() => o.value !== props.value && props.onChange(o.value)}
        >
          {o.label}
        </ToggleButton>
      ))}
    </div>
  );
}

/** The page header's right-hand slot (App renders it); pages fill it through PageActions. */
export const HeaderActionsSlot = createContext<HTMLElement | null>(null);

/** Buttons that belong beside the page title, in the fixed header above the scrolling page. */
export function PageActions({ children }: { children: ReactNode }) {
  const slot = useContext(HeaderActionsSlot);
  return slot ? createPortal(children, slot) : null;
}

/** A navigation row, as in WinUI's NavigationView. Look and selected state come from states.css. */
export function NavRow(props: { icon: ReactNode; label: string; selected: boolean; onClick?: () => void }) {
  return (
    <button
      className="st-row"
      data-selected={props.selected}
      aria-current={props.selected ? "page" : undefined}
      onClick={props.onClick}
    >
      <span className="st-row__icon">{props.icon}</span>
      <Body1>{props.label}</Body1>
    </button>
  );
}

/** Mirrors store toasts into Fluent's Toaster. */
export function ToastBridge() {
  const toasterId = useId("toaster");
  const { dispatchToast } = useToastController(toasterId);
  const toasts = useStore((s) => s.toasts);
  const shown = useRef(new Set<number>());
  useEffect(() => {
    for (const t of toasts) {
      if (shown.current.has(t.id)) continue;
      shown.current.add(t.id);
      dispatchToast(
        <Toast>
          <ToastTitle>{t.text}</ToastTitle>
        </Toast>,
        { intent: t.tone === "error" ? "error" : "success", timeout: t.tone === "error" ? 6000 : 2500 },
      );
    }
  }, [toasts, dispatchToast]);
  return <Toaster toasterId={toasterId} position="bottom-end" />;
}

export function DeviceProgress() {
  const progress = useStore((s) => s.progress);
  if (!progress) return null;
  return <ProgressBar value={progress[0] / progress[1]} thickness="medium" />;
}
