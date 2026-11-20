import { num, t } from "./i18n";

/** 8000 → "8,000 Hz". */
export const hzText = (hz: number) => t("{{hz}} Hz", { hz: num(hz) });
