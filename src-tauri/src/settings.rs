//! Typed view of the mouse flash image. Decoding falls back to the vendor app's defaults for fields
//! whose check byte is invalid; encoding only touches the bytes of fields it knows about.

use serde::{Deserialize, Serialize};

use crate::protocol::{checksum, group_valid, CONFIG_REGION, FLASH_SIZE};

const POLLING: [(u32, u8); 7] = [(125, 8), (250, 4), (500, 2), (1000, 1), (2000, 0x10), (4000, 0x20), (8000, 0x40)];
pub const PEAK_TIMEOUTS_S: [u32; 6] = [30, 60, 120, 300, 600, 900];
/// "Sleep after" choices. 0xBB is seconds without movement before the mouse sleeps, measured on a
/// Z1 PRO MAX from 10 to 120 (scripts/sleep_timer.py, 2026-10-17); it comes set to 10.
pub const SLEEP_TIMES_S: [u32; 4] = [10, 30, 60, 120];
pub const MAX_STAGES: usize = 8;
pub const BUTTON_SLOTS: usize = 16;

mod addr {
    pub const POLLING: usize = 0x00;
    pub const STAGE_COUNT: usize = 0x02;
    pub const STAGE: usize = 0x04;
    pub const LOD: usize = 0x0A;
    pub const DPI: usize = 0x0C;
    pub const DPI_COLOR: usize = 0x2C;
    pub const BUTTONS: usize = 0x60;
    pub const DEBOUNCE: usize = 0xA9;
    pub const MOTION_SYNC: usize = 0xAB;
    pub const ANGLE_SNAP: usize = 0xAF;
    pub const RIPPLE: usize = 0xB1;
    pub const PEAK_ENABLE: usize = 0xB5;
    pub const PEAK_TIMEOUT: usize = 0xB7;
    pub const SENSOR_MODE: usize = 0xB9;
    pub const SLEEP: usize = 0xBB;
    pub const SHORTCUTS: usize = 0x100;
    pub const SHORTCUT_LEN: usize = 0x20;
}

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
    Mouse { button: MouseButton },
    Dpi { action: DpiAction },
    Scroll { direction: ScrollDirection },
    /// Rapid fire: `times` = 0 repeats while held.
    Fire { interval_ms: u8, times: u8 },
    PollingSwitch,
    DpiLock { dpi: u32 },
    /// HID consumer-page usage (e.g. 0x00CD play/pause).
    Media { usage: u16 },
    /// Key combination: modifier bitmask (HID: 1 LCtrl, 2 LShift, 4 LAlt, 8 LWin, 16.. right-hand) plus key usages.
    Keys { modifiers: u8, keys: Vec<u8> },
    Macro { slot: u8, repeat: u8 },
    /// Anything this app does not model yet; written back unchanged.
    Raw { code: u8, p1: u8, p2: u8 },
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

/// Connection kinds, from the handshake's byte 6.
pub fn link(device_type: u8) -> (bool, u32) {
    match device_type {
        0 => (false, 1000), // Wireless1K
        1 => (false, 4000), // Wireless4K
        2 => (true, 1000),  // Wired1K
        3 => (true, 8000),  // Wired8K
        4 => (false, 2000), // Wireless2K
        5 => (false, 8000), // Wireless8K
        _ => (false, 1000),
    }
}

pub fn capabilities(mid: u8, device_type: u8) -> Capabilities {
    // PAW3950 models (MAX line) go to 30K, PAW3395 to 26K.
    let max_dpi = if (4..=6).contains(&mid) { 30000 } else { 26000 };
    let (wired, max_polling_hz) = link(device_type);
    Capabilities {
        max_dpi,
        dpi_step: 50,
        wired,
        max_polling_hz,
        polling_hz: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= max_polling_hz).collect(),
        peak_timeouts_s: PEAK_TIMEOUTS_S.to_vec(),
        sleep_times_s: if mid == 4 { SLEEP_TIMES_S.to_vec() } else { Vec::new() },
        low_lod: (4..=6).contains(&mid),
        buttons: ["Left", "Right", "Wheel click", "Back", "Forward", "DPI"]
            .iter()
            .enumerate()
            .map(|(i, n)| (i as u8, n.to_string()))
            .collect(),
    }
}

pub fn model_name(mid: u8) -> &'static str {
    match mid {
        1 => "ZI PRO",
        2 => "Z2",
        3 => "Z2 mini",
        4 => "Z1 PRO MAX",
        5 => "Z2 MAX",
        6 => "Z2 mini MAX",
        _ => "Zaopin mouse",
    }
}

