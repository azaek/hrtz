//! Games installed through Steam, Epic and Riot, for the Profiles page's app picker.
//!
//! Only the launchers' own records are read (Steam's library list and app manifests, Epic's
//! install manifests, Riot's product settings), plus a look inside each game's folder for the exe
//! that actually runs: the one a per-app rule has to match.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGame {
    /// The launcher's name for it; empty when it has none (the caller names it from the exe).
    pub name: String,
    pub exe: String,
    pub path: String,
    /// "Steam", "Epic" or "Riot".
    pub source: &'static str,
}

/// Every game found, one per exe, sorted by name. `steam` is Steam's folder, from the registry.
pub fn installed_games(steam: Option<PathBuf>, program_data: &Path) -> Vec<InstalledGame> {
    let mut all = Vec::new();
    if let Some(root) = steam {
        all.extend(steam_games(&root));
    }
    all.extend(epic_games(&program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests")));
    all.extend(riot_games(&program_data.join(r"Riot Games\Metadata")));
    let mut seen = HashSet::new();
    all.retain(|g| seen.insert(g.exe.to_lowercase()));
    all.sort_by_key(|g| g.name.to_lowercase());
    all
}

// ---- Steam ------------------------------------------------------------------------------------

fn steam_games(root: &Path) -> Vec<InstalledGame> {
    let mut libraries = vec![root.to_path_buf()];
    if let Ok(text) = fs::read_to_string(root.join(r"steamapps\libraryfolders.vdf")) {
        libraries.extend(vdf_values(&text, "path").into_iter().map(PathBuf::from));
    }
    let mut seen = HashSet::new();
    libraries.retain(|l| {
        seen.insert(l.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').to_string())
    });

    let mut out = Vec::new();
    for lib in libraries {
        let Ok(entries) = fs::read_dir(lib.join("steamapps")) else { continue };
        for e in entries.flatten() {
            let file = e.file_name().to_string_lossy().to_lowercase();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            let (Some(name), Some(dir)) =
                (vdf_values(&text, "name").into_iter().next(), vdf_values(&text, "installdir").into_iter().next())
            else {
                continue;
            };
            if is_steam_tool(&name) {
                continue;
            }
            if let Some(exe) = main_exe(&lib.join(r"steamapps\common").join(&dir)) {
                out.push(game(name, &exe, "Steam"));
            }
        }
    }
    out
}

/// Steam installs some things that aren't games.
fn is_steam_tool(name: &str) -> bool {
    let n = name.to_lowercase();
    ["redistributable", "steamworks", "steamvr", "proton", "dedicated server", "sdk"].iter().any(|t| n.contains(t))
}

/// Every value of `key` in a Valve KeyValues text file (`"key"  "value"` lines), unescaped.
pub fn vdf_values(text: &str, key: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let tokens = quoted(line);
            (tokens.len() == 2 && tokens[0].eq_ignore_ascii_case(key)).then(|| tokens[1].clone())
        })
        .collect()
}

fn quoted(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => s.extend(chars.next()),
                '"' => break,
                _ => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

// ---- Epic -------------------------------------------------------------------------------------

fn epic_games(manifests: &Path) -> Vec<InstalledGame> {
    let Ok(entries) = fs::read_dir(manifests) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        if !e.file_name().to_string_lossy().to_lowercase().ends_with(".item") {
            continue;
        }
        let Some(m) =
            fs::read_to_string(e.path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        else {
            continue;
        };
        let is_game = m["AppCategories"].as_array().is_some_and(|c| c.iter().any(|x| x.as_str() == Some("games")));
        if !is_game || m["bIsIncompleteInstall"].as_bool() == Some(true) {
            continue;
        }
        let (Some(name), Some(dir)) = (m["DisplayName"].as_str(), m["InstallLocation"].as_str()) else { continue };
        if let Some(exe) = main_exe(Path::new(dir)) {
            out.push(game(name.to_string(), &exe, "Epic"));
        }
    }
    out
}

// ---- Riot -------------------------------------------------------------------------------------

fn riot_games(metadata: &Path) -> Vec<InstalledGame> {
    let Ok(entries) = fs::read_dir(metadata) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let product = e.file_name().to_string_lossy().to_string();
        // Live installs only: not the client, test servers or patch data.
        if !product.ends_with(".live") {
            continue;
        }
        let Ok(text) = fs::read_to_string(e.path().join(format!("{product}.product_settings.yaml"))) else { continue };
        let Some(dir) = yaml_value(&text, "product_install_full_path") else { continue };
        let name = match product.as_str() {
            "valorant.live" => "VALORANT",
            "league_of_legends.live" => "League of Legends",
            _ => "",
        };
        // Riot writes the path with forward slashes.
        if let Some(exe) = main_exe(Path::new(&dir.replace('/', "\\"))) {
            out.push(game(name.to_string(), &exe, "Riot"));
        }
    }
    out
}

/// A top-level `key: "value"` from a YAML file.
fn yaml_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix(':').map(|v| v.trim().trim_matches('"').to_string()))
}

// ---- The exe that runs ------------------------------------------------------------------------

/// Exes that run a game but aren't in `games.rs` (which would put them on the pad map's list).
const KNOWN_EXES: &[&str] = &["league of legends.exe"];

/// Folders that never hold the game's own exe, and are big.
const SKIP_DIRS: &[&str] = &[
    "engine",
    "_commonredist",
    "commonredist",
    "redist",
    "__installer",
    "directx",
    "support",
    "easyanticheat",
    "battleye",
    "content",
    "paks",
    "movies",
    "localization",
];

