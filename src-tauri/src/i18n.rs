//! The text hrtz shows from here (the tray menu, its notifications and the DPI pop-up) in the
//! language the window shows (`src/i18n.ts`): the one picked in Settings, else Windows' display
//! language, else English. It reads the window's own files (`src/locales`), keyed by the English,
//! so each line is translated once for both. Each key used here is listed in
//! `src/locales/backend.ts`, which keeps it in those files when `pnpm strings` gathers them;
//! `every_key_is_listed` checks that.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The languages, as `src/i18n.ts` lists them; English is the keys themselves.
const LANGUAGES: [&str; 7] = ["en", "de", "es", "pt-BR", "ru", "uk", "zh-CN"];
const FILES: [(&str, &str); 6] = [
    ("de", include_str!("../../src/locales/de.json")),
    ("es", include_str!("../../src/locales/es.json")),
    ("pt-BR", include_str!("../../src/locales/pt-BR.json")),
    ("ru", include_str!("../../src/locales/ru.json")),
    ("uk", include_str!("../../src/locales/uk.json")),
    ("zh-CN", include_str!("../../src/locales/zh-CN.json")),
];

/// The language shown, as its place in `LANGUAGES`: English until `set_language`.
static SHOWN: AtomicUsize = AtomicUsize::new(0);

fn table(language: &str) -> Option<&'static HashMap<String, String>> {
    static TABLES: OnceLock<HashMap<&'static str, HashMap<String, String>>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            FILES.iter().map(|(code, json)| (*code, serde_json::from_str(json).unwrap_or_default())).collect()
        })
        .get(language)
}

/// Shows `choice` ("uk"), the language picked in Settings; with none (or one hrtz no longer has),
/// Windows' display language if hrtz has it, else English.
pub fn set_language(choice: Option<&str>) {
    let known = |code: &str| LANGUAGES.iter().position(|l| *l == code);
    let i = choice.and_then(known).or_else(|| windows_language().and_then(known)).unwrap_or(0);
    SHOWN.store(i, Ordering::Relaxed);
}

/// The language shown: "en", "uk", "zh-CN", …
pub fn language() -> &'static str {
    LANGUAGES[SHOWN.load(Ordering::Relaxed)]
}

/// Windows' display language, as the window's WebView2 takes it for `navigator.language`: by its
/// primary language, any Portuguese as Brazil's, and Chinese only the Simplified (China's and
/// Singapore's sublanguages).
#[cfg(windows)]
fn windows_language() -> Option<&'static str> {
    // SAFETY: takes nothing and only reads the user's setting.
    language_of(unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() })
}

/// A Windows language ID's language, if hrtz has it.
fn language_of(id: u16) -> Option<&'static str> {
    match (id & 0x3ff, id >> 10) {
        (0x07, _) => Some("de"),
        (0x0a, _) => Some("es"),
        (0x16, _) => Some("pt-BR"),
        (0x19, _) => Some("ru"),
        (0x22, _) => Some("uk"),
        (0x04, 0x02 | 0x04) => Some("zh-CN"),
        _ => None,
    }
}

#[cfg(not(windows))]
fn windows_language() -> Option<&'static str> {
    None
}

/// `key` in the language shown; English (the key) where it isn't translated yet.
pub fn tr(key: &str) -> String {
    tr_with(key, &[])
}

/// `tr` with its values filled in: `tr_with("Mouse battery at {{level}}%", &[("level", "15")])`.
pub fn tr_with(key: &str, values: &[(&str, &str)]) -> String {
    let text = table(language()).and_then(|t| t.get(key)).filter(|s| !s.is_empty()).map_or(key, String::as_str);
    values.iter().fold(text.to_string(), |s, (name, value)| s.replace(&format!("{{{{{name}}}}}"), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_languages_map_as_the_window_maps_them() {
        assert_eq!(language_of(0x0804), Some("zh-CN"), "China");
        assert_eq!(language_of(0x1004), Some("zh-CN"), "Singapore");
        assert_eq!(language_of(0x0404), None, "Taiwan's is Traditional");
        assert_eq!(language_of(0x0416), Some("pt-BR"));
        assert_eq!(language_of(0x0816), Some("pt-BR"), "Portugal's too");
        assert_eq!(language_of(0x0c0a), Some("es"));
        assert_eq!(language_of(0x0407), Some("de"));
        assert_eq!(language_of(0x0419), Some("ru"));
        assert_eq!(language_of(0x0422), Some("uk"));
        assert_eq!(language_of(0x0409), None, "English is the keys");
    }

    /// Every `tr("…")` and `tr_with("…"` in the Rust code is listed in `src/locales/backend.ts`,
    /// so `pnpm strings` keeps it in the language files.
    #[test]
    fn every_key_is_listed() {
        let listed =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/locales/backend.ts")).unwrap();
        let mut missing = Vec::new();
        let mut files = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("i18n.rs") {
                continue;
            }
            let code = std::fs::read_to_string(&path).unwrap();
            for call in ["tr(", "tr_with("] {
                for (at, _) in code.match_indices(call) {
                    // Not the end of another name ("attr(").
                    if code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    // The key, also when rustfmt has put a long one on the next line.
                    let Some(rest) = code[at + call.len()..].trim_start().strip_prefix('"') else { continue };
                    let key = &rest[..rest.find('"').unwrap()];
                    if !listed.contains(&format!("msg(\"{key}\")")) {
                        missing.push(format!("{}: {key}", path.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "add these to src/locales/backend.ts:\n{}", missing.join("\n"));
    }

    #[test]
    fn untranslated_is_english_and_values_are_filled_in() {
        assert_eq!(tr_with("Mouse battery at {{level}}%", &[("level", "15")]), "Mouse battery at 15%");
        assert_eq!(tr("A line no file has"), "A line no file has");
    }
}