fn pair(f: &[u8], a: usize) -> Option<u8> {
    group_valid(&f[a..a + 2]).then_some(f[a])
}

fn put_pair(f: &mut [u8], a: usize, v: u8) {
    f[a] = v;
    f[a + 1] = checksum(&[v]);
}

fn put_triple(f: &mut [u8], a: usize, v: [u8; 3]) {
    f[a..a + 3].copy_from_slice(&v);
    f[a + 3] = checksum(&v);
}

pub fn dpi_decode(x: u8, ex: u8) -> u32 {
    ((x as u32 | (((ex >> 6) as u32) << 8)) + 1) * 50
}

/// Returns `(x, ex)` for the PAW3395/3950 encoding (50-DPI steps, high bits mirrored into ex).
pub fn dpi_encode(dpi: u32) -> (u8, u8) {
    let raw = (dpi.clamp(50, 30000) / 50).saturating_sub(1);
    let hi = (raw >> 8) as u8;
    (raw as u8, (hi << 2) | (hi << 6))
}

fn polling_code(hz: u32) -> Option<u8> {
    POLLING.iter().find(|p| p.0 == hz).map(|p| p.1)
}

fn polling_hz(code: u8) -> Option<u32> {
    POLLING.iter().find(|p| p.1 == code).map(|p| p.0)
}

pub fn decode(f: &[u8]) -> Settings {
    assert!(f.len() >= CONFIG_REGION.max(addr::SHORTCUTS + BUTTON_SLOTS * addr::SHORTCUT_LEN));
    let stage_count = pair(f, addr::STAGE_COUNT).filter(|n| (1..=8).contains(n)).unwrap_or(6);
    let stages = (0..MAX_STAGES)
        .map(|i| {
            let d = addr::DPI + 4 * i;
            let c = addr::DPI_COLOR + 4 * i;
            let dpi = if group_valid(&f[d..d + 4]) { dpi_decode(f[d], f[d + 2]) } else { 1600 };
            let color = if group_valid(&f[c..c + 4]) { [f[c], f[c + 1], f[c + 2]] } else { [255, 255, 255] };
            DpiStage { dpi, color: format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]) }
        })
        .collect();
    Settings {
        polling_hz: pair(f, addr::POLLING).and_then(polling_hz).unwrap_or(1000),
        dpi_stage_count: stage_count,
        dpi_stage: pair(f, addr::STAGE).filter(|s| *s < stage_count).unwrap_or(0),
        stages,
        lod: match pair(f, addr::LOD) {
            Some(3) => Lod::Low,
            Some(2) => Lod::High,
            _ => Lod::Mid,
        },
        sensor_mode: match pair(f, addr::SENSOR_MODE) {
            Some(1) => SensorMode::Hp,
            Some(10) => SensorMode::Corded,
            _ => SensorMode::Lp,
        },
        debounce_ms: pair(f, addr::DEBOUNCE).unwrap_or(8),
        motion_sync: pair(f, addr::MOTION_SYNC).map_or(true, |v| v != 0),
        angle_snapping: pair(f, addr::ANGLE_SNAP).is_some_and(|v| v != 0),
        ripple_control: pair(f, addr::RIPPLE).is_some_and(|v| v != 0),
        peak_timeout_enabled: pair(f, addr::PEAK_ENABLE).is_some_and(|v| v != 0),
        peak_timeout_s: pair(f, addr::PEAK_TIMEOUT).unwrap_or(6) as u32 * 10,
        sleep_s: pair(f, addr::SLEEP).filter(|v| *v > 0).unwrap_or(10) as u32,
        buttons: (0..BUTTON_SLOTS).map(|i| decode_button(f, i)).collect(),
    }
}

