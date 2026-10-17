//! Games the pad map knows by name, and the mouse sensitivity in their own settings files.
//!
//! The files are only read, never written. Each read looks at the file as it is now, so a change
//! made in the game shows up the next time the map asks.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub struct Game {
    pub name: &'static str,
    exes: &'static [&'static str],
    /// Degrees turned per count at sensitivity 1, where it's a plain multiple.
    yaw: Option<f64>,
    read: Option<fn() -> Option<Found>>,
}

struct Found {
    sens: f64,
    acceleration: bool,
    file: PathBuf,
}

const GAMES: &[Game] = &[
    Game { name: "Valorant", exes: &["valorant-win64-shipping.exe"], yaw: Some(0.07), read: Some(valorant) },
    Game { name: "Apex Legends", exes: &["r5apex.exe", "r5apex_dx12.exe"], yaw: Some(0.022), read: Some(apex) },
    Game { name: "Counter-Strike 2", exes: &["cs2.exe"], yaw: Some(0.022), read: None },
    Game { name: "Overwatch 2", exes: &["overwatch.exe"], yaw: Some(0.0066), read: None },
    Game { name: "Fortnite", exes: &["fortniteclient-win64-shipping.exe"], yaw: None, read: None },
    Game { name: "Rainbow Six Siege", exes: &["rainbowsix.exe", "rainbowsix_vulkan.exe"], yaw: None, read: None },
];

pub fn find(exe: &str) -> Option<&'static Game> {
    GAMES.iter().find(|g| g.exes.iter().any(|e| e.eq_ignore_ascii_case(exe)))
}

/// A game's sensitivity as its settings file has it now.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Sensitivity {
    pub game: &'static str,
    pub sens: f64,
    /// Degrees turned per count at sensitivity 1.
    pub yaw: f64,
    /// Mouse acceleration is on in the game, so a turn's length depends on speed.
    pub acceleration: bool,
    /// Where it was read from.
    pub file: String,
}

pub fn sensitivity(exe: &str) -> Option<Sensitivity> {
    let game = find(exe)?;
    let found = (game.read?)()?;
    Some(Sensitivity {
        game: game.name,
        sens: found.sens,
        yaw: game.yaw?,
        acceleration: found.acceleration,
        file: found.file.to_string_lossy().into_owned(),
    })
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// `%USERPROFILE%\Saved Games\Respawn\Apex\local\settings.cfg`
fn apex() -> Option<Found> {
    let file = env_path("USERPROFILE")?.join(r"Saved Games\Respawn\Apex\local\settings.cfg");
    let (sens, acceleration) = parse_apex(&fs::read_to_string(&file).ok()?)?;
    Some(Found { sens, acceleration, file })
}

/// Lines like `mouse_sensitivity "0.85"`.
fn parse_apex(text: &str) -> Option<(f64, bool)> {
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (k, v) = line.trim().split_once(char::is_whitespace)?;
            (k == key).then(|| v.trim().trim_matches('"').parse::<f64>().ok()).flatten()
        })
    };
    let sens = value("mouse_sensitivity").filter(|s| *s > 0.0)?;
    Some((sens, value("m_acceleration").is_some_and(|a| a != 0.0)))
}

/// `%LOCALAPPDATA%\VALORANT\Saved\Config\<account>-<region>\Windows\RiotUserSettings.ini`, one folder
/// per Riot account: the one played last is the one changed last.
fn valorant() -> Option<Found> {
    let root = env_path("LOCALAPPDATA")?.join(r"VALORANT\Saved\Config");
    let file = fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|e| e.path().join(r"Windows\RiotUserSettings.ini"))
        .filter_map(|p| Some((modified(&p)?, p)))
        .max_by_key(|(t, _)| *t)?
        .1;
    let sens = parse_valorant(&fs::read_to_string(&file).ok()?)?;
    Some(Found { sens, acceleration: false, file })
}

/// `EAresFloatSettingName::MouseSensitivity=0.144999996`
fn parse_valorant(text: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let v = line.trim().strip_prefix("EAresFloatSettingName::MouseSensitivity=")?;
        // Stored as a 32-bit float: 0.145 comes back as 0.144999996.
        let sens = v.trim().parse::<f32>().ok()?;
        (sens > 0.0).then(|| format!("{sens}").parse().unwrap_or(sens as f64))
    })
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(p).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_apex() {
        let text = "m_acceleration \"0\"\nmouse_sensitivity \"0.85\"\nmouse_zoomed_sensitivity_scalar_0 \"0.8\"\n";
        assert_eq!(parse_apex(text), Some((0.85, false)));
        assert_eq!(parse_apex("mouse_sensitivity \"2.5\"\nm_acceleration \"1\""), Some((2.5, true)));
        assert_eq!(parse_apex("mouse_sensitivity_x \"2\""), None);
    }

    #[test]
    fn reads_valorant() {
        let text = "[Settings]\nEAresFloatSettingName::MouseSensitivityADS=1.00047016\nEAresFloatSettingName::MouseSensitivity=0.144999996\n";
        assert_eq!(parse_valorant(text), Some(0.145));
        assert_eq!(parse_valorant("[Settings]\n"), None);
    }

    /// What this PC's own game files say. `cargo test --lib games -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_this_pcs_games() {
        for exe in ["VALORANT-Win64-Shipping.exe", "r5apex_dx12.exe", "cs2.exe"] {
            eprintln!("{exe}: {:?}", sensitivity(exe));
        }
    }

    #[test]
    fn knows_games_by_exe() {
        assert_eq!(find("VALORANT-Win64-Shipping.exe").map(|g| g.name), Some("Valorant"));
        assert_eq!(find("r5apex_dx12.exe").map(|g| g.name), Some("Apex Legends"));
        assert!(find("notepad.exe").is_none());
    }
}
