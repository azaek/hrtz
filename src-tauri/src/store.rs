//! Preferences, per-app rules and flash backups on disk.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::mice::compx::protocol::FLASH_SIZE;
use crate::mice::compx::{image, models};
use crate::mice::model::{ButtonAction, Settings};
use crate::tray::TrayStyle;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    /// Executable file name, matched case-insensitively (e.g. `cs2.exe`).
    pub exe: String,
    /// Profile index 0-3.
    pub profile: u8,
    /// Friendly name and full path, kept for display when the app isn't running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A game the pad map records in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PadGame {
    /// Executable file name, matched case-insensitively.
    pub exe: String,
    pub name: String,
    /// Full path, for the icon.
    #[serde(default)]
    pub path: String,
}

/// What a profile held the last time it was in use. The mouse only reports the profile in use, so
/// the Profiles page shows the others from these.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    /// The model it was read from, so another mouse's profiles aren't shown as this one's.
    pub mid: u8,
    /// DPI of the stage in use, and how many stages the DPI button cycles.
    pub dpi: u32,
    pub stages: u8,
    pub polling_hz: u32,
    /// What each button that exists does, by flash index, to tell which were changed.
    pub buttons: Vec<ButtonAction>,
}

impl ProfileSnapshot {
    /// `physical` is the flash indices of the buttons the model has. None for a mouse without DPI
    /// stages or a polling rate, which has nothing for the Profiles page to sum up.
    pub fn of(mid: u8, s: &Settings, physical: &[(u8, String)]) -> Option<Self> {
        let n = physical.iter().map(|(i, _)| *i as usize + 1).max().unwrap_or(0);
        Some(Self {
            mid,
            dpi: s.dpi()?,
            stages: s.dpi_stage_count?,
            polling_hz: s.polling_hz?,
            buttons: s.buttons.as_deref().unwrap_or(&[]).iter().take(n).cloned().collect(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub battery_alerts: bool,
    pub low_battery_percent: u8,
    pub osd: bool,
    pub tray_style: TrayStyle,
    /// Track where the mouse moves on the pad, in the background too.
    pub pad_tracking: bool,
    /// Count double clicks over days, in the background too (on unless turned off).
    pub click_history: bool,
    /// The games the pad map records in; nothing else is recorded. Known games join on their own
    /// the first time they're played.
    pub pad_games: Vec<PadGame>,
    /// Known games taken off the list, so they don't join again (exe names in lower case).
    pub pad_games_removed: Vec<String>,
    /// The first-run window has been through (it sets startup, alerts and the saver).
    pub welcomed: bool,
    /// Agreed to share anonymous data that tunes the algorithms (lift detection, battery
    /// estimates). Opt-in; nothing is sent until sharing exists, and it shows what goes first.
    pub share_data: bool,
    /// The in-game HUD (see `hud`).
    pub hud: HudPrefs,
}

/// What hrtz keeps for one mouse: its profiles' names and what they held, the apps that switch
/// them, and its battery saver. Filed in `mice.json` under the mouse's key (`KnownMice`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MousePrefs {
    pub rules_enabled: bool,
    /// Profile to return to when no rule matches; `None` leaves the profile alone.
    pub default_profile: Option<u8>,
    pub rules: Vec<AppRule>,
    pub profile_names: Vec<String>,
    /// What each profile held when last in use (see `ProfileSnapshot`).
    pub profile_snapshots: Vec<Option<ProfileSnapshot>>,
    /// Lower the polling rate to `battery_saver_hz` at `battery_saver_below` percent, until charging.
    pub battery_saver: bool,
    pub battery_saver_below: u8,
    pub battery_saver_hz: u32,
    /// The rate to go back to, while the saver has it lowered.
    pub battery_saver_restore: Option<u32>,
}

impl Default for MousePrefs {
    fn default() -> Self {
        Self {
            rules_enabled: true,
            default_profile: None,
            rules: Vec::new(),
            profile_names: (1..=4).map(|i| format!("Profile {i}")).collect(),
            profile_snapshots: vec![None; 4],
            battery_saver: false,
            battery_saver_below: 20,
            battery_saver_hz: 1000,
            battery_saver_restore: None,
        }
    }
}

impl MousePrefs {
    /// Its fields as they were in `prefs.json`, from before several mice.
    const FIELDS: [&str; 9] = [
        "rulesEnabled",
        "defaultProfile",
        "rules",
        "profileNames",
        "profileSnapshots",
        "batterySaver",
        "batterySaverBelow",
        "batterySaverHz",
        "batterySaverRestore",
    ];

    /// Profile `index`'s name: the one given, else "Profile 2".
    pub fn profile_name(&self, index: u8) -> String {
        self.profile_names.get(index as usize).cloned().unwrap_or_else(|| format!("Profile {}", index + 1))
    }

    /// Make room for `count` profiles (the Zaopins have 4, the Superlights 5): a name and a snapshot
    /// slot each. Returns whether anything was added; nothing is taken away.
    pub fn fit_profiles(&mut self, count: u8) -> bool {
        let n = count as usize;
        let grow = self.profile_names.len() < n || self.profile_snapshots.len() < n;
        while self.profile_names.len() < n {
            self.profile_names.push(format!("Profile {}", self.profile_names.len() + 1));
        }
        if self.profile_snapshots.len() < n {
            self.profile_snapshots.resize(n, None);
        }
        grow
    }

    /// Remember what profile `index` holds while it's in use. Returns whether that changed.
    pub fn note_profile(&mut self, index: u8, snap: ProfileSnapshot) -> bool {
        let Some(slot) = self.profile_snapshots.get_mut(index as usize) else { return false };
        if slot.as_ref() == Some(&snap) {
            return false;
        }
        *slot = Some(snap);
        true
    }

    /// Fill in the name and path of a rule typed in by hand, the first time its app is seen, so it
    /// gets its icon and real name. Returns whether a rule changed.
    pub fn fill_rule(&mut self, exe: &str, name: String, path: &str) -> bool {
        let Some(r) = self.rules.iter_mut().find(|r| r.exe.eq_ignore_ascii_case(exe) && r.path.is_none()) else {
            return false;
        };
        r.path = Some(path.to_string());
        r.name.get_or_insert(name);
        true
    }

    /// What the battery saver should do now: the polling rate to set, if any, and the rate to go
    /// back to afterwards. `None` when nothing changes.
    pub fn saver_step(&self, level: u8, charging: bool, hz: u32) -> Option<(Option<u32>, Option<u32>)> {
        let step = match self.battery_saver_restore {
            // Changed by hand (or with the mouse's button) while lowered: that choice stands.
            Some(_) if hz != self.battery_saver_hz => (None, None),
            Some(old) if charging || !self.battery_saver => (Some(old), None),
            Some(old) => (None, Some(old)),
            None if self.battery_saver
                && !charging
                && level <= self.battery_saver_below
                && hz > self.battery_saver_hz =>
            {
                (Some(self.battery_saver_hz), Some(hz))
            }
            None => (None, None),
        };
        (step.0.is_some() || step.1 != self.battery_saver_restore).then_some(step)
    }

    /// The profile for an app coming into focus. A rule for a game `games.rs` knows covers all its
    /// exes (Apex runs as r5apex.exe or r5apex_dx12.exe).
    pub fn profile_for(&self, exe: &str) -> Option<u8> {
        let game = crate::games::find(exe).map(|g| g.name);
        self.rules
            .iter()
            .find(|r| {
                r.exe.eq_ignore_ascii_case(exe)
                    || (game.is_some() && crate::games::find(&r.exe).map(|g| g.name) == game)
            })
            .map(|r| r.profile)
            .or(self.default_profile)
    }
}

/// When the in-game HUD shows its tabs.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HudShow {
    Off,
    /// A tab for 3 s after its value changes. A prefs file from when the HUD had a peek hotkey
    /// may say "hotkey"; that mode is gone, and this is the nearest to it.
    #[default]
    #[serde(alias = "hotkey")]
    Change,
    /// Every chosen tab while a game is in front.
    Always,
}

/// How big the tabs are, on top of the monitor's own scaling.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HudSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl HudSize {
    /// What everything the HUD draws is multiplied by, besides the monitor's scale.
    pub fn factor(self) -> f32 {
        match self {
            Self::Small => 0.85,
            Self::Medium => 1.0,
            Self::Large => 1.2,
        }
    }
}

/// The edge or corner of the screen the HUD sits against.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HudAnchor {
    TopLeft,
    Top,
    #[default]
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl HudAnchor {
    /// At the left or right edge the tabs stack; everywhere else they form a row.
    pub fn stacked(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    pub fn right(self) -> bool {
        matches!(self, Self::TopRight | Self::Right | Self::BottomRight)
    }

    pub fn left(self) -> bool {
        matches!(self, Self::TopLeft | Self::Left | Self::BottomLeft)
    }

    pub fn bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::Bottom | Self::BottomRight)
    }