fn decode_button(f: &[u8], i: usize) -> ButtonAction {
    let a = addr::BUTTONS + 4 * i;
    if !group_valid(&f[a..a + 4]) {
        return ButtonAction::Disabled;
    }
    let (code, p1, p2) = (f[a], f[a + 1], f[a + 2]);
    match (code, p1) {
        (0, _) => ButtonAction::Disabled,
        (1, 0x01) => ButtonAction::Mouse { button: MouseButton::Left },
        (1, 0x02) => ButtonAction::Mouse { button: MouseButton::Right },
        (1, 0x04) => ButtonAction::Mouse { button: MouseButton::Middle },
        (1, 0x08) => ButtonAction::Mouse { button: MouseButton::Back },
        (1, 0x10) => ButtonAction::Mouse { button: MouseButton::Forward },
        (2, 1) => ButtonAction::Dpi { action: DpiAction::Cycle },
        (2, 2) => ButtonAction::Dpi { action: DpiAction::Up },
        (2, 3) => ButtonAction::Dpi { action: DpiAction::Down },
        (3, 1) => ButtonAction::Scroll { direction: ScrollDirection::Left },
        (3, 2) => ButtonAction::Scroll { direction: ScrollDirection::Right },
        (11, 1) => ButtonAction::Scroll { direction: ScrollDirection::Up },
        (11, 2) => ButtonAction::Scroll { direction: ScrollDirection::Down },
        (4, _) => ButtonAction::Fire { interval_ms: p1, times: p2 },
        (5, _) if p1 != 0 || p2 != 0 => ButtonAction::Media { usage: u16::from_le_bytes([p1, p2]) },
        (5, _) => decode_keys(f, i),
        (6, _) => ButtonAction::Macro { slot: p1, repeat: p2 },
        (7, _) => ButtonAction::PollingSwitch,
        (10, _) => ButtonAction::DpiLock { dpi: (p1 as u32 + 1) * 50 },
        _ => ButtonAction::Raw { code, p1, p2 },
    }
}

fn shortcut_slot(i: usize) -> usize {
    addr::SHORTCUTS + i * addr::SHORTCUT_LEN
}

fn decode_keys(f: &[u8], i: usize) -> ButtonAction {
    let s = &f[shortcut_slot(i)..shortcut_slot(i) + addr::SHORTCUT_LEN];
    let count = s[0] as usize;
    let mut modifiers = 0u8;
    let mut keys = Vec::new();
    if (2..=6).contains(&count) && group_valid(&s[..count * 3 + 2]) {
        for e in s[1..1 + count * 3].chunks(3) {
            let (down, kind) = (e[0] & 0xC0 == 0x80, e[0] & 0x0F);
            match (down, kind) {
                (true, 0) => modifiers |= e[1],
                (true, 1) => keys.push(e[1]),
                _ => {}
            }
        }
    }
    ButtonAction::Keys { modifiers, keys }
}

/// Shortcut-slot events for a key combination: every key down in order, then up in reverse.
fn shortcut_events(modifiers: u8, keys: &[u8]) -> Vec<[u8; 3]> {
    let mut downs: Vec<[u8; 2]> = (0..8).filter(|b| modifiers & (1 << b) != 0).map(|b| [0, 1 << b]).collect();
    downs.extend(keys.iter().map(|k| [1, *k]));
    downs.truncate(3);
    let mut ev: Vec<[u8; 3]> = downs.iter().map(|d| [0x80 | d[0], d[1], 0]).collect();
    ev.extend(downs.iter().rev().map(|d| [0x40 | d[0], d[1], 0]));
    ev
}

fn put_shortcut(f: &mut [u8], i: usize, events: &[[u8; 3]]) {
    let s = shortcut_slot(i);
    f[s] = events.len() as u8;
    for (n, e) in events.iter().enumerate() {
        f[s + 1 + n * 3..s + 4 + n * 3].copy_from_slice(e);
    }
    let end = s + 1 + events.len() * 3;
    f[end] = checksum(&f[s..end]);
}

fn encode_button(f: &mut [u8], i: usize, b: &ButtonAction) {
    let (code, p1, p2) = match b {
        ButtonAction::Disabled => (0, 0, 0),
        ButtonAction::Mouse { button } => (1, match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            MouseButton::Back => 0x08,
            MouseButton::Forward => 0x10,
        }, 0),
        ButtonAction::Dpi { action } => (2, match action {
            DpiAction::Cycle => 1,
            DpiAction::Up => 2,
            DpiAction::Down => 3,
        }, 0),
        ButtonAction::Scroll { direction } => match direction {
            ScrollDirection::Left => (3, 1, 0),
            ScrollDirection::Right => (3, 2, 0),
            ScrollDirection::Up => (11, 1, 0),
            ScrollDirection::Down => (11, 2, 0),
        },
        ButtonAction::Fire { interval_ms, times } => (4, (*interval_ms).max(10), (*times).min(3)),
        ButtonAction::Media { usage } => {
            let [lo, hi] = usage.to_le_bytes();
            put_shortcut(f, i, &[[0x82, lo, hi], [0x42, lo, hi]]);
            (5, lo, hi)
        }
        ButtonAction::Keys { modifiers, keys } => {
            put_shortcut(f, i, &shortcut_events(*modifiers, keys));
            (5, 0, 0)
        }
        ButtonAction::Macro { slot, repeat } => (6, *slot, *repeat),
        ButtonAction::PollingSwitch => (7, 0, 0),
        ButtonAction::DpiLock { dpi } => (10, dpi_encode(*dpi).0, 0),
        ButtonAction::Raw { code, p1, p2 } => (*code, *p1, *p2),
    };
    put_triple(f, addr::BUTTONS + 4 * i, [code, p1, p2]);
}

