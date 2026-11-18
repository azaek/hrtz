import { defineConfig } from "i18next-cli";

// `pnpm strings` gathers the text the app shows (`t("…")`, `msg("…")`, `<Trans>`) into
// src/locales: English filled in from the code, the other languages left empty until translated.
export default defineConfig({
  locales: ["en", "uk", "ru"],
  extract: {
    input: ["src/**/*.{ts,tsx}"],
    ignore: ["src/lab/**", "src/components/ui/**", "src/i18n.ts"],
    output: "src/locales/{{language}}.json",
    defaultNS: false,
    keySeparator: false,
    nsSeparator: false,
    functions: ["t", "msg"],
    primaryLanguage: "en",
    extractFromComments: false,
    removeUnusedKeys: true,
    sort: true,
  },
});
