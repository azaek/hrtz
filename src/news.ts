import { useEffect, useState } from "react";
import type { DeviceState } from "./api";
import type { PageId } from "./App";
import { isZaopin } from "./models";
import { markSeen, useStore } from "./store";

/**
 * New features, each badged "New" on its page in the sidebar and beside its card, for a mouse it
 * applies to, until its card has been on screen (`useSeen`). Then it's noted in the prefs
 * (`seen`) and the badges go. Take an entry out a release or two after it shipped.
 */
export const NEWS = [
  // SignalRGB sets a Zaopin's light only.
  { id: "signalrgb", page: "sensor", applies: (d: DeviceState) => !!d.capabilities?.dpi?.light && isZaopin(d) },
] as const satisfies readonly {
  id: string;
  page: PageId;
  applies: (d: DeviceState) => boolean;
}[];

export type NewsId = (typeof NEWS)[number]["id"];

/** The sidebar's pages with something new for mouse `device`, not seen yet. */
export function useNewPages(device: DeviceState | null | undefined): Set<PageId> {
  const seen = useStore((x) => x.prefs?.seen);
  const pages = new Set<PageId>();
  if (!device?.capabilities || !seen) return pages;
  for (const n of NEWS) if (n.applies(device) && !seen.includes(n.id)) pages.add(n.page);
  return pages;
}

/**
 * Whether feature `id` is new, as it stood when its card appeared: the badge stays while the page
 * is open, after it's been noted. And the ref for its badge: once that has been fully on screen for
 * a moment, the feature is seen.
 */
export function useSeen(id: NewsId): { isNew: boolean; ref: (el: HTMLElement | null) => void } {
  const seen = useStore((x) => x.prefs?.seen);
  const [isNew, setIsNew] = useState<boolean | null>(null);
  const [el, setEl] = useState<HTMLElement | null>(null);
  useEffect(() => {
    if (isNew === null && seen) setIsNew(!seen.includes(id));
  }, [seen, id, isNew]);
  useEffect(() => {
    if (!el || !isNew) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const watch = new IntersectionObserver(
      ([e]) => {
        clearTimeout(timer);
        if (e?.isIntersecting) timer = setTimeout(() => markSeen(id), 1500);
      },
      { threshold: 1 },
    );
    watch.observe(el);
    return () => {
      watch.disconnect();
      clearTimeout(timer);
    };
  }, [el, isNew, id]);
  return { isNew: !!isNew, ref: setEl };
}
