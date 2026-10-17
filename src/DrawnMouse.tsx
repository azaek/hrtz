/**
 * A mouse drawn from above at an angle, in the model renders' frame (298 × 263): a stand-in for a
 * mouse with no render yet. `outline` draws it dashed, for no mouse at all (the "No mouse
 * connected" card, the onboarding while it waits for one). The outline takes `currentColor`.
 */
export function DrawnMouse({ width, outline }: { width: number; outline?: boolean }) {
  const body =
    "M 0 -100 C 34 -100 46 -80 48 -50 C 50 -20 52 20 50 50 C 47 85 28 100 0 100 C -28 100 -47 85 -50 50 C -52 20 -50 -20 -48 -50 C -46 -80 -34 -100 0 -100 Z";
  const id = outline ? "drawn-mouse-o" : "drawn-mouse";
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
      {!outline && <ellipse cx="156" cy="150" rx="128" ry="92" fill={`url(#${id}-shadow)`} transform="rotate(-40 156 150)" />}
      <g transform="translate(149 128) rotate(-40)">
        {outline ? (
          <g fill="none" stroke="currentColor" strokeOpacity="0.45" strokeWidth="3" strokeLinecap="round">
            <path d={body} strokeDasharray="10 9" />
            <path d="M 0 -100 L 0 -40 M -48 -38 Q 0 -28 48 -38" />
            <rect x="-6" y="-88" width="12" height="28" rx="6" />
          </g>
        ) : (
          <>
            <path d={body} fill={`url(#${id})`} stroke="rgb(0 0 0 / 0.18)" strokeWidth="1.5" />
            <path d="M 0 -100 L 0 -40 M -48 -38 Q 0 -28 48 -38" fill="none" stroke="rgb(0 0 0 / 0.28)" strokeWidth="2" />
            <path d="M -40 -70 C -36 -88 -20 -97 -6 -98" fill="none" stroke="rgb(255 255 255 / 0.9)" strokeWidth="3" strokeLinecap="round" />
            <rect x="-6" y="-88" width="12" height="28" rx="6" fill="#2a2b30" stroke="rgb(0 0 0 / 0.3)" />
          </>
        )}
      </g>
    </svg>
  );
}

/** A mouse's picture at `width`: its render (`image`), else the drawn one; outlined for none. */
export function MousePicture({ image, width, none }: { image?: string; width: number; none?: boolean }) {
  if (image) return <img src={image} alt="" width={width} style={{ display: "block", height: "auto", pointerEvents: "none", userSelect: "none" }} />;
  return <DrawnMouse width={width} outline={none} />;
}
