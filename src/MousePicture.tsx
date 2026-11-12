import { useId } from "react";
import { EMBLEM } from "./brand";

/**
 * The placeholder for a mouse with no picture, and for no mouse at all (the empty mouse card, the
 * onboarding while it waits for one, Inspect a mouse): the hrtz emblem, a mouse shell split at its
 * buttons, dashed and striped, upright in the model renders' frame (298 × 263). It takes
 * `currentColor`.
 */
export function MousePlaceholder({ width }: { width: number }) {
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

/** A mouse's picture at `width`: its render (`image`), else the placeholder. */
export function MousePicture({ image, width }: { image?: string; width: number }) {
  if (image) return <img src={image} alt="" width={width} style={{ display: "block", height: "auto", pointerEvents: "none", userSelect: "none" }} />;
  return <MousePlaceholder width={width} />;
}