    pub fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::Top | Self::TopRight)
    }
}

/// The values' colour: teal is Valorant's, white keeps to the app's greys.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HudValueColor {
    #[default]
    Teal,
    White,
}

/// Which facts the HUD has a tab for. Long range isn't here: it shows only while it's on. Nor is
/// hrtz's mark, which always leads.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct HudFields {
    pub battery: bool,
    /// Time left, inside the battery's tab.
    pub time_left: bool,
    pub polling: bool,
    pub dpi: bool,
    pub profile: bool,
    /// "Battery:" and the rest before the values. Without them each value says what it is by
    /// itself: "82%", "8K Hz", "800 DPI".
    pub labels: bool,
}

impl Default for HudFields {
    fn default() -> Self {
        Self { battery: true, time_left: true, polling: true, dpi: true, profile: false, labels: true }
    }
}

/// A game the HUD can't draw over, and why. Written by the backend when it sees it happen.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct HudBlocked {
    pub exe: String,
    pub name: String,
    /// "exclusive": the game runs in exclusive fullscreen, where no other window can draw.
    pub reason: String,
}

/// Unknown fields are skipped, not refused: a prefs file from when the HUD had a peek hotkey has
/// a "hotkey" object here, and it should still load.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HudPrefs {
    pub show: HudShow,
    pub fields: HudFields,
    pub anchor: HudAnchor,
    pub size: HudSize,
    /// The tabs' darkness, 0-100: black at `plate * 0.69 / 100`. Below 30 there's no tab and the
    /// text is outlined instead.
    #[serde(deserialize_with = "percent")]
    pub plate: u8,
    pub value_color: HudValueColor,
    /// In "always": dim after 10 s with nothing changing, and move a pixel or two every 3 minutes.
    pub oled_care: bool,
    /// Games the HUD stays off in (exe names in lower case).
    pub hidden_games: Vec<String>,
    pub blocked: Vec<HudBlocked>,
}

impl Default for HudPrefs {
    fn default() -> Self {
        Self {
            show: HudShow::default(),
            fields: HudFields::default(),
            anchor: HudAnchor::default(),
            size: HudSize::default(),
            plate: 90,
            value_color: HudValueColor::default(),
            oled_care: true,
            hidden_games: Vec::new(),
            blocked: Vec::new(),
        }
    }
}

impl HudPrefs {
    pub fn hidden(&self, exe: &str) -> bool {
        self.hidden_games.iter().any(|g| g.eq_ignore_ascii_case(exe))
    }

