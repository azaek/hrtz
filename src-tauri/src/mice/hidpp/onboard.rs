// Ported from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/logitech/onboard-profiles.ts.
// Translated to Rust and modified for hrtz, 2026-10-18. Copyright (C) the OpenMouse contributors.
// AGPL-3.0-or-later.

//! Feature 0x8100, onboard profiles: where a Logitech mouse keeps the settings it runs without
//! software. The profiles live in sectors of flash (255 bytes on the Superlights), each ending in a
//! CRC; sector 0 is the directory. What each byte means depends on the profile format the mouse
//! reports: formats 1 to 5 share one layout (v1), 6 and later another (v6). Pure functions only,
//! so all of it is tested against sectors read from real mice.

use super::protocol::REPORT_RATE_HZ;

/// 0x8100's functions.
pub mod func {
    pub const INFO: u8 = 0x00;
    pub const GET_MODE: u8 = 0x20;
    pub const SET_CURRENT: u8 = 0x30;
    pub const GET_CURRENT: u8 = 0x40;
    pub const READ: u8 = 0x50;
    pub const ADDR_WRITE: u8 = 0x60;
    pub const WRITE: u8 = 0x70;
    pub const WRITE_END: u8 = 0x80;
    /// The stage the mouse runs now, and setting it: RAM only, no flash write. Not in OpenMouse;
    /// from libratbag's HID++ 2.0 code (MIT), to be seen answered on each mouse.
    pub const GET_DPI_INDEX: u8 = 0xB0;
    pub const SET_DPI_INDEX: u8 = 0xC0;
}

/// getMode: the mouse runs its onboard profile, or software drives it live (host mode, which
/// hrtz never switches to: settings there are lost at power-off). G HUB switches to host mode
/// (setMode, 0x10) when it starts the mouse with its On-board memory mode off, and back to onboard
/// when it lets go (GHUB.md, from its code).
pub const MODE_ONBOARD: u8 = 0x01;
pub const MODE_HOST: u8 = 0x02;

/// Where the factory (ROM) profiles are read: 0x0101 for the first. G HUB falls back to them when a
/// profile won't read (GHUB.md); libratbag reads them the same way. Not written.
pub const ROM_FIRST: u16 = 0x0101;

/// memoryRead hands back 16 bytes at a time; memoryWrite takes 16.
pub const CHUNK: usize = 16;

/// What getInfo says about the mouse's profile memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub memory_model: u8,
    pub format: u8,
    pub macro_format: u8,
    pub profile_count: u8,
    /// Factory profiles kept in read-only memory, read from `ROM_FIRST` on.
    pub rom_count: u8,
    pub button_count: u8,
    pub sector_count: u8,
    pub sector_size: u16,
}

impl Info {
    /// From getInfo's reply, its parameters.
    pub fn parse(p: &[u8]) -> Self {
        let at = |i: usize| p.get(i).copied().unwrap_or(0);
        Self {
            memory_model: at(0),
            format: at(1),
            macro_format: at(2),
            profile_count: at(3),
            rom_count: at(4),
            button_count: at(5),
            sector_count: at(6),
            sector_size: u16::from_be_bytes([at(7), at(8)]),
        }
    }

    /// How many bytes a sector holds: as reported, or 255 when the report makes no sense.
    pub fn sector_len(&self) -> usize {
        match self.sector_size {
            1..=1024 => self.sector_size as usize,
            _ => 255,
        }
    }
}

/// One profile in the directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub sector: u16,
    /// The directory's flag byte. OpenMouse reads it as "enabled"; on a Superlight 2 it was set on
    /// the profile in use. To settle on hardware.
    pub enabled: bool,
}

/// The directory (sector 0): 4 bytes a profile, the sector big-endian, then the flag, then a
/// reserved byte. It ends at sector 0xFFFF (erased) or 0x0000.
pub fn directory(bytes: &[u8]) -> Vec<Entry> {
    bytes
        .chunks_exact(4)
        .map(|e| (u16::from_be_bytes([e[0], e[1]]), e[2]))
        .take_while(|(sector, _)| *sector != 0xFFFF && *sector != 0x0000)
        .map(|(sector, flag)| Entry { sector, enabled: flag == 0x01 })
        .collect()
}

