import { makeStyles, mergeClasses } from "@fluentui/react-components";

// The emblem, 226 x 247 (branding/emblem.svg): a mouse shell split at the button seam, the left
// button tall and the right one short, so it reads as an h.
const LEFT =
  "M0 112.091C0 59.6577 35.7123 15.565 84.1404 2.81008C95.2232 -0.108917 100.765 -1.56842 105.876 2.53566C110.987 6.63973 110.723 13.2744 110.195 26.5436L102.447 221.126C101.977 232.934 101.742 238.838 97.968 242.465C94.1942 246.091 88.2853 246.091 76.4677 246.091H29.7025C19.719 246.091 14.7272 246.091 10.8658 244.262C6.90057 242.384 3.70768 239.191 1.82925 235.226C0 231.364 0 226.372 0 216.389V112.091Z";
const RIGHT =
  "M118.629 102.064C118.082 88.7644 117.809 82.1144 122.96 78.0039C128.11 73.8934 133.629 75.3895 144.666 78.3815C192.383 91.3162 226 135.728 226 188.091V216.389C226 226.372 226 231.364 224.171 235.226C222.292 239.191 219.099 242.384 215.134 244.262C211.273 246.091 206.281 246.091 196.297 246.091H149.501C137.697 246.091 131.795 246.091 128.022 242.471C124.249 238.85 124.007 232.953 123.523 221.159L118.629 102.064Z";
const W = 226;
const H = 247;
/** The seam at its narrowest, at the top, in emblem units. */
const SEAM = 8.4;

/** How far to push each half out so the seam stays about a pixel wide. Nothing from 32 px up;
 *  below that the seam would drop under a pixel and close up. */
function spread(height: number) {
  return height > 32 ? 0 : Math.max(0, ((1.1 * H) / height - SEAM) / 2);
}

/** The bare emblem in the text colour. Everywhere but the app icon, which adds the dark plate. */
export function Emblem({ height, className }: { height: number; className?: string }) {
  const sp = spread(height);
  const width = W + 2 * sp;
  return (
    <svg className={className} width={(height * width) / H} height={height} viewBox={`${-sp} 0 ${width} ${H}`} fill="currentColor" aria-hidden>
      <path fillRule="evenodd" transform={`translate(${-sp} 0)`} d={LEFT} />
      <path fillRule="evenodd" transform={`translate(${sp} 0)`} d={RIGHT} />
    </svg>
  );
}

const useStyles = makeStyles({
  // The emblem stands in for the h, sitting on the baseline at the h's height.
  wordmark: {
    display: "inline-flex",
    alignItems: "baseline",
    gap: "0.06em",
    fontFamily: '"Segoe UI Variable Display", "Segoe UI", sans-serif',
    fontWeight: 600,
    letterSpacing: "-0.02em",
    lineHeight: 1,
  },
});

/** The name, written with the emblem as its h. `size` is the font size in pixels. */
export function Wordmark({ size, className }: { size: number; className?: string }) {
  const s = useStyles();
  return (
    <span className={mergeClasses(s.wordmark, className)} style={{ fontSize: size }} role="img" aria-label="hrtz">
      {/* The h of Segoe UI Variable Display stands 0.75 em tall. */}
      <Emblem height={Math.round(size * 0.75)} />
      <span aria-hidden>rtz</span>
    </span>
  );
}
