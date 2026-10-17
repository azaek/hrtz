import { useEffect, useState } from "react";
import { Body1, Button, Caption1 } from "@fluentui/react-components";
import { getVersion } from "@tauri-apps/api/app";
import { Emblem } from "@/brand";
import { LATEST } from "@/changelog";
import { OPEN_WHATS_NEW, whatsNewUnseen } from "@/WhatsNew";

/**
 * hrtz itself, at the top of Settings like PowerToys' General page: its mark on the app icon's
 * dark plate, the version, and What's new, with a dot until the latest version's notes are read.
 */
export function AppCard() {
  const [version, setVersion] = useState(LATEST);
  const [unseen, setUnseen] = useState(whatsNewUnseen);
  useEffect(() => {
    getVersion().then(setVersion, () => {});
  }, []);
  return (
    <div className="tw flex items-center gap-4 rounded-[var(--borderRadiusLarge)] border border-[color:var(--card-stroke)] bg-[var(--card)] px-5 py-4">
      {/* The app icon's plate (branding/build-icons.py): a dark gradient with a faint light edge. */}
      <span className="grid size-14 shrink-0 place-items-center rounded-xl bg-gradient-to-b from-[#383838] to-[#1d1d1d] shadow-[inset_0_0_0_1px_rgb(255_255_255/0.1)] text-white">
        <Emblem height={30} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col">
        <Body1 style={{ fontWeight: 600, fontSize: 16 }}>hrtz</Body1>
        <Caption1 style={{ color: "var(--colorNeutralForeground3)", fontVariantNumeric: "tabular-nums" }}>Version {version} · Settings and tray companion for gaming mice</Caption1>
      </div>
      <Button
        aria-label={unseen ? `What's new in ${LATEST}, not opened yet` : "What's new"}
        onClick={() => {
          setUnseen(false);
          window.dispatchEvent(new Event(OPEN_WHATS_NEW));
        }}
      >
        What's new
        {/* A soft marker for notes not read yet: the accent dot, no outline. */}
        {unseen && <span aria-hidden className="ml-2 size-1.5 rounded-full bg-[var(--accent)]" />}
      </Button>
    </div>
  );
}