/// CRC-16/CCITT-FALSE over all of a sector but its last two bytes, where it's kept.
pub fn crc(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in &bytes[..bytes.len().saturating_sub(2)] {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

/// The CRC a sector carries, big-endian in its last two bytes.
pub fn stored_crc(bytes: &[u8]) -> u16 {
    match bytes.len() {
        n @ 2.. => u16::from_be_bytes([bytes[n - 2], bytes[n - 1]]),
        _ => 0,
    }
}

pub fn crc_ok(bytes: &[u8]) -> bool {
    bytes.len() > 2 && crc(bytes) == stored_crc(bytes)
}

pub fn apply_crc(bytes: &mut [u8]) {
    let n = bytes.len();
    if n >= 2 {
        let [hi, lo] = crc(bytes).to_be_bytes();
        bytes[n - 2] = hi;
        bytes[n - 1] = lo;
    }
}

/// Formats whose layout was confirmed on a real profile with a matching CRC, and whose writes
/// were applied and read back on hardware (in OpenMouse): 2 (G502), 3 (G703), 4 (G102, and the
/// first Superlight's layout), 7 (Superlight 2) and 8 (the Superstrikes).
pub fn writable(format: u8) -> bool {
    matches!(format, 2 | 3 | 4 | 7 | 8)
}

/// Formats 1 to 5 keep one DPI per stage and one report interval for both links; 6 and later keep
/// X and Y and a lift-off level per stage, and a rate for each link.
pub fn v6(format: u8) -> bool {
    format >= 6
}

/// DPI stage limits a format holds, before the sensor narrows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpiLimits {
    pub max_stages: u8,
    pub min: u32,
    pub max: u32,
    pub step: u32,
}

/// The fastest rate each link carries. They differ: the radio and the USB cable are not equally
/// fast on every mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateLimits {
    pub wireless_max: u32,
    pub wired_max: u32,
}

/// What a profile format holds, by the format the mouse reports, not by model: a new mouse on a
/// known format inherits it. A format whose limits were never established offers nothing.
pub fn dpi_limits(format: u8) -> Option<DpiLimits> {
    let (min, max, step) = match format {
        1 => (252, 4032, 84),  // a G402
        2 => (100, 25600, 50), // the G502 HERO's grid; the sensor's own list narrows it
        3 => (50, 12000, 50),  // a G703
        4 => (50, 8000, 50),   // a G102 LIGHTSYNC
        7 => (100, 32000, 50), // the Superlight 2
        8 => (100, 48000, 50), // the PRO X 3 Superstrike's list
        _ => return None,
    };
    Some(DpiLimits { max_stages: STAGE_SLOTS as u8, min, max, step })
}

/// USB IDs whose cable carries the wireless link's full rate, not the 1,000 Hz the rest of their
/// format is held to: the PRO X 3 Superstrike.
const FULL_RATE_USB: [&str; 1] = ["C0A9"];

pub fn rate_limits(format: u8, usb_id: Option<&str>) -> Option<RateLimits> {
    let mut limits = match format {
        1..=4 => RateLimits { wireless_max: 1000, wired_max: 1000 },
        // Reported on hardware: the Superlight 2's LIGHTSPEED runs to 8 kHz, its cable is a 1 kHz
        // charging link.
        7 | 8 => RateLimits { wireless_max: 8000, wired_max: 1000 },
        _ => return None,
    };
    if usb_id.is_some_and(|id| FULL_RATE_USB.iter().any(|f| f.eq_ignore_ascii_case(id))) {
        limits.wired_max = limits.wireless_max;
    }
    Some(limits)
}

/// The rates a link's byte can hold, lowest first.
pub fn rates_for(limits: Option<RateLimits>, wired: bool) -> Vec<u32> {
    let Some(l) = limits else { return Vec::new() };
    let ceiling = if wired { l.wired_max } else { l.wireless_max };
    REPORT_RATE_HZ.iter().copied().filter(|hz| *hz <= ceiling).collect()
}