/// Validate `s` against device limits, returning a human-readable reason on failure.
pub fn validate(s: &Settings, old: &Settings, caps: &Capabilities) -> Result<(), String> {
    // A rate above this link's limit may already be stored for the other link; keeping it is fine.
    if s.polling_hz != old.polling_hz && !caps.polling_hz.contains(&s.polling_hz) {
        return Err(format!("{} Hz is not supported by this receiver", s.polling_hz));
    }
    if !(1..=MAX_STAGES as u8).contains(&s.dpi_stage_count) || s.dpi_stage >= s.dpi_stage_count {
        return Err("DPI stage out of range".into());
    }
    if s.stages.len() != MAX_STAGES || s.buttons.len() != BUTTON_SLOTS {
        return Err("settings are incomplete".into());
    }
    for st in &s.stages {
        if st.dpi < 50 || st.dpi > caps.max_dpi || st.dpi % 50 != 0 {
            return Err(format!("{} DPI must be a multiple of 50 between 50 and {}", st.dpi, caps.max_dpi));
        }
        parse_color(&st.color).ok_or_else(|| format!("{} is not a #RRGGBB colour", st.color))?;
    }
    if s.debounce_ms > 20 {
        return Err("debounce must be 0-20 ms".into());
    }
    if !PEAK_TIMEOUTS_S.contains(&s.peak_timeout_s) {
        return Err("unsupported peak performance timeout".into());
    }
    if !(1..=255).contains(&s.sleep_s) {
        return Err("sleep must be 1-255 s".into());
    }
    for b in &s.buttons {
        if let ButtonAction::Keys { modifiers, keys } = b {
            if modifiers.count_ones() as usize + keys.len() > 3 {
                return Err("a key combination can have at most three keys".into());
            }
        }
    }
    Ok(())
}

