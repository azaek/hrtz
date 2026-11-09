import type { ReactNode } from "react";
import { Button, Link, Switch } from "@fluentui/react-components";
import { Color20Regular } from "@fluentui/react-icons";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, message } from "@/api";
import { SettingCard } from "@/components";
import { NewBadge } from "@/kit";
import { useSeen } from "@/news";
import { savePrefs, toast, useStore } from "@/store";

/** Whether SignalRGB is setting mouse `key`'s light now, so the page's own light controls step
 *  aside. */
export function useSetBySignalRgb(key: string | null | undefined): boolean {
  return useStore((x) => !!key && !!x.prefs?.signalrgb && !!x.status.signalrgb?.driving.includes(key));
}

/**
 * The SignalRGB link (hrtz-notes/SIGNALRGB.md): SignalRGB sets the DPI light with the rest of the
 * PC's lighting, through hrtz's add-on. The description says where it stands, with the one thing
 * to do next beside the switch.
 */
export function SignalRgbCard({ mouseKey }: { mouseKey: string | null | undefined }) {
  const prefs = useStore((x) => x.prefs);
  const link = useStore((x) => x.status.signalrgb);
  const driving = useSetBySignalRgb(mouseKey);
  const news = useSeen("signalrgb");
  if (!prefs) return null;
  const on = prefs.signalrgb;

  const install = () =>
    void api
      .installSignalRgbAddon()
      .then(() => toast("Added to SignalRGB. Restart SignalRGB to load it."))
      .catch((e) => toast(message(e), "error"));

  let description: string;
  let action: ReactNode = null;
  if (!on || !link) {
    description = "Lets SignalRGB set the DPI light with the rest of your lighting.";
  } else if (link.error) {
    description = link.error;
  } else if (!link.installed) {
    description = "SignalRGB isn't installed on this PC.";
    action = <Link onClick={() => void openUrl("https://signalrgb.com").catch((e) => toast(message(e), "error"))}>Get SignalRGB</Link>;
  } else if (link.addon !== "current") {
    description =
      link.addon === "missing" ? "Add hrtz to SignalRGB, then restart SignalRGB." : "hrtz's add-on in SignalRGB is out of date. Update it, then restart SignalRGB.";
    action = <Button onClick={install}>{link.addon === "missing" ? "Add to SignalRGB" : "Update add-on"}</Button>;
  } else if (driving) {
    description = "SignalRGB is setting the light. Animated effects keep the last solid colour.";
  } else if (link.connected) {
    description = "Connected to SignalRGB.";
  } else {
    description = "Waiting for SignalRGB. If it's open, restart it to load hrtz's add-on.";
  }

  return (
    <SettingCard
      icon={<Color20Regular />}
      title={
        <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          SignalRGB
          {news.isNew && <NewBadge ref={news.ref} />}
        </span>
      }
      description={description}
    >
      {action}
      <Switch checked={on} onChange={(_, d) => void savePrefs({ ...prefs, signalrgb: d.checked })} aria-label="Let SignalRGB set the DPI light" />
    </SettingCard>
  );
}