/// The rates hrtz offers for the link the mouse is on: what the format holds, and what the mouse
/// itself says that link carries, when it says.
pub fn rates_offered(limits: Option<RateLimits>, wired: bool, live: &[u32]) -> Vec<u32> {
    let stored = rates_for(limits, wired);
    if live.is_empty() { stored } else { stored.into_iter().filter(|hz| live.contains(hz)).collect() }
}

/// The stage table has room for five, whatever a format allows.
pub const STAGE_SLOTS: usize = 5;

/// Where the fields hrtz reads and writes sit in a profile.
struct Layout {
    rate_wireless: usize,
    rate_wired: usize,
    /// The DPI block: the default stage, the G-shift stage, then the five stages.
    dpi: usize,
}

fn layout(format: u8) -> Layout {
    if v6(format) {
        Layout { rate_wireless: 0x00, rate_wired: 0x01, dpi: 0x02 }
    } else {
        // One report interval for both links.
        Layout { rate_wireless: 0x00, rate_wired: 0x00, dpi: 0x01 }
    }
}

/// One DPI stage. `lod` is the stage's lift-off level on formats 6 and later (1 low, 2 medium, 3
/// high; 0 marks the stage unused); formats 1 to 5 have none, and X is Y.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub x: u32,
    pub y: u32,
    pub lod: u8,
}

/// What hrtz reads from a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// The stages in use, in order.
    pub stages: Vec<Stage>,
    /// The stage the mouse starts on.
    pub default_index: Option<u8>,
    pub rate_wireless: Option<u32>,
    pub rate_wired: Option<u32>,
}

/// Erased flash reads 0xFFFF.
fn u16_le(bytes: &[u8], at: usize) -> Option<u32> {
    let v = u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]);
    (v != 0xFFFF).then_some(v as u32)
}

fn put_u16_le(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 2].copy_from_slice(&(v as u16).to_le_bytes());
}

pub fn decode(bytes: &[u8], format: u8) -> Profile {
    let l = layout(format);
    let default_index = bytes.get(l.dpi).copied().filter(|i| *i != 0xFF);
    let stages = (0..STAGE_SLOTS)
        .filter_map(|i| {
            if v6(format) {
                let base = l.dpi + 2 + i * 5;
                let (x, y, lod) = (u16_le(bytes, base)?, u16_le(bytes, base + 2)?, *bytes.get(base + 4)?);
                (x != 0).then_some(Stage { x, y, lod })
            } else {
                let dpi = u16_le(bytes, l.dpi + 2 + i * 2)?;
                (dpi != 0).then_some(Stage { x: dpi, y: dpi, lod: 0 })
            }
        })
        .collect();
    let rate = |at: usize| {
        let raw = *bytes.get(at)?;
        if v6(format) {
            (raw != 0xFF).then(|| REPORT_RATE_HZ.get(raw as usize).copied()).flatten()
        } else {
            // Formats 1 to 5 keep the USB interval in milliseconds, as 0x8060 does.
            (raw != 0 && raw != 0xFF && 1000 % raw as u32 == 0).then(|| 1000 / raw as u32)
        }
    };
    Profile { stages, default_index, rate_wireless: rate(l.rate_wireless), rate_wired: rate(l.rate_wired) }
}

/// The DPI stages to write: the ones in use, and the one the mouse starts on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub stages: Vec<Stage>,
    pub default_index: u8,
}

/// Why `plan` can't be written to this format, if it can't.
pub fn check_plan(plan: &Plan, limits: Option<DpiLimits>, format: u8) -> Result<(), String> {
    let Some(DpiLimits { max_stages, min, max, step }) = limits else {
        return Err("hrtz doesn't know this mouse's DPI stages yet.".into());
    };
    if plan.stages.is_empty() || plan.stages.len() > max_stages as usize {
        return Err(format!("This mouse holds 1 to {max_stages} DPI stages."));
    }
    if plan.default_index as usize >= plan.stages.len() {
        return Err("The stage in use must be one of the stages.".into());
    }
    for (i, s) in plan.stages.iter().enumerate() {
        for v in [s.x, s.y] {
            if v < min || v > max {
                return Err(format!("Stage {} must be {min} to {max} DPI.", i + 1));
            }
            if v % step != 0 {
                return Err(format!("Stage {} must be a multiple of {step} DPI.", i + 1));
            }
        }
        if !v6(format) && (s.x != s.y || s.lod != 0) {
            return Err(format!("Stage {} can't hold a separate Y DPI or a lift-off level.", i + 1));
        }
        if v6(format) && !(1..=3).contains(&s.lod) {
            return Err(format!("Stage {} has no lift-off level.", i + 1));
        }
    }
    Ok(())
}

