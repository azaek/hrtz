import { Fragment, useEffect, useState, type ReactNode } from "react";
import {
  Body1,
  Button,
  Caption1,
  Caption1Strong,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  makeStyles,
  Subtitle2,
  tokens,
} from "@fluentui/react-components";
import { LATEST, RELEASES } from "./changelog";

/** The last version the "What's new" window was opened for. */
const SEEN_KEY = "hrtz.seenVersion";
/** Dispatch on `window` to open the window (Settings' What's new button). */
export const OPEN_WHATS_NEW = "hrtz:whats-new";

const useStyles = makeStyles({
  surface: { maxWidth: "560px" },
  body: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXXL },
  release: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalM },
  head: { display: "flex", alignItems: "baseline", gap: tokens.spacingHorizontalS },
  muted: { color: tokens.colorNeutralForeground3 },
  section: { display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS },
  list: { margin: 0, paddingLeft: "18px", display: "flex", flexDirection: "column", gap: tokens.spacingVerticalXS },
  code: { fontFamily: tokens.fontFamilyMonospace, fontSize: "0.92em" },
});

/** `code` spans in a changelog line; everything else is plain text. */
function Inline({ text }: { text: string }) {
  const s = useStyles();
  const parts: ReactNode[] = text.split("`").map((part, i) => (i % 2 ? <code key={i} className={s.code}>{part}</code> : part));
  return <>{parts}</>;
}

/** A version hasn't been looked at yet: the What's new button shows a dot. */
export const whatsNewUnseen = () => !!LATEST && localStorage.getItem(SEEN_KEY) !== LATEST;

/** The What's new window: what changed in each version, newest first. Opened by OPEN_WHATS_NEW. */
export function WhatsNew() {
  const s = useStyles();
  const [open, setOpen] = useState(false);

  const show = () => {
    setOpen(true);
    localStorage.setItem(SEEN_KEY, LATEST);
  };

  useEffect(() => {
    const open = () => show();
    window.addEventListener(OPEN_WHATS_NEW, open);
    return () => window.removeEventListener(OPEN_WHATS_NEW, open);
  }, []);

  if (!RELEASES.length) return null;
  return (
    <>
      <Dialog open={open} onOpenChange={(_, d) => setOpen(d.open)}>
        <DialogSurface className={s.surface}>
          <DialogBody>
            <DialogTitle>What's new</DialogTitle>
            <DialogContent className={s.body}>
              {RELEASES.map((r) => (
                <section key={r.version} className={s.release} aria-label={`Version ${r.version}`}>
                  <div className={s.head}>
                    <Subtitle2>{r.version}</Subtitle2>
                    <Caption1 className={s.muted}>{r.date}</Caption1>
                  </div>
                  {r.sections.map((sec) => (
                    <div key={sec.title} className={s.section}>
                      <Caption1Strong className={s.muted}>{sec.title}</Caption1Strong>
                      <ul className={s.list}>
                        {sec.items.map((item, i) => (
                          <Fragment key={i}>
                            <li>
                              <Body1>
                                <Inline text={item} />
                              </Body1>
                            </li>
                          </Fragment>
                        ))}
                      </ul>
                    </div>
                  ))}
                </section>
              ))}
            </DialogContent>
            <DialogActions>
              <Button appearance="primary" onClick={() => setOpen(false)}>
                Close
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </>
  );
}
