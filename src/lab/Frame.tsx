import { useEffect, useState, type ComponentType, type ReactNode } from "react";
import { FluentProvider } from "@fluentui/react-components";
import App, { type PageId } from "@/App";
import { setStatesTheme } from "@/states";
import { dark, light } from "@/theme";
import { Picker } from "./Picker";

// Throwaway, like the labs: the real app (on the stand-in backend) at the window's real size, one
// page swapped for the direction being looked at, and the lab's own controls in a panel beside it.
//
// URL: ?v=1-4 direction, ?theme=light|dark, ?size=1080|880|1400, ?panel=0 (the window alone, for
// screenshots).

/** `enter` runs as the direction is picked, before its page mounts: a lab resets its data there. */
export type Variant = { name: string; Component: ComponentType; note: string; enter?: () => void };

const SIZES = { 880: [880, 600], 1080: [1080, 720], 1400: [1400, 860] } as const;
type Size = keyof typeof SIZES;

const params = new URLSearchParams(location.search);

export function Frame({ page, variants, panel }: { page: PageId; variants: Variant[]; panel: (v: Variant) => ReactNode }) {
  const [current, setCurrent] = useState(() => {
    const i = Math.min(variants.length, Math.max(1, Number(params.get("v")) || 1)) - 1;
    variants[i].enter?.();
    return i;
  });
  const pick = (i: number) => {
    variants[i].enter?.();
    setCurrent(i);
  };
  const [isDark, setDark] = useState(() => (params.get("theme") ? params.get("theme") === "dark" : matchMedia("(prefers-color-scheme: dark)").matches));
  const [size, setSize] = useState<Size>(() => (Number(params.get("size")) in SIZES ? (Number(params.get("size")) as Size) : 1080));
  const showPanel = params.get("panel") !== "0";

  useEffect(() => {
    document.documentElement.dataset.theme = isDark ? "dark" : "light";
    document.documentElement.classList.toggle("dark", isDark);
    setStatesTheme(isDark);
    const url = new URL(location.href);
    url.searchParams.set("theme", isDark ? "dark" : "light");
    url.searchParams.set("size", String(size));
    history.replaceState(null, "", url);
  }, [isDark, size]);

  const v = variants[current];
  const [w, h] = SIZES[size];
  const field = "h-7 rounded-md border bg-transparent px-2";

  return (
    <FluentProvider theme={isDark ? dark : light} className="provider" style={{ background: "transparent" }}>
      <div
        className="tw"
        style={{ display: "flex", gap: 20, padding: showPanel ? 20 : 0, height: "100vh", overflow: "auto", boxSizing: "border-box", background: isDark ? "#111" : "#d9d9d9", alignItems: "flex-start" }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 10, flexShrink: 0 }}>
          {showPanel && (
            <p className="m-0 max-w-[1080px] text-xs text-muted-foreground" style={{ width: w }}>
              <span className="font-medium text-foreground">{v.name}:</span> {v.note}
            </p>
          )}
          {/* The window: Mica's usual tone behind the app, which draws no background of its own. */}
          <div
            style={{
              width: w,
              height: h,
              overflow: "hidden",
              borderRadius: showPanel ? 8 : 0,
              background: isDark ? "#202020" : "#f3f3f3",
              boxShadow: showPanel ? "0 0 0 1px rgba(255,255,255,0.08), 0 16px 48px rgba(0,0,0,0.4)" : undefined,
            }}
          >
            <App pages={{ [page]: v.Component }} start={page} />
          </div>
        </div>

        {showPanel && (
          <aside className="flex w-[300px] shrink-0 flex-col gap-3 text-xs text-muted-foreground">
            <div className="grid grid-cols-2 gap-2">
              <label className="flex flex-col gap-1">
                Window
                <select className={field} value={size} onChange={(e) => setSize(Number(e.target.value) as Size)}>
                  {(Object.keys(SIZES) as unknown as Size[]).map((s) => (
                    <option key={s} value={s} className="bg-popover">
                      {SIZES[s][0]} × {SIZES[s][1]}
                      {Number(s) === 1080 ? " (default)" : Number(s) === 880 ? " (smallest)" : ""}
                    </option>
                  ))}
                </select>
              </label>
              <label className="flex flex-col gap-1">
                Theme
                <select className={field} value={isDark ? "dark" : "light"} onChange={(e) => setDark(e.target.value === "dark")}>
                  <option value="dark" className="bg-popover">Dark</option>
                  <option value="light" className="bg-popover">Light</option>
                </select>
              </label>
            </div>
            {panel(v)}
          </aside>
        )}
      </div>
      {showPanel && <Picker names={variants.map((x) => x.name)} current={current} onPick={pick} />}
    </FluentProvider>
  );
}

/** A panel heading. */
export function PanelTitle({ children }: { children: ReactNode }) {
  return <div className="mt-2 font-medium text-foreground">{children}</div>;
}

/** A panel button that makes something happen. */
export function PanelButton({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return (
    <button className="h-8 cursor-pointer rounded-md border px-2 text-left hover:bg-muted" onClick={onClick}>
      {children}
    </button>
  );
}
