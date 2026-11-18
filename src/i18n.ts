import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import ru from "./locales/ru.json";
import uk from "./locales/uk.json";

/**
 * The languages hrtz shows its text in, each named in itself. The English is the text in the code,
 * and each key is that English: `t("Polling rate")`. A line not translated yet shows in English.
 * `pnpm strings` gathers the keys into `src/locales`, where each language fills in its own.
 */
export const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "uk", name: "Українська" },
  { code: "ru", name: "Русский" },
] as const;
export type Language = (typeof LANGUAGES)[number]["code"];

/** The language picked in Settings, kept here too, so every window (the OSD's too) starts in it. */
const KEY = "hrtz.language";

/** Windows' display language (WebView2's `navigator.language`), if hrtz has it; English if not.
 *  The tray and notifications take the same one (`src-tauri/src/i18n.rs`). */
export function windowsLanguage(): Language {
  return LANGUAGES.find((l) => l.code === navigator.language.toLowerCase().split("-")[0])?.code ?? "en";
}

/** The language to show for a choice in Settings: null (or one hrtz no longer has) is Windows'. */
export const languageFor = (choice: string | null | undefined): Language => LANGUAGES.find((l) => l.code === choice)?.code ?? windowsLanguage();

function stored(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, uk: { translation: uk }, ru: { translation: ru } },
  lng: languageFor(stored()),
  fallbackLng: "en",
  // The keys are English sentences, with their own dots and colons.
  keySeparator: false,
  nsSeparator: false,
  // A line left empty in a language's file isn't translated yet: it shows in English.
  returnEmptyString: false,
  // React escapes the text itself.
  interpolation: { escapeValue: false },
  initAsync: false,
});
document.documentElement.lang = i18n.language;
i18n.on("languageChanged", (lng) => (document.documentElement.lang = lng));

/** Shows the language picked in Settings (null: Windows'), in this window and the others. */
export function setLanguage(choice: string | null | undefined) {
  try {
    if (choice) localStorage.setItem(KEY, choice);
    else localStorage.removeItem(KEY);
  } catch {
    // Without storage, the other windows start in Windows' language.
  }
  const lng = languageFor(choice);
  if (i18n.language !== lng) void i18n.changeLanguage(lng);
}
// Another window changed it.
window.addEventListener("storage", (e) => {
  if (e.key === KEY) void i18n.changeLanguage(languageFor(e.newValue));
});

/**
 * The text in the language shown: `t("Polling rate")`, `t("At {{dpi}} DPI", { dpi: num(dpi) })`,
 * and for a count, `t("{{count}} presses", { count, defaultValue_one: "{{count}} press" })`.
 * Called where the text is shown, never when a module loads (the language can change after).
 */
export const t = i18n.getFixedT(null, "translation");

/**
 * Marks text that a table holds at load for `pnpm strings` to find, unchanged; it's translated
 * where it's shown, with `t(label)`. Prefer building the table in a function that calls `t`.
 */
export const msg = <S extends string>(text: S): S => text;

/** A number as the language writes it: 1,600 in English, 1 600 in Ukrainian and Russian. */
export const num = (n: number, options?: Intl.NumberFormatOptions) => n.toLocaleString(i18n.language, options);

/** Where a date or time follows Windows' regional format ("15 Oct" in English in India): that
 *  format while Windows is in the language shown, else the language's usual one. */
export const dateLocale = () => (navigator.language.toLowerCase().split("-")[0] === i18n.language ? navigator.language : i18n.language);

/** A line from its English, which is its key, and the values it holds: `t`'s shape. */
export type Say = (key: string, values?: Record<string, unknown>) => string;

/**
 * How a helper that names something says it: on screen in the language shown (`screen`, the
 * default), or in English (`english`) for what we read, the mouse check's report and its issue.
 */
export interface Speech {
  t: Say;
  num: (n: number, options?: Intl.NumberFormatOptions) => string;
}
export const screen: Speech = { t: (key, values) => t(key, values), num };
/** `t` in English, by itself: the key with its values filled in (and `defaultValue_one` for a
 *  count of one), so the report's English never depends on the language files. */
export const english: Speech = {
  t: (key, values = {}) =>
    (values.count === 1 && typeof values.defaultValue_one === "string" ? values.defaultValue_one : key).replace(/{{(\w+)}}/g, (m, k: string) => (k in values ? String(values[k]) : m)),
  num: (n, options) => n.toLocaleString("en-US", options),
};

export default i18n;
