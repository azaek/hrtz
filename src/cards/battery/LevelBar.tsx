import { cn } from "@/lib/utils";

/** The battery level as a thin bar: green while charging, amber at or under the low-battery level. */
export function LevelBar({ level, charging, low, className }: { level: number; charging?: boolean; low?: number; className?: string }) {
  const warn = !charging && low != null && level <= low;
  return (
    <div role="presentation" className={cn("h-1.5 overflow-hidden rounded-full bg-foreground/10", className)}>
      <div
        className={cn("h-full rounded-full transition-[width] duration-500 ease-out", charging ? "bg-emerald-400" : warn ? "bg-amber-400" : "bg-foreground/85")}
        style={{ width: `${Math.max(2, level)}%` }}
      />
    </div>
  );
}