    /// Note that a game runs where the HUD can't draw. Returns whether the list changed.
    pub fn block(&mut self, exe: &str, name: String, reason: &str) -> bool {
        if self.blocked.iter().any(|b| b.exe.eq_ignore_ascii_case(exe) && b.reason == reason) {
            return false;
        }
        self.blocked.push(HudBlocked { exe: exe.to_string(), name, reason: reason.to_string() });
        true
    }
}

/// A 0-100 setting that a slider may send as a fraction, such as 37.5.
fn percent<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u8, D::Error> {
    let v = f64::deserialize(d)?;
    Ok(v.round().clamp(0.0, 100.0) as u8)
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            battery_alerts: true,
            low_battery_percent: 15,
            osd: true,
            tray_style: TrayStyle::default(),
            pad_tracking: false,
            click_history: true,
            pad_games: Vec::new(),
            pad_games_removed: Vec::new(),
            welcomed: false,
            share_data: false,
            hud: HudPrefs::default(),
        }
    }
}

impl Prefs {
    pub fn load(path: &Path) -> Self {
        Self::load_with_legacy(path).0
    }

    /// The prefs, and what a file from before several mice held for its one mouse (its rules,
    /// profile names and battery saver), if it held any: the first Zaopin to connect takes it.
    pub fn load_with_legacy(path: &Path) -> (Self, Option<MousePrefs>) {
        let Some(v) = fs::read_to_string(path).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        else {
            return (Self::default(), None);
        };
        let old = v.as_object().is_some_and(|o| MousePrefs::FIELDS.iter().any(|f| o.contains_key(*f)));
        let legacy = old.then(|| serde_json::from_value::<MousePrefs>(v.clone()).ok()).flatten();
        (Self::from_value(v), legacy)
    }

    fn from_value(v: serde_json::Value) -> Self {
        let mut v = v;
        if let Ok(p) = serde_json::from_value(v.clone()) {
            return p;
        }
        // HUD settings this version can't read (a newer version's, or edited by hand) cost only
        // the HUD's settings, not the rules and names besides them.
        if let Some(o) = v.as_object_mut() {
            o.remove("hud");
        }
        serde_json::from_value(v).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn pad_game(&self, exe: &str) -> Option<&PadGame> {
        self.pad_games.iter().find(|g| g.exe.eq_ignore_ascii_case(exe))
    }

    /// Put a known game on the pad list the first time it's played, unless it was taken off.
    /// Returns whether the list changed.
    pub fn join_pad_game(&mut self, exe: &str, name: String, path: String) -> bool {
        let removed = self.pad_games_removed.iter().any(|e| e.eq_ignore_ascii_case(exe));
        if removed || self.pad_game(exe).is_some() {
            return false;
        }
        self.pad_games.push(PadGame { exe: exe.to_string(), name, path });
        true
    }
}

/// A mouse hrtz has met, and what it keeps for it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KnownMouse {
    /// Stays the same across connections and restarts (`DeviceState::key`).
    pub key: String,
    pub model: String,
    /// When it was first and last connected, in seconds since 1970.
    pub first_seen: u64,
    pub last_seen: u64,
    #[serde(default)]
    pub prefs: MousePrefs,
    /// What it held the last time it was connected: what its pages show while it isn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<crate::mice::DeviceState>,
}

/// What `KnownMice::meet` found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Met {
    /// Met before.
    Again,
    /// Never met: a new mouse.
    New,
    /// Never met, and given the prefs kept from before several mice: not new to the user.
    TookOver,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MiceFile {
    mice: Vec<KnownMouse>,
    /// The one mouse's prefs from before several mice, until a Zaopin connects and takes them.
    legacy: Option<MousePrefs>,
}

/// The mice hrtz has met (`mice.json`), each with its prefs.
pub struct KnownMice {
    path: Option<PathBuf>,
    file: MiceFile,
}

