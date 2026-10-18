//! What the UI reads and writes for a mouse: its settings and what it can do. Shaped by the Zaopin
//! for now; PLAN-mice.md has where it's going (every feature optional, so a mouse without one
//! leaves its card out).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lod {
    #[serde(rename = "0.7mm")]
    Low,
    #[serde(rename = "1mm")]
    Mid,
    #[serde(rename = "2mm")]
    High,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SensorMode {
    Lp,
    Hp,
    Corded,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DpiStage {
    pub dpi: u32,
    /// `#RRGGBB`
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ButtonAction {
    Disabled,
    Mouse {
        button: MouseButton,
    },
    Dpi {
        action: DpiAction,
    },
    Scroll {
        direction: ScrollDirection,
    },
    /// Rapid fire: `times` = 0 repeats while held.
    Fire {
        interval_ms: u8,
        times: u8,
    },
    PollingSwitch,
    DpiLock {
        dpi: u32,
    },
    /// HID consumer-page usage (e.g. 0x00CD play/pause).
    Media {
        usage: u16,
    },
    /// Key combination: modifier bitmask (HID: 1 LCtrl, 2 LShift, 4 LAlt, 8 LWin, 16.. right-hand) plus key usages.
    Keys {
        modifiers: u8,
        keys: Vec<u8>,
    },
    Macro {
        slot: u8,
        repeat: u8,
    },
    /// Anything this app does not model yet; written back unchanged.
    Raw {
        code: u8,
        p1: u8,
        p2: u8,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DpiAction {
    Cycle,
    Up,
    Down,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// What the mouse holds now. A setting this mouse doesn't have is `None`, and its card isn't shown.
/// Sent back whole to change something; `None` there leaves that setting as it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_hz: Option<u32>,
    /// How many DPI stages the DPI button cycles through.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_stage_count: Option<u8>,
    /// 0-based index of the stage in use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_stage: Option<u8>,
    /// Every stage slot the mouse has, used or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<Vec<DpiStage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lod: Option<Lod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_mode: Option<SensorMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion_sync: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle_snapping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ripple_control: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_timeout_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_timeout_s: Option<u32>,
    /// Seconds without movement before the mouse sleeps (not in the vendor app).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_s: Option<u32>,
    /// What each button does, by the mouse's own button index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<ButtonAction>>,
}

impl Settings {
    /// The DPI of the stage in use.
    pub fn dpi(&self) -> Option<u32> {
        self.stages.as_ref()?.get(self.dpi_stage? as usize).map(|s| s.dpi)
    }
}

/// What the mouse can do, with its real limits. A feature it lacks is `None` (or `false`), so the
/// UI leaves its card out, and app features that need it step aside for this mouse.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    /// Connected by USB cable rather than a receiver.
    pub wired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<DpiCaps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling: Option<PollingCaps>,
    /// Lift-off distances offered, lowest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lod: Option<Vec<Lod>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_mode: Option<SensorModeCaps>,
    /// Debounce, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debounce_ms: Option<Range>,
    pub motion_sync: bool,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    /// Peak performance's timeouts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_timeouts_s: Option<Vec<u32>>,
    /// "Sleep after" choices: only where the sleep timer has been measured, a Z1 PRO MAX so far.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_times_s: Option<Vec<u32>>,
    /// The buttons that physically exist: the mouse's index for each, and its name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<(u8, String)>>,
    /// How many onboard profiles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profiles: Option<u8>,
    pub battery: bool,
    pub long_range: bool,
    pub pairing: bool,
    pub factory_reset: bool,
    /// Its whole memory can be read and written back, for backups.
    pub backups: bool,
    /// Asleep, it isn't asked whether it woke: hrtz waits to see it used (raw input on its port),
    /// so raw input is followed while it's connected.
    pub wake_by_input: bool,
    /// It has a button that cycles the DPI stages out of the box (every Zaopin; not the Superlights).
    pub dpi_button: bool,
}

impl Capabilities {
    /// The highest polling rate this connection carries.
    pub fn top_rate(&self) -> Option<u32> {
        self.polling.as_ref()?.rates.last().copied()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DpiCaps {
    pub min: u32,
    pub max: u32,
    pub step: u32,
    /// Stage slots; the DPI button cycles through the first `dpi_stage_count`.
    pub max_stages: u8,
    /// Each stage has a colour for the mouse's light.
    pub colors: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PollingCaps {
    /// The rates this connection carries, lowest first. The mouse may hold a higher one, set for
    /// its other connection.
    pub rates: Vec<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SensorModeCaps {
    pub modes: Vec<SensorMode>,
    /// The mouse runs Corded at this rate and above, whatever is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corded_from_hz: Option<u32>,
    /// On its cable, the mouse always runs Corded.
    pub corded_when_wired: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub min: u32,
    pub max: u32,
}

/// A mouse hrtz can run, as its family's table lists it: for "See which mice work" and the report
/// forms.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Supported {
    pub family: &'static str,
    pub brand: &'static str,
    pub model: &'static str,
    /// The key a connected one gets, for its picture (`compx/7-4`).
    pub key: String,
    /// Tried on the real mouse.
    pub tested: bool,
}
