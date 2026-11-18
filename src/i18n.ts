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

/** The first of Windows' languages that hrtz speaks; English if none. */
export function windowsLanguage(): Language {
  for (const tag of navigator.languages?.length ? navigator.languages : [navigator.language]) {
    const hit = LANGUAGES.find((l) => l.code === tag.toLowerCase().split("-")[0]);
    if (hit) return hit.code;
  }
  return "en";
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

export default i18n;
