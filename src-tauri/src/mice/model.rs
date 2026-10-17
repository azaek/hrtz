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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub polling_hz: u32,
    pub dpi_stage_count: u8,
    /// 0-based index of the active stage.
    pub dpi_stage: u8,
    pub stages: Vec<DpiStage>,
    pub lod: Lod,
    pub sensor_mode: SensorMode,
    pub debounce_ms: u8,
    pub motion_sync: bool,
    pub angle_snapping: bool,
    pub ripple_control: bool,
    pub peak_timeout_enabled: bool,
    pub peak_timeout_s: u32,
    /// Seconds without movement before the mouse sleeps (not in the vendor app).
    pub sleep_s: u32,
    pub buttons: Vec<ButtonAction>,
}

/// Device-specific limits the UI needs.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub max_dpi: u32,
    pub dpi_step: u32,
    /// Connected by USB cable rather than a receiver.
    pub wired: bool,
    /// Highest rate this connection carries; the flash may hold a higher one for the other link.
    pub max_polling_hz: u32,
    pub polling_hz: Vec<u32>,
    pub peak_timeouts_s: Vec<u32>,
    /// "Sleep after" choices: only where the sleep timer has been measured, a Z1 PRO MAX so far.
    pub sleep_times_s: Vec<u32>,
    /// 0.7 mm lift-off is only offered on PAW3950 models.
    pub low_lod: bool,
    /// Flash indices of buttons that physically exist, with their names.
    pub buttons: Vec<(u8, String)>,
}