impl KnownMice {
    pub fn load(path: Option<PathBuf>) -> Self {
        let file = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { path, file }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(&self.file)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn list(&self) -> &[KnownMouse] {
        &self.file.mice
    }

    /// The mouse's prefs, or the defaults for one not met yet.
    pub fn prefs(&self, key: &str) -> MousePrefs {
        self.file.mice.iter().find(|m| m.key == key).map(|m| m.prefs.clone()).unwrap_or_default()
    }

    /// The mouse seen most recently.
    pub fn last_seen(&self) -> Option<&KnownMouse> {
        self.file.mice.iter().max_by_key(|m| m.last_seen)
    }

    /// Hold the prefs of the one mouse from before several mice, until a Zaopin takes them. Only
    /// the first time: once mice are known, an old prefs file has nothing to give.
    pub fn hold_legacy(&mut self, legacy: MousePrefs) -> bool {
        if !self.file.mice.is_empty() || self.file.legacy.is_some() {
            return false;
        }
        self.file.legacy = Some(legacy);
        true
    }

    /// A mouse connected at `now`. A new one is noted, the first Zaopin with the prefs from before
    /// several mice.
    pub fn meet(&mut self, key: &str, model: &str, now: u64) -> Met {
        if let Some(m) = self.file.mice.iter_mut().find(|m| m.key == key) {
            m.last_seen = now;
            m.model = model.to_string();
            return Met::Again;
        }
        let prefs = if key.starts_with("compx/") { self.file.legacy.take() } else { None };
        let met = if prefs.is_some() { Met::TookOver } else { Met::New };
        self.file.mice.push(KnownMouse {
            key: key.to_string(),
            model: model.to_string(),
            first_seen: now,
            last_seen: now,
            prefs: prefs.unwrap_or_default(),
            last: None,
        });
        met
    }

    /// What the mouse holds now, kept for when it isn't connected (saved with the rest).
    pub fn note_last(&mut self, key: &str, state: &crate::mice::DeviceState) {
        if let Some(m) = self.file.mice.iter_mut().find(|m| m.key == key) {
            m.last = Some(state.clone());
        }
    }

    /// The mouse was there until `now`.
    pub fn seen(&mut self, key: &str, now: u64) {
        if let Some(m) = self.file.mice.iter_mut().find(|m| m.key == key) {
            m.last_seen = now;
        }
    }

    /// Forget mouse `key`, with what's kept for it: if it comes back it's met as new. Returns what
    /// was kept, for Undo.
    pub fn forget(&mut self, key: &str) -> Option<KnownMouse> {
        let i = self.file.mice.iter().position(|m| m.key == key)?;
        Some(self.file.mice.remove(i))
    }

    /// Put back a mouse just forgotten (Undo). Not if it's been met again meanwhile.
    pub fn restore(&mut self, mouse: KnownMouse) -> bool {
        if self.file.mice.iter().any(|m| m.key == mouse.key) {
            return false;
        }
        self.file.mice.push(mouse);
        true
    }

    /// Change a known mouse's prefs. Returns whether anything changed.
    pub fn set_prefs(&mut self, key: &str, prefs: MousePrefs) -> bool {
        let Some(m) = self.file.mice.iter_mut().find(|m| m.key == key) else { return false };
        if m.prefs == prefs {
            return false;
        }
        m.prefs = prefs;
        true
    }

    /// Fill in a rule typed in by hand, on every mouse that has one for this app (see
    /// `MousePrefs::fill_rule`). Returns whether any changed.
    pub fn fill_rule(&mut self, exe: &str, name: &str, path: &str) -> bool {
        let mut changed = false;
        for m in &mut self.file.mice {
            changed |= m.prefs.fill_rule(exe, name.to_string(), path);
        }
        changed
    }

    /// Every app with a profile rule, on any mouse: the HUD counts them as games.
    pub fn rule_exes(&self) -> Vec<String> {
        self.file.mice.iter().flat_map(|m| m.prefs.rules.iter().map(|r| r.exe.to_lowercase())).collect()
    }
}

/// Why hrtz saved a backup by itself. Backups you save have none.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AutoReason {
    /// The mouse as hrtz first found it: no backup of its model existed yet.
    First,
    /// Just before a restore, so it can be undone. Names the backup restored, as it was then.
    Restore { note: String, created: String },
    /// Just before factory settings.
    Factory,
}

/// How many automatic backups are kept; the oldest go first. Yours are kept until you delete them.
pub const AUTO_KEPT: usize = 10;

/// File format shared with `scripts/backup.py`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupFile {
    pub format: String,
    pub created: String,
    pub device: BackupDevice,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<AutoReason>,
    pub flash_size: usize,
    pub sha256: String,
    pub flash_base64: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupDevice {
    pub cid: u8,
    pub mid: u8,
    pub device_type: u8,
    pub firmware: String,
    pub profile_index: Option<u8>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BackupMeta {
    pub id: String,
    pub created: String,
    pub model: String,
    pub firmware: String,
    pub profile: Option<u8>,
    pub note: String,
    pub sha256: String,
    pub auto: Option<AutoReason>,
}

const FORMAT: &str = "zaopin-flash-backup/1";

#[derive(Clone)]
pub struct Backups {
    dir: PathBuf,
}

impl Backups {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        if id.contains(['/', '\\']) || id.contains("..") || !id.ends_with(".json") {
            bail!("invalid backup id");
        }
        Ok(self.dir.join(id))
    }

    pub fn list(&self) -> Vec<BackupMeta> {
        let mut out: Vec<BackupMeta> = fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let id = e.file_name().to_string_lossy().to_string();
                let file = read_backup(&e.path()).ok()?;
                Some(meta(id, &file))
            })
            .collect();
        out.sort_by(|a, b| (b.created.as_str(), seq(&b.id)).cmp(&(a.created.as_str(), seq(&a.id))));
        out
    }

    /// A backup you asked for, kept until you delete it.
    pub fn create(&self, image: &[u8], device: BackupDevice, note: &str) -> Result<BackupMeta> {
        self.write_new(image, device, note, None)
    }

    /// A backup hrtz saves by itself. Only the newest `AUTO_KEPT` of these are kept.
    pub fn create_auto(&self, image: &[u8], device: BackupDevice, reason: AutoReason) -> Result<BackupMeta> {
        let m = self.write_new(image, device, "", Some(reason))?;
        for old in self.list().into_iter().filter(|b| b.auto.is_some()).skip(AUTO_KEPT) {
            let _ = self.delete(&old.id);
        }
        Ok(m)
    }

    fn write_new(
        &self,
        image: &[u8],
        device: BackupDevice,
        note: &str,
        auto: Option<AutoReason>,
    ) -> Result<BackupMeta> {
        fs::create_dir_all(&self.dir)?;
        let now = chrono::Local::now();
        let file = BackupFile {
            format: FORMAT.into(),
            created: now.format("%Y-%m-%dT%H:%M:%S").to_string(),
            device,
            note: note.trim().to_string(),
            auto,
            flash_size: image.len(),
            sha256: hex_sha256(image),
            flash_base64: base64::engine::general_purpose::STANDARD.encode(image),
        };
        // Ids go by the second: a second backup in the same second (one saved just before a
        // restore, say) gets a number rather than replacing the first.
        let stamp = now.format("%Y%m%d-%H%M%S").to_string();
        let id = (1..)
            .map(|n| if n == 1 { format!("{stamp}_backup.json") } else { format!("{stamp}-{n}_backup.json") })
            .find(|id| !self.dir.join(id).exists())
            .expect("a free name");
        fs::write(self.dir.join(&id), serde_json::to_vec_pretty(&file)?)?;
        Ok(meta(id, &file))
    }

    /// Gives a backup a name. A named automatic backup becomes yours: it's no longer one of the
    /// ten hrtz keeps. An empty name leaves it going by its date (or its reason).
    pub fn rename(&self, id: &str, note: &str) -> Result<BackupMeta> {
        let p = self.path(id)?;
        let mut file = read_backup(&p)?;
        file.note = note.trim().to_string();
        if !file.note.is_empty() {
            file.auto = None;
        }
        writable(&p);
        fs::write(&p, serde_json::to_vec_pretty(&file)?)?;
        Ok(meta(id.to_string(), &file))
    }

    pub fn load(&self, id: &str) -> Result<(BackupFile, Vec<u8>)> {
        let file = read_backup(&self.path(id)?)?;
        let image = image_of(&file)?;
        Ok((file, image))
    }

    pub fn settings(&self, id: &str) -> Result<Settings> {
        Ok(image::decode(&self.load(id)?.1))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let p = self.path(id)?;
        writable(&p);
        fs::remove_file(p).context("delete backup")
    }

    pub fn export(&self, id: &str, dest: &Path) -> Result<()> {
        fs::copy(self.path(id)?, dest).context("export backup")?;
        Ok(())
    }

    pub fn import(&self, src: &Path) -> Result<BackupMeta> {
        let file = read_backup(src)?;
        image_of(&file)?;
        fs::create_dir_all(&self.dir)?;
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "imported".into());
        let mut id = format!("{}.json", sanitize(&stem));
        if !id.ends_with("_backup.json") {
            id = id.replace(".json", "_backup.json");
        }
        let dest = self.dir.join(&id);
        if dest.exists() {
            bail!("A backup named {id} already exists.");
        }
        // One you bring in is yours, even if hrtz saved it by itself somewhere: it shouldn't count
        // toward the ten that are kept, and go.
        if file.auto.is_some() {
            let file = BackupFile { auto: None, ..file };
            fs::write(&dest, serde_json::to_vec_pretty(&file)?)?;
            return Ok(meta(id, &file));
        }
        fs::copy(src, &dest)?;
        Ok(meta(id, &file))
    }
}