/// `sector` with its stage table replaced and its CRC redone. Stages past the plan are zeroed, as
/// the vendor app marks a stage unused. The G-shift stage is left alone unless it would point past
/// the stages in use. Without `write_lod`, each stage's lift-off byte stays as it was: the first
/// Superlight refused that byte on a format 7 profile.
pub fn encode_stages(
    sector: &[u8],
    format: u8,
    plan: &Plan,
    limits: Option<DpiLimits>,
    write_lod: bool,
) -> Result<Vec<u8>, String> {
    check_plan(plan, limits, format)?;
    let l = layout(format);
    let mut out = sector.to_vec();
    out[l.dpi] = plan.default_index;
    if out[l.dpi + 1] as usize >= plan.stages.len() {
        out[l.dpi + 1] = plan.default_index;
    }
    for i in 0..STAGE_SLOTS {
        let s = plan.stages.get(i);
        if v6(format) {
            let base = l.dpi + 2 + i * 5;
            put_u16_le(&mut out, base, s.map_or(0, |s| s.x));
            put_u16_le(&mut out, base + 2, s.map_or(0, |s| s.y));
            if write_lod {
                out[base + 4] = s.map_or(0, |s| s.lod);
            }
        } else {
            put_u16_le(&mut out, l.dpi + 2 + i * 2, s.map_or(0, |s| s.x));
        }
    }
    apply_crc(&mut out);
    Ok(out)
}

/// `sector` with one link's report rate replaced and its CRC redone. Formats 1 to 5 keep one
/// interval for both links, in milliseconds; 6 and later index `REPORT_RATE_HZ`, a byte per link.
pub fn encode_rate(
    sector: &[u8],
    format: u8,
    wired: bool,
    hz: u32,
    limits: Option<RateLimits>,
) -> Result<Vec<u8>, String> {
    let allowed = rates_for(limits, wired);
    if allowed.is_empty() {
        return Err("hrtz doesn't know this mouse's polling rates yet.".into());
    }
    if !allowed.contains(&hz) {
        return Err(format!("{} carries {} Hz.", if wired { "The cable" } else { "The receiver" }, list(&allowed)));
    }
    let l = layout(format);
    let mut out = sector.to_vec();
    if v6(format) {
        let at = if wired { l.rate_wired } else { l.rate_wireless };
        out[at] = REPORT_RATE_HZ.iter().position(|r| *r == hz).unwrap_or(0) as u8;
    } else {
        out[l.rate_wireless] = (1000.0 / hz as f64).round() as u8;
    }
    apply_crc(&mut out);
    Ok(out)
}

/// "125, 250, 500 or 1,000".
fn list(rates: &[u32]) -> String {
    let shown: Vec<String> = rates
        .iter()
        .map(|r| if *r >= 1000 { format!("{},{:03}", r / 1000, r % 1000) } else { r.to_string() })
        .collect();
    match shown.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        _ => shown.join(""),
    }
}

/// The DPI limits for the connected sensor: the format's slots, and the sensor's own range and
/// step (the largest step every value it lists is a multiple of), when it lists any.
pub fn dpi_limits_for_sensor(limits: Option<DpiLimits>, options: &[u32]) -> Option<DpiLimits> {
    let limits = limits?;
    let mut values: Vec<u32> = options.iter().copied().filter(|v| *v > 0).collect();
    values.sort_unstable();
    values.dedup();
    let (Some(&min), Some(&max)) = (values.first(), values.last()) else { return Some(limits) };
    let gcd = values.iter().fold(0, |a, &b| gcd(a, b));
    Some(DpiLimits { max_stages: limits.max_stages, min, max, step: if gcd == 0 { limits.step } else { gcd } })
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}