fn parse_color(c: &str) -> Option<[u8; 3]> {
    let h = c.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// Write every modelled field of `s` into a copy of `image`.
pub fn encode(s: &Settings, image: &[u8]) -> Vec<u8> {
    let mut f = image.to_vec();
    let old = decode(image);
    // Only fields the caller changed are written, so invalid-but-untouched bytes stay as they are.
    let mut pair_if = |changed: bool, a: usize, v: u8| {
        if changed {
            put_pair(&mut f, a, v);
        }
    };
    pair_if(s.polling_hz != old.polling_hz, addr::POLLING, polling_code(s.polling_hz).unwrap_or(1));
    pair_if(s.dpi_stage_count != old.dpi_stage_count, addr::STAGE_COUNT, s.dpi_stage_count);
    pair_if(s.dpi_stage != old.dpi_stage, addr::STAGE, s.dpi_stage);
    pair_if(s.lod != old.lod, addr::LOD, match s.lod {
        Lod::Low => 3,
        Lod::Mid => 1,
        Lod::High => 2,
    });
    pair_if(s.sensor_mode != old.sensor_mode, addr::SENSOR_MODE, match s.sensor_mode {
        SensorMode::Lp => 0,
        SensorMode::Hp => 1,
        SensorMode::Corded => 10,
    });
    pair_if(s.debounce_ms != old.debounce_ms, addr::DEBOUNCE, s.debounce_ms);
    pair_if(s.motion_sync != old.motion_sync, addr::MOTION_SYNC, s.motion_sync as u8);
    pair_if(s.angle_snapping != old.angle_snapping, addr::ANGLE_SNAP, s.angle_snapping as u8);
    pair_if(s.ripple_control != old.ripple_control, addr::RIPPLE, s.ripple_control as u8);
    pair_if(s.peak_timeout_enabled != old.peak_timeout_enabled, addr::PEAK_ENABLE, s.peak_timeout_enabled as u8);
    pair_if(s.peak_timeout_s != old.peak_timeout_s, addr::PEAK_TIMEOUT, (s.peak_timeout_s / 10) as u8);
    pair_if(s.sleep_s != old.sleep_s, addr::SLEEP, s.sleep_s as u8);
    for (i, st) in s.stages.iter().enumerate().take(MAX_STAGES) {
        if old.stages.get(i) != Some(st) {
            let (x, ex) = dpi_encode(st.dpi);
            put_triple(&mut f, addr::DPI + 4 * i, [x, x, ex]);
            put_triple(&mut f, addr::DPI_COLOR + 4 * i, parse_color(&st.color).unwrap_or([255, 255, 255]));
        }
    }
    for (i, b) in s.buttons.iter().enumerate().take(BUTTON_SLOTS) {
        if old.buttons.get(i) != Some(b) {
            encode_button(&mut f, i, b);
        }
    }
    f
}

/// Byte ranges where `new` differs from `old`, as `(addr, bytes)` runs of at most `max` bytes.
pub fn diff(old: &[u8], new: &[u8], max: usize) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    let mut i = 0;
    let n = old.len().min(new.len()).min(FLASH_SIZE);
    while i < n {
        if old[i] == new[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && i - start < max && old[i] != new[i] {
            i += 1;
        }
        out.push((start, new[start..i].to_vec()));
    }
    out
}

/// A made-up Z1 PRO MAX image for the tests: the real layout with invented values, and blank
/// shortcuts and macros. Not read from any mouse.
#[cfg(test)]
pub(crate) fn sample_image() -> Vec<u8> {
    let mut f = vec![0xFF; FLASH_SIZE];
    put_pair(&mut f, addr::POLLING, 0x20); // 4,000 Hz
    put_pair(&mut f, addr::STAGE_COUNT, 4);
    put_pair(&mut f, addr::STAGE, 1);
    put_pair(&mut f, 0x06, 0); // X spindown
    put_pair(&mut f, 0x08, 0); // Y spindown
    put_pair(&mut f, addr::LOD, 1); // 1 mm
    let colours = [[0xFF, 0x40, 0x40], [0x40, 0xFF, 0x40], [0x40, 0x40, 0xFF], [0xFF, 0xFF, 0x40], [0xFF, 0x40, 0xFF], [0x40, 0xFF, 0xFF], [0xFF, 0xFF, 0xFF], [0x80, 0x80, 0x80]];
    for (i, dpi) in [400, 800, 1600, 3200, 6400, 12800, 26000, 30000].into_iter().enumerate() {
        let (x, ex) = dpi_encode(dpi);
        put_triple(&mut f, addr::DPI + 4 * i, [x, x, ex]);
        put_triple(&mut f, addr::DPI_COLOR + 4 * i, colours[i]);
    }
    // The DPI light: fixed, brightness, speed, on.
    for (i, v) in [1, 0x80, 3, 1].into_iter().enumerate() {
        put_pair(&mut f, 0x4C + 2 * i, v);
    }
    let buttons: [[u8; 3]; BUTTON_SLOTS] = [
        [1, 0x01, 0], // left
        [1, 0x02, 0], // right
        [1, 0x04, 0], // middle
        [1, 0x08, 0], // back
        [1, 0x10, 0], // forward
        [2, 1, 0],    // cycle DPI stages
        [11, 1, 0],   // scroll up
        [7, 0, 0],    // polling switch
        [10, 15, 0],  // hold for 800 DPI
        [6, 0, 1],    // macro 1, once
        [4, 10, 3],   // rapid fire, 10 ms, 3 clicks
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
    ];
    for (i, b) in buttons.into_iter().enumerate() {
        put_triple(&mut f, addr::BUTTONS + 4 * i, b);
    }
    // The LED bar: mode, r, g, b, speed, brightness and a check byte, then its switch.
    let bar = [1, 0x10, 0x20, 0x30, 2, 0x64];
    f[0xA0..0xA6].copy_from_slice(&bar);
    f[0xA6] = checksum(&bar);
    put_pair(&mut f, 0xA7, 1);
    put_pair(&mut f, addr::DEBOUNCE, 6);
    put_pair(&mut f, addr::MOTION_SYNC, 1);
    put_pair(&mut f, 0xAD, 3); // lights off after 30 s
    put_pair(&mut f, addr::ANGLE_SNAP, 0);
    put_pair(&mut f, addr::RIPPLE, 1);
    put_pair(&mut f, 0xB3, 0); // lights stay on while moving
    put_pair(&mut f, addr::PEAK_ENABLE, 1);
    put_pair(&mut f, addr::PEAK_TIMEOUT, 12); // 2 min
    put_pair(&mut f, addr::SENSOR_MODE, 1); // high performance
    put_pair(&mut f, addr::SLEEP, 20);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    static FIXTURE: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(sample_image);

    #[test]
    fn decodes_a_z1_pro_max_image() {
        let s = decode(&FIXTURE);
        assert_eq!(s.polling_hz, 4000);
        assert_eq!(s.dpi_stage_count, 4);
        assert_eq!(s.dpi_stage, 1);
        assert_eq!(s.stages[0].dpi, 400);
        assert_eq!(s.stages[0].color, "#FF4040");
        assert_eq!(s.stages[6].dpi, 26000);
        assert_eq!(s.stages[7].dpi, 30000);
        assert_eq!(s.lod, Lod::Mid);
        assert_eq!(s.sensor_mode, SensorMode::Hp);
        assert_eq!(s.debounce_ms, 6);
        assert!(s.motion_sync && !s.angle_snapping && s.ripple_control);
        assert!(s.peak_timeout_enabled);
        assert_eq!(s.peak_timeout_s, 120);
        assert_eq!(s.sleep_s, 20);
        assert_eq!(s.buttons[0], ButtonAction::Mouse { button: MouseButton::Left });
        assert_eq!(s.buttons[4], ButtonAction::Mouse { button: MouseButton::Forward });
        assert_eq!(s.buttons[5], ButtonAction::Dpi { action: DpiAction::Cycle });
        assert_eq!(s.buttons[6], ButtonAction::Scroll { direction: ScrollDirection::Up });
        assert_eq!(s.buttons[7], ButtonAction::PollingSwitch);
        assert_eq!(s.buttons[8], ButtonAction::DpiLock { dpi: 800 });
        assert_eq!(s.buttons[9], ButtonAction::Macro { slot: 0, repeat: 1 });
        assert_eq!(s.buttons[10], ButtonAction::Fire { interval_ms: 10, times: 3 });
        assert_eq!(s.buttons[11], ButtonAction::Disabled);
    }

    #[test]
    fn unchanged_settings_encode_to_identical_image() {
        let s = decode(&FIXTURE);
        assert!(diff(&FIXTURE, &encode(&s, &FIXTURE), 10).is_empty());
    }

    #[test]
    fn single_change_produces_single_write() {
        let mut s = decode(&FIXTURE);
        s.debounce_ms = 4;
        let d = diff(&FIXTURE, &encode(&s, &FIXTURE), 10);
        assert_eq!(d, vec![(0xA9, vec![4, 0x51])]);
    }

    #[test]
    fn sleep_after_writes_its_own_pair() {
        let mut s = decode(&FIXTURE);
        s.sleep_s = 30;
        assert_eq!(diff(&FIXTURE, &encode(&s, &FIXTURE), 10), vec![(0xBB, vec![30, 0x37])]);
        s.sleep_s = 120;
        assert_eq!(diff(&FIXTURE, &encode(&s, &FIXTURE), 10), vec![(0xBB, vec![120, 0xDD])]);
    }

    #[test]
    fn dpi_round_trips_across_the_range() {
        for dpi in (50..=30000).step_by(50) {
            let (x, ex) = dpi_encode(dpi);
            assert_eq!(dpi_decode(x, ex), dpi, "dpi {dpi}");
        }
    }

    #[test]
    fn key_combo_round_trips() {
        let mut s = decode(&FIXTURE);
        // Ctrl + Shift + Esc
        s.buttons[3] = ButtonAction::Keys { modifiers: 0x03, keys: vec![41] };
        let img = encode(&s, &FIXTURE);
        assert_eq!(decode(&img).buttons[3], s.buttons[3]);
        let slot = shortcut_slot(3);
        assert_eq!(img[slot], 6);
    }

    #[test]
    fn media_key_round_trips() {
        let mut s = decode(&FIXTURE);
        s.buttons[4] = ButtonAction::Media { usage: 0x00CD };
        let img = encode(&s, &FIXTURE);
        assert_eq!(decode(&img).buttons[4], s.buttons[4]);
    }
}