/// Parts of an exe's name that mark a helper rather than the game.
const HELPERS: &[&str] = &[
    "unins",
    "setup",
    "install",
    "redist",
    "crash",
    "report",
    "helper",
    "updat",
    "easyanticheat",
    "beservice",
    "battleye",
    "dxsetup",
    "prereq",
    "cefprocess",
    "webhelper",
    "overlay",
    "start_protected_game",
    "bootstrap",
    "launcher",
];

/// The exe in a game's folder that runs the game:
/// 1. one `games.rs` (or `KNOWN_EXES`) knows by name;
/// 2. Unreal Engine's `*-Shipping.exe`, which the small exe at the top only starts;
/// 3. a Unity exe, which sits beside its `<name>_Data` folder;
/// 4. otherwise the largest exe near the top that isn't an installer, updater or crash reporter.
pub fn main_exe(dir: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    let mut budget = 6000;
    walk(dir, 0, &mut found, &mut budget);
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();

    if let Some((p, ..)) =
        found.iter().find(|(p, ..)| crate::games::find(&name(p)).is_some() || KNOWN_EXES.contains(&name(p).as_str()))
    {
        return Some(p.clone());
    }
    let largest = |items: Vec<&(PathBuf, u64, usize)>| {
        items.into_iter().max_by_key(|(_, size, _)| *size).map(|(p, ..)| p.clone())
    };
    let shipping = found.iter().filter(|(p, ..)| name(p).ends_with("-shipping.exe")).collect::<Vec<_>>();
    if !shipping.is_empty() {
        return largest(shipping);
    }
    let unity = found
        .iter()
        .filter(|(p, _, depth)| {
            *depth <= 1
                && p.file_stem()
                    .is_some_and(|stem| p.with_file_name(format!("{}_Data", stem.to_string_lossy())).is_dir())
        })
        .collect::<Vec<_>>();
    if !unity.is_empty() {
        return largest(unity);
    }
    largest(found.iter().filter(|(p, _, depth)| *depth <= 2 && !HELPERS.iter().any(|h| name(p).contains(h))).collect())
}

/// Exes under `dir` with their size and depth, five folders deep at most, skipping folders that
/// only hold assets or third-party installers.
fn walk(dir: &Path, depth: usize, found: &mut Vec<(PathBuf, u64, usize)>, budget: &mut usize) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(kind) = e.file_type() else { continue };
        let name = e.file_name().to_string_lossy().to_lowercase();
        if kind.is_dir() {
            if depth < 5 && !SKIP_DIRS.contains(&name.as_str()) && !name.ends_with("_data") {
                walk(&e.path(), depth + 1, found, budget);
            }
        } else if name.ends_with(".exe") {
            found.push((e.path(), e.metadata().map(|m| m.len()).unwrap_or(0), depth));
        }
    }
}

fn game(name: String, exe: &Path, source: &'static str) -> InstalledGame {
    InstalledGame {
        name,
        exe: exe.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        path: exe.to_string_lossy().to_string(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steam_library_folders_and_manifests() {
        let folders = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        assert_eq!(vdf_values(folders, "path"), vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
        let manifest = "\"AppState\"\n{\n\t\"name\"\t\t\"Liar's Bar\"\n\t\"installdir\"\t\t\"Liar's Bar\"\n}";
        assert_eq!(vdf_values(manifest, "name"), vec!["Liar's Bar"]);
        assert!(is_steam_tool("Steamworks Common Redistributables"));
        assert!(!is_steam_tool("Apex Legends"));
    }

    #[test]
    fn reads_riot_install_path() {
        let yaml = "locale_data:\n    available_locales:\nproduct_install_full_path: \"C:/Riot Games/VALORANT/live\"\nproduct_install_root: \"C:/Riot Games/\"";
        assert_eq!(yaml_value(yaml, "product_install_full_path").as_deref(), Some("C:/Riot Games/VALORANT/live"));
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "reads this PC's launchers; run with --ignored --nocapture"]
    fn lists_this_pcs_games() {
        let program_data = PathBuf::from(std::env::var("ProgramData").unwrap());
        for g in installed_games(crate::win::steam_path(), &program_data) {
            println!("{:<6} {:<28} {}", g.source, g.name, g.path);
        }
    }

    #[test]
    fn picks_the_exe_that_runs_the_game() {
        let root = std::env::temp_dir().join(format!("hrtz-installed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let put = |rel: &str, bytes: usize| {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, vec![0u8; bytes]).unwrap();
        };
        // Unreal: the shipping build, not the small starter at the top or the crash reporter.
        put(r"unreal\Marvel.exe", 10);
        put(r"unreal\MarvelGame\Marvel\Binaries\Win64\Marvel-Win64-Shipping.exe", 100);
        put(r"unreal\Engine\Binaries\Win64\CrashReportClient.exe", 500);
        assert_eq!(main_exe(&root.join("unreal")).unwrap().file_name().unwrap(), "Marvel-Win64-Shipping.exe");
        // Unity: the exe beside its _Data folder, not the crash handler.
        put(r"unity\Muck.exe", 10);
        put(r"unity\Muck_Data\level0", 1);
        put(r"unity\UnityCrashHandler64.exe", 900);
        assert_eq!(main_exe(&root.join("unity")).unwrap().file_name().unwrap(), "Muck.exe");
        // Known by name.
        put(r"apex\r5apex.exe", 10);
        put(r"apex\EasyAntiCheat_EOS_Setup.exe", 900);
        assert_eq!(main_exe(&root.join("apex")).unwrap().file_name().unwrap(), "r5apex.exe");
        // Otherwise the largest that isn't a helper.
        put(r"other\Game.exe", 50);
        put(r"other\unins000.exe", 900);
        assert_eq!(main_exe(&root.join("other")).unwrap().file_name().unwrap(), "Game.exe");
        let _ = fs::remove_dir_all(&root);
    }
}
