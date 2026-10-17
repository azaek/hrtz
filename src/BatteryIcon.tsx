import {
  Battery020Regular,
  Battery120Regular,
  Battery220Regular,
  Battery320Regular,
  Battery420Regular,
  Battery520Regular,
  Battery620Regular,
  Battery720Regular,
  Battery820Regular,
  Battery920Regular,
  Battery1020Regular,
  BatteryCharge20Regular,
} from "@fluentui/react-icons";

const LEVELS = [
  Battery020Regular,
  Battery120Regular,
  Battery220Regular,
  Battery320Regular,
  Battery420Regular,
  Battery520Regular,
  Battery620Regular,
  Battery720Regular,
  Battery820Regular,
  Battery920Regular,
  Battery1020Regular,
];

/** Fluent battery glyph in 10 % steps, or the charging glyph. */
export function BatteryIcon({ level, charging, className }: { level: number; charging: boolean; className?: string }) {
  const Icon = charging ? BatteryCharge20Regular : LEVELS[Math.min(10, Math.max(0, Math.round(level / 10)))];
  return <Icon className={className} aria-hidden />;
}
