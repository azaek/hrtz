import type { ReactNode } from "react";
import { Caption1, Title1 } from "@fluentui/react-components";
import { CheckmarkCircle16Filled, CheckmarkCircle20Filled, Circle16Regular, DismissCircle16Filled, Info20Regular, Warning20Filled } from "@fluentui/react-icons";
import { artOf } from "@/models";
import { MousePicture } from "@/MousePicture";
import type { Line, Verdict } from "./report";

// Pieces Inspect a mouse's check and its report share.

const muted = { color: "var(--colorNeutralForeground3)" };
const good = { color: "var(--colorPaletteGreenForeground1)" };
const warn = { color: "var(--colorPaletteDarkOrangeForeground1)" };

/** The mouse at the top of the page: its picture, its name, and a line under it. */
export function MouseHead({ artKey, variant, name, caption }: { artKey: string | null; variant?: number | null; name: string; caption: ReactNode }) {
  return (
    <>
      <MousePicture image={artOf(artKey, variant)?.image} width={200} />
      <Title1 as="h1" style={{ margin: 0 }}>
        {name}
      </Title1>
      <Caption1 style={muted}>{caption}</Caption1>
    </>
  );
}

/** What a check found, a line each: a tick, a cross, or a ring for what's only noted. */
export function Results({ lines }: { lines: Line[] }) {
  return (
    <ul className="m-0 flex list-none flex-col gap-1.5 p-0 text-left">
      {lines.map((l, i) => (
        <li key={i} className="flex items-start gap-2 text-sm">
          <span className="flex h-5 shrink-0 items-center">
            {l.ok === null ? <Circle16Regular style={muted} /> : l.ok ? <CheckmarkCircle16Filled style={good} /> : <DismissCircle16Filled style={warn} />}
          </span>
          <span>{l.text}</span>
        </li>
      ))}
    </ul>
  );
}

/** A verdict's mark: all there, something to look at again, or only a little. */
export function VerdictIcon({ tone }: { tone: Verdict["tone"] }) {
  if (tone === "good") return <CheckmarkCircle20Filled style={good} />;
  if (tone === "warn") return <Warning20Filled style={warn} />;
  return <Info20Regular style={muted} />;
}
