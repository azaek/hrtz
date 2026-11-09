import { useId } from "react";
import { EMBLEM } from "./brand";

/**
 * No mouse at all (the empty mouse card, the onboarding while it waits for one, Inspect a mouse):
 * the hrtz emblem, a mouse shell split at its buttons, dashed and striped, upright in the model
 * renders' frame (298 × 263). It takes `currentColor`.
 */
function NoMouse({ width }: { width: number }) {
  const id = useId().replace(/[^\w-]/g, "");
  // 82% of the frame's height, centred; the dashes and stripes as wide on screen as the frame says.
  const k = (0.82 * 263) / EMBLEM.height;
  const shape = `translate(${(298 - EMBLEM.width * k) / 2} ${(263 - EMBLEM.height * k) / 2}) scale(${k})`;
  return (
    <svg width={width} height={(width * 263) / 298} viewBox="0 0 298 263" aria-hidden style={{ display: "block" }}>
      <defs>
        <pattern id={`${id}-stripes`} width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
          <rect width="3" height="10" fill="currentColor" />
        </pattern>
        <clipPath id={`${id}-shape`}>
          <path transform={shape} d={EMBLEM.left} />
          <path transform={shape} d={EMBLEM.right} />
        </clipPath>
      </defs>
      <rect width="298" height="263" fill={`url(#${id}-stripes)`} fillOpacity="0.16" clipPath={`url(#${id}-shape)`} />
      <g
        transform={shape}
        fill="none"
        stroke="currentColor"
        strokeOpacity="0.45"
        strokeWidth={3 / k}
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeDasharray={`${10 / k} ${9 / k}`}
      >
        <path d={EMBLEM.left} />
        <path d={EMBLEM.right} />
      </g>
    </svg>
  );
}

/**
 * A mouse drawn from above at an angle, in the model renders' frame (298 × 263): a stand-in for a
 * mouse with no render yet. `outline` is for no mouse at all, and draws the emblem instead
 * (`NoMouse`).
 */
export function DrawnMouse({ width, outline }: { width: number; outline?: boolean }) {
  if (outline) return <NoMouse width={width} />;
  const body =
    "M 0 -100 C 34 -100 46 -80 48 -50 C 50 -20 52 20 50 50 C 47 85 28 100 0 100 C -28 100 -47 85 -50 50 C -52 20 -50 -20 -48 -50 C -46 -80 -34 -100 0 -100 Z";
  const id = "drawn-mouse";
  return (
    <svg width={width} height={(width * 263) / 298} viewBox="0 0 298 263" aria-hidden style={{ display: "block" }}>
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#fbfbfc" />
          <stop offset="0.55" stopColor="#e4e5e9" />
          <stop offset="1" stopColor="#a9abb3" />
        </linearGradient>
        <radialGradient id={`${id}-shadow`}>
          <stop offset="0" stopColor="#000" stopOpacity="0.35" />
          <stop offset="1" stopColor="#000" stopOpacity="0" />
        </radialGradient>
      </defs>
      <ellipse cx="156" cy="150" rx="128" ry="92" fill={`url(#${id}-shadow)`} transform="rotate(-40 156 150)" />
      <g transform="translate(149 128) rotate(-40)">
        <path d={body} fill={`url(#${id})`} stroke="rgb(0 0 0 / 0.18)" strokeWidth="1.5" />
        <path d="M 0 -100 L 0 -40 M -48 -38 Q 0 -28 48 -38" fill="none" stroke="rgb(0 0 0 / 0.28)" strokeWidth="2" />
        <path d="M -40 -70 C -36 -88 -20 -97 -6 -98" fill="none" stroke="rgb(255 255 255 / 0.9)" strokeWidth="3" strokeLinecap="round" />
        <rect x="-6" y="-88" width="12" height="28" rx="6" fill="#2a2b30" stroke="rgb(0 0 0 / 0.3)" />
      </g>
    </svg>
  );
}

/** A mouse's picture at `width`: its render (`image`), else the drawn one; outlined for none. */
export function MousePicture({ image, width, none }: { image?: string; width: number; none?: boolean }) {
  if (image) return <img src={image} alt="" width={width} style={{ display: "block", height: "auto", pointerEvents: "none", userSelect: "none" }} />;
  return <DrawnMouse width={width} outline={none} />;
}