/// Where a backup falls among those saved in the same second, which get `-2`, `-3`… after the
/// time (`20261016-131800-2_backup.json`). 1 for the first, and for names from elsewhere.
fn seq(id: &str) -> u32 {
    let stem = id.strip_suffix("_backup.json").unwrap_or(id);
    let mut parts = stem.splitn(3, '-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_), Some(_), Some(n)) => n.parse().unwrap_or(1),
        _ => 1,
    }
}

/// Backups written by the Python tool are read-only.
fn writable(p: &Path) {
    if let Ok(m) = fs::metadata(p) {
        let mut perm = m.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = fs::set_permissions(p, perm);
    }
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn read_backup(path: &Path) -> Result<BackupFile> {
    let file: BackupFile = serde_json::from_slice(&fs::read(path)?).context("not a backup file")?;
    if file.format != FORMAT {
        bail!("unsupported backup format {}", file.format);
    }
    Ok(file)
}

fn image_of(file: &BackupFile) -> Result<Vec<u8>> {
    let image = base64::engine::general_purpose::STANDARD.decode(&file.flash_base64)?;
    if image.len() != FLASH_SIZE {
        bail!("backup holds {} bytes, expected {FLASH_SIZE}", image.len());
    }
    if hex_sha256(&image) != file.sha256 {
        return Err(anyhow!("backup is corrupted (checksum mismatch)"));
    }
    Ok(image)
}

/// Firmware as the app writes it ("2.17"). scripts/backup.py writes the raw word ("0x0217").
fn firmware_text(raw: &str) -> String {
    match raw.strip_prefix("0x").and_then(|h| u16::from_str_radix(h, 16).ok()) {
        Some(w) => format!("{:x}.{:02x}", w >> 8, w & 0xff),
        None => raw.to_string(),
    }
}

fn meta(id: String, f: &BackupFile) -> BackupMeta {
    BackupMeta {
        id,
        created: f.created.clone(),
        model: models::model_name(f.device.mid).to_string(),
        firmware: firmware_text(&f.device.firmware),
        profile: f.device.profile_index,
        note: f.note.clone(),
        sha256: f.sha256.clone(),
        auto: f.auto.clone(),
    }
}

fn hex_sha256(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_for_a_known_game_covers_its_other_exes() {
        let p = MousePrefs {
            rules: vec![AppRule { exe: "r5apex.exe".into(), profile: 3, name: None, path: None }],
            ..MousePrefs::default()
        };
        assert_eq!(p.profile_for("R5Apex_DX12.exe"), Some(3));
        assert_eq!(p.profile_for("cs2.exe"), None);
    }

    #[test]
    fn profiles_are_remembered_and_typed_rules_filled_in() {
        let mut p = MousePrefs::default();
        let snap = ProfileSnapshot { mid: 4, dpi: 400, stages: 1, polling_hz: 8000, buttons: vec![] };
        assert!(p.note_profile(1, snap.clone()));
        assert!(!p.note_profile(1, snap.clone()), "unchanged");
        assert!(!p.note_profile(4, snap), "only four profiles");
        assert_eq!(p.profile_snapshots[1].as_ref().map(|s| s.dpi), Some(400));

        p.rules.push(AppRule { exe: "Game.exe".into(), profile: 2, name: None, path: None });
        assert!(p.fill_rule("game.EXE", "The Game".into(), r"C:\Games\Game.exe"));
        assert_eq!(p.rules[0].name.as_deref(), Some("The Game"));
        assert!(!p.fill_rule("game.exe", "Other".into(), r"D:\Game.exe"), "only once");
    }

    #[test]
    fn battery_saver_lowers_once_and_puts_the_rate_back_on_charge() {
        let mut p = MousePrefs { battery_saver: true, ..MousePrefs::default() };
        assert_eq!(p.saver_step(50, false, 8000), None);
        assert_eq!(p.saver_step(20, false, 8000), Some((Some(1000), Some(8000))));
        assert_eq!(p.saver_step(20, false, 1000), None, "already at or below the saver's rate");
        p.battery_saver_restore = Some(8000);
        assert_eq!(p.saver_step(15, false, 1000), None, "stays lowered");
        assert_eq!(p.saver_step(15, true, 1000), Some((Some(8000), None)), "back once charging");
        assert_eq!(p.saver_step(15, false, 4000), Some((None, None)), "changed by hand: keep it");
        p.battery_saver = false;
        assert_eq!(p.saver_step(15, false, 1000), Some((Some(8000), None)), "turned off: back now");
    }

    #[test]
    fn known_games_join_the_pad_list_once_and_not_after_removal() {
        let mut p = Prefs::default();
        assert!(p.join_pad_game("VALORANT-Win64-Shipping.exe", "Valorant".into(), String::new()));
        assert!(!p.join_pad_game("valorant-win64-shipping.exe", "Valorant".into(), String::new()));
        assert!(p.pad_game("valorant-WIN64-shipping.exe").is_some());
        p.pad_games.clear();
        p.pad_games_removed.push("valorant-win64-shipping.exe".into());
        assert!(!p.join_pad_game("VALORANT-Win64-Shipping.exe", "Valorant".into(), String::new()));
        // Older prefs files have neither list.
        let old: Prefs = serde_json::from_str(r#"{"padTracking": true}"#).unwrap();
        assert!(old.pad_games.is_empty() && old.pad_tracking);
    }

    #[test]
    fn hud_prefs_keep_the_shape_the_page_expects() {
        let v = serde_json::to_value(HudPrefs::default()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "show": "change",
                "fields": { "battery": true, "timeLeft": true, "polling": true, "dpi": true, "profile": false, "labels": true },
                "anchor": "top-right",
                "size": "medium",
                "plate": 90,
                "valueColor": "teal",
                "oledCare": true,
                "hiddenGames": [],
                "blocked": []
            })
        );
        // Older prefs files have no HUD settings at all, or only some.
        let old: Prefs = serde_json::from_str(r#"{"osd": false}"#).unwrap();
        assert_eq!(old.hud, HudPrefs::default());
        let some: HudPrefs =
            serde_json::from_str(r#"{"show": "always", "plate": 37.5, "anchor": "bottom-left"}"#).unwrap();
        assert_eq!(
            (some.show, some.plate, some.anchor, some.size),
            (HudShow::Always, 38, HudAnchor::BottomLeft, HudSize::Medium)
        );
        assert_eq!(some.fields, HudFields::default());
        // The mark is always shown; a file from when it could be turned off still loads.
        let with_mark: HudFields = serde_json::from_str(r#"{"mark": false, "battery": false}"#).unwrap();
        assert_eq!(with_mark, HudFields { battery: false, ..HudFields::default() });

        // HUD settings it can't read cost only those, when loading from disk.
        let path = std::env::temp_dir().join(format!("hrtz-hud-prefs-{}.json", std::process::id()));
        fs::write(&path, r#"{"lowBatteryPercent": 25, "hud": {"show": "sometimes", "plate": null}}"#).unwrap();
        let loaded = Prefs::load(&path);
        let _ = fs::remove_file(&path);
        assert_eq!((loaded.low_battery_percent, loaded.hud.clone()), (25, HudPrefs::default()));

        let mut hud = HudPrefs { hidden_games: vec!["cs2.exe".into()], ..HudPrefs::default() };
        assert!(hud.hidden("CS2.exe"));
        assert!(hud.block("VALORANT-Win64-Shipping.exe", "VALORANT".into(), "exclusive"));
        assert!(!hud.block("valorant-win64-shipping.exe", "VALORANT".into(), "exclusive"), "once");
        assert_eq!(
            serde_json::to_value(&hud.blocked).unwrap(),
            serde_json::json!([{ "exe": "VALORANT-Win64-Shipping.exe", "name": "VALORANT", "reason": "exclusive" }])
        );
    }

    /// The HUD's settings as the page writes them, defaults included. `src/api.ts` has the same
    /// shape, so a change here has to be made there too.
    fn contract_hud_json() -> serde_json::Value {
        serde_json::json!({
            "show": "change",
            "fields": { "battery": true, "timeLeft": true, "polling": true, "dpi": true, "profile": false, "labels": true },
            "anchor": "top-right",
            "size": "medium",
            "plate": 90,
            "valueColor": "teal",
            "oledCare": true,
            "hiddenGames": [],
            "blocked": []
        })
    }

    #[test]
    fn hud_defaults_round_trip_to_the_contract_json() {
        let v = serde_json::to_value(HudPrefs::default()).unwrap();
        assert_eq!(v, contract_hud_json());
        assert_eq!(serde_json::from_value::<HudPrefs>(v).unwrap(), HudPrefs::default());
        // And inside the whole prefs file, the way `get_prefs` and the "prefs" event send it.
        let prefs = serde_json::to_value(Prefs::default()).unwrap();
        assert_eq!(prefs["hud"], contract_hud_json());
        assert_eq!(serde_json::from_value::<Prefs>(prefs).unwrap(), Prefs::default());
    }

    #[test]
    fn a_prefs_file_from_before_the_hud_loads_with_its_defaults() {
        let dir = std::env::temp_dir().join(format!("hrtz-prefs-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prefs.json");
        std::fs::write(
            &path,
            r#"{
                "batteryAlerts": false,
                "lowBatteryPercent": 20,
                "osd": true,
                "rulesEnabled": true,
                "defaultProfile": 1,
                "rules": [{ "exe": "cs2.exe", "profile": 2 }],
                "profileNames": ["A", "B", "C", "D"],
                "trayStyle": "percent",
                "padTracking": true,
                "welcomed": true
            }"#,
        )
        .unwrap();
        let (old, legacy) = Prefs::load_with_legacy(&path);
        assert_eq!(old.hud, HudPrefs::default());
        assert_eq!(old.low_battery_percent, 20);
        // Its mouse's part comes out, for the first Zaopin to take.
        let legacy = legacy.expect("the mouse's prefs");
        assert_eq!((legacy.default_profile, legacy.rules.len()), (Some(1), 1));
        assert_eq!(legacy.profile_names, ["A", "B", "C", "D"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_mouse_with_more_profiles_gets_a_name_and_a_snapshot_for_each() {
        let mut p = MousePrefs { profile_names: vec!["Aim".into()], ..MousePrefs::default() };
        assert!(p.fit_profiles(5));
        assert_eq!(p.profile_names, ["Aim", "Profile 2", "Profile 3", "Profile 4", "Profile 5"]);
        assert_eq!(p.profile_snapshots.len(), 5);
        assert!(!p.fit_profiles(5) && !p.fit_profiles(4), "nothing is taken away");
        assert_eq!(p.profile_names.len(), 5);
    }

    #[test]
    fn the_first_zaopin_takes_the_prefs_from_before_several_mice() {
        let mut known = KnownMice::load(None);
        let mut old = MousePrefs::default();
        old.rules.push(AppRule { exe: "cs2.exe".into(), profile: 2, name: None, path: None });
        assert!(known.hold_legacy(old.clone()));
        assert_eq!(known.meet("hidpp/4093/1234", "PRO X SUPERLIGHT", 10), Met::New);
        assert_eq!(known.prefs("hidpp/4093/1234"), MousePrefs::default(), "not a Zaopin: starts afresh");
        assert_eq!(known.meet("compx/7-4", "Z1 PRO MAX", 20), Met::TookOver, "not new to the user");
        assert_eq!(known.prefs("compx/7-4"), old, "the Zaopin has them");
        assert_eq!(known.meet("compx/7-4", "Z1 PRO MAX", 30), Met::Again);
        assert_eq!(known.last_seen().map(|m| m.key.as_str()), Some("compx/7-4"));
        assert!(!known.hold_legacy(MousePrefs::default()), "once mice are known, nothing more to hold");
        assert!(known.fill_rule("CS2.exe", "Counter-Strike 2", r"C:\cs2.exe"));
        assert_eq!(known.rule_exes(), ["cs2.exe"]);
    }

    #[test]
    fn a_forgotten_mouse_is_met_as_new_unless_put_back() {
        let mut known = KnownMice::load(None);
        known.meet("compx/7-3", "Z2 mini", 10);
        known.meet("hidpp/c094/1a2b3c4d", "PRO X SUPERLIGHT", 20);
        let names = MousePrefs { profile_names: vec!["Aim".into()], ..MousePrefs::default() };
        known.set_prefs("compx/7-3", names.clone());
        let gone = known.forget("compx/7-3").unwrap();
        assert_eq!(gone.prefs, names, "what it kept goes with it");
        assert!(known.forget("compx/7-3").is_none());
        assert_eq!(known.list().len(), 1);
        assert!(known.restore(gone.clone()), "Undo");
        assert_eq!(known.prefs("compx/7-3"), names);
        assert!(!known.restore(gone), "not twice");
        known.forget("compx/7-3");
        assert_eq!(known.meet("compx/7-3", "Z2 mini", 30), Met::New, "back as a new mouse");
        assert_eq!(known.prefs("compx/7-3"), MousePrefs::default());
    }

    #[test]
    fn prefs_saved_now_leave_out_what_each_mouse_keeps() {
        let v = serde_json::to_value(Prefs::default()).unwrap();
        let o = v.as_object().unwrap();
        assert!(MousePrefs::FIELDS.iter().all(|f| !o.contains_key(*f)));
    }

    #[test]
    fn a_full_hud_object_reads_every_field() {
        let p: Prefs = serde_json::from_str(
            r#"{
                "osd": false,
                "hud": {
                    "show": "always",
                    "fields": { "battery": false, "timeLeft": false, "polling": true, "dpi": false, "profile": true, "labels": false },
                    "anchor": "bottom-left",
                    "size": "large",
                    "plate": 20,
                    "valueColor": "white",
                    "oledCare": false,
                    "hiddenGames": ["cs2.exe"],
                    "blocked": [{ "exe": "VALORANT-Win64-Shipping.exe", "name": "VALORANT", "reason": "exclusive" }]
                }
            }"#,
        )
        .unwrap();
        let want = HudPrefs {
            show: HudShow::Always,
            fields: HudFields {
                battery: false,
                time_left: false,
                polling: true,
                dpi: false,
                profile: true,
                labels: false,
            },
            anchor: HudAnchor::BottomLeft,
            size: HudSize::Large,
            plate: 20,
            value_color: HudValueColor::White,
            oled_care: false,
            hidden_games: vec!["cs2.exe".into()],
            blocked: vec![HudBlocked {
                exe: "VALORANT-Win64-Shipping.exe".into(),
                name: "VALORANT".into(),
                reason: "exclusive".into(),
            }],
        };
        assert_eq!(p.hud, want);
        assert!(!p.osd);
        // Every other enum string the page can send.
        for (s, show) in [("off", HudShow::Off), ("change", HudShow::Change), ("always", HudShow::Always)] {
            assert_eq!(serde_json::to_value(show).unwrap(), serde_json::json!(s));
            assert_eq!(serde_json::from_value::<HudShow>(serde_json::json!(s)).unwrap(), show);
        }
        for (s, size) in [("small", HudSize::Small), ("medium", HudSize::Medium), ("large", HudSize::Large)] {
            assert_eq!(serde_json::to_value(size).unwrap(), serde_json::json!(s));
            assert_eq!(serde_json::from_value::<HudSize>(serde_json::json!(s)).unwrap(), size);
        }
        for (s, anchor) in [
            ("top-left", HudAnchor::TopLeft),
            ("top", HudAnchor::Top),
            ("top-right", HudAnchor::TopRight),
            ("left", HudAnchor::Left),
            ("right", HudAnchor::Right),
            ("bottom-left", HudAnchor::BottomLeft),
            ("bottom", HudAnchor::Bottom),
            ("bottom-right", HudAnchor::BottomRight),
        ] {
            assert_eq!(serde_json::to_value(anchor).unwrap(), serde_json::json!(s));
            assert_eq!(serde_json::from_value::<HudAnchor>(serde_json::json!(s)).unwrap(), anchor);
        }
        // What it reads, it writes back the same.
        let back = serde_json::to_value(&p.hud).unwrap();
        assert_eq!(serde_json::from_value::<HudPrefs>(back).unwrap(), want);
    }

    #[test]
    fn a_saved_peek_hotkey_loads_as_on_change() {
        // The "While held" mode and its hotkey were removed on 2026-10-15. A file saved before
        // then still loads, with the mode read as On change and the hotkey left behind.
        assert_eq!(serde_json::from_value::<HudShow>(serde_json::json!("hotkey")).unwrap(), HudShow::Change);
        let p: Prefs = serde_json::from_str(
            r#"{
                "lowBatteryPercent": 20,
                "hud": {
                    "show": "hotkey",
                    "anchor": "bottom-left",
                    "plate": 60,
                    "hotkey": { "ctrl": true, "alt": false, "shift": true, "win": false, "key": "KeyH" },
                    "hiddenGames": ["cs2.exe"]
                }
            }"#,
        )
        .unwrap();
        let want = HudPrefs {
            anchor: HudAnchor::BottomLeft,
            plate: 60,
            hidden_games: vec!["cs2.exe".into()],
            ..HudPrefs::default()
        };
        assert_eq!((p.low_battery_percent, &p.hud), (20, &want));
        assert_eq!(p.hud.show, HudShow::Change);
        // A cleared hotkey loads too.
        let cleared: HudPrefs = serde_json::from_str(r#"{"hotkey": null}"#).unwrap();
        assert_eq!(cleared, HudPrefs::default());
        // And it isn't written back.
        assert!(serde_json::to_value(&p.hud).unwrap().get("hotkey").is_none());
        // Through `Prefs::load` too, which would drop the whole HUD object if it couldn't read it.
        let path = std::env::temp_dir().join(format!("hrtz-hud-hotkey-{}.json", std::process::id()));
        fs::write(&path, r#"{"hud": {"show": "hotkey", "plate": 60, "hotkey": {"ctrl": true, "key": "KeyH"}}}"#)
            .unwrap();
        let loaded = Prefs::load(&path);
        let _ = fs::remove_file(&path);
        assert_eq!((loaded.hud.show, loaded.hud.plate), (HudShow::Change, 60));
    }

    #[test]
    fn reads_backup_written_by_python_tool() {
        let dir = std::env::temp_dir().join(format!("hrtz-test-{}", std::process::id()));
        let b = Backups::new(dir.clone());
        let image: &[u8] = &crate::mice::compx::image::sample_image();
        let dev = BackupDevice { cid: 7, mid: 4, device_type: 5, firmware: "0x0217".into(), profile_index: Some(0) };
        let m = b.create(image, dev, "test").unwrap();
        let (_, back) = b.load(&m.id).unwrap();
        assert_eq!(back, image.to_vec());
        assert_eq!(b.list().len(), 1);
        // Shown as the app writes firmware, not as the Python tool stored it.
        assert_eq!(m.firmware, "2.17");
        assert_eq!(super::firmware_text("2.17"), "2.17");
        b.delete(&m.id).unwrap();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_the_newest_automatic_backups_and_all_of_yours() {
        let dir = std::env::temp_dir().join(format!("hrtz-auto-{}", std::process::id()));
        let b = Backups::new(dir.clone());
        let image: &[u8] = &crate::mice::compx::image::sample_image();
        let dev = || BackupDevice { cid: 7, mid: 4, device_type: 5, firmware: "2.17".into(), profile_index: Some(0) };
        let yours = b.create(image, dev(), "Mine").unwrap();
        // Several in the same second: each gets its own name instead of replacing the last.
        let first = b.create_auto(image, dev(), AutoReason::First).unwrap();
        for _ in 0..AUTO_KEPT + 2 {
            b.create_auto(image, dev(), AutoReason::Factory).unwrap();
        }
        let list = b.list();
        assert_eq!(list.iter().filter(|m| m.auto.is_some()).count(), AUTO_KEPT);
        assert!(list.iter().any(|m| m.id == yours.id), "yours are never pruned");
        assert!(list.iter().all(|m| m.id != first.id), "the oldest automatic one went");

        // A name makes an automatic backup yours; an empty one doesn't.
        let auto = list.iter().find(|m| m.auto.is_some()).unwrap().id.clone();
        assert!(b.rename(&auto, "  ").unwrap().auto.is_some());
        let kept = b.rename(&auto, " Tournament ").unwrap();
        assert_eq!((kept.note.as_str(), kept.auto), ("Tournament", None));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rule_lookup_is_case_insensitive_with_fallback() {
        let p = MousePrefs {
            rules: vec![AppRule { exe: "cs2.exe".into(), profile: 2, name: None, path: None }],
            default_profile: Some(0),
            ..Default::default()
        };
        assert_eq!(p.profile_for("CS2.EXE"), Some(2));
        assert_eq!(p.profile_for("explorer.exe"), Some(0));
    }
}
