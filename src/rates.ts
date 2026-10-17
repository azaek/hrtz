/** A distinct colour for each polling rate, to tell them apart in the battery charts. (Not the
 *  receiver's light: the 8K receiver's stays green, and turns red for low battery.) */
const RATE_COLOR: Record<number, string | undefined> = {
  125: "#e5484d",
  250: "#3e63dd",
  500: "#f5d90a",
  1000: "#05a2c2",
  2000: "#8e4ec6",
  4000: "#30a46c",
};

/** The rate's chart colour; 8,000 Hz and any other rate get a neutral one. */
export const rateColor = (hz: number) => RATE_COLOR[hz] ?? "#b4b4b4";

/** 8000 → "8,000 Hz". */
export const hzText = (hz: number) => `${hz.toLocaleString("en-US")} Hz`;
