export const DAY = 86_400;

export const unixNow = () => Math.floor(Date.now() / 1000);
/** 0.4 → "24 min", 7.6 → "8 h". */
export const hours = (h: number) => (h < 1.5 ? `${Math.round(h * 60)} min` : `${Math.round(h)} h`);
/** 1.2 → "29 hours", 5.12 → "5.1 days". */
export const days = (d: number) => (d < 1.5 ? `${Math.round(d * 24)} hours` : `${d < 10 ? d.toFixed(1).replace(/\.0$/, "") : Math.round(d)} days`);
/** Unix seconds → "15 Sept". */
export const date = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short" });
