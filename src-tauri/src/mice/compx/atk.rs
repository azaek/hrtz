// Facts from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/atk/hid.ts, src/drivers/atk/products.ts,
// src/drivers/atk/device-catalog.generated.ts and src/atk/index.ts. Translated to Rust and modified
// for hrtz, 2026-11-10. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! ATK's mice on CompX's vendor channel (ATK, VXE and MAD, in ATK's own app). They use a Zaopin's
//! packets, commands and memory addresses, and say who they are with their own command (0x10).
//! What's offered is what ATK's own app (ATK HUB 3.2.32) offers and keeps as a Zaopin does, with the
//! values it offers, its buttons and the four onboard profiles; the sensor mode and the sensor
//! angle as ATK's app keeps them, from the firmware it offers each on; the rest (macros) isn't
//! known yet, so it's left as the mouse holds it. The models are in `atk_catalog`, every named one
//! in ATK's app; what's offered on each sensor is `Sensor`'s. Another ATK, or one hrtz can't name,
//! is left alone.

use super::atk_catalog::{MODELS, ON_ATK_VID, ON_COMPX_VID, RECEIVERS};
use super::atk_dpi as dpi;
pub use super::atk_dpi::Method;
use super::image::POLLING;
use super::protocol as p;
use crate::mice::model::{
    Capabilities, DpiCaps, DpiStage, Lod, PollingCaps, Range, SensorAngle, SensorMode, SensorModeCaps, Settings,
    Supported,
};

/// ATK's USB vendor ID, on its receivers and on its mice's cables.
pub const VENDOR_ID: u16 = 0x373B;

/// GetMouseCIDMID: the answer's first two data bytes are the maker (CID) and the model (MID). ATK's
/// own app asks this to name the mouse.
pub const CID_MID: u8 = 0x10;

/// The VXE R1 SE's 1K receiver: there the polling rate is kept in a row of its own
/// (mouse-protocol's "live settings"), not at 0x00, so hrtz doesn't offer it on this one.
pub const R1_SE_RECEIVER: u16 = 0x1085;

/// ATK's generic NearLink receiver, which the U2 Plus comes with: "NK mouse NANO dongle", named
/// "Nearlink Mouse Dongle" before a receiver update.
pub const NEARLINK_RECEIVER: u16 = 0x10C9;

/// An ATK mouse's sensor, as ATK's app names it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sensor {
    Paw3395,
    Paw3395Ultra,
    Paw3950,
    Paw3950Dm,
    Paw3950Ultra,
    Paw3955,
    Paw3395Se,
    Paw3311,
    Paw3315,
    Core26k,
}

impl Sensor {
    /// How its DPI is stored, where hrtz is sure: where two readings of ATK's app agree (see
    /// `atk_dpi`). Elsewhere (None) the DPI stages, their colours and the light aren't shown or
    /// written, and everything else is. Once an owner's report settles one, its arm here turns it on:
    /// - PAW3311 and PAW3315: ATK HUB 3.2.32 looks each DPI up in a table (the PAW3395 SE's, and its
    ///   own); mouse-protocol counts 100 per step from 200. hrtz-notes' atk-dpi-tables.json has both.
    /// - CORE26K: written 50 per step by both, but ATK's app reads two more flags.
    pub fn dpi_method(self) -> Option<Method> {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra | Sensor::Paw3950 | Sensor::Paw3950Dm => Some(Method::Step50),
            Sensor::Paw3950Ultra => Some(Method::Ultra),
            Sensor::Paw3395Se => Some(Method::Table),
            Sensor::Paw3955 => Some(Method::Paw3955),
            Sensor::Paw3311 | Sensor::Paw3315 | Sensor::Core26k => None,
        }
    }

    /// The DPI range hrtz offers, in steps of 50: within what ATK's app offers, and what each
    /// method holds in steps of 50 (a PAW3950 goes on to 36,000 there, a PAW3950 Ultra to 42,000,
    /// a PAW3395 SE to 18,000 and a PAW3955 to 50,000, past where these stop, by other flags).
    fn dpi(self) -> (u32, u32) {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra => (100, 30000),
            Sensor::Paw3950 | Sensor::Paw3950Dm | Sensor::Paw3950Ultra => (50, 30000),
            Sensor::Paw3395Se => (200, 10000),
            Sensor::Paw3955 => (50, 40000),
            Sensor::Paw3311 | Sensor::Paw3315 | Sensor::Core26k => (200, 10000),
        }
    }

    /// The lift-off distances ATK's app offers on this sensor, lowest first, each with its code at
    /// 0x0A: 1 or 2 mm (codes 1 and 2) as a Zaopin; 0.7 mm too (3) on a PAW3950; and on a PAW3950
    /// Ultra and a PAW3955, 0.7 to 1.7 mm a tenth at a time (codes 1 to 11, a slider there). None
    /// on a PAW3311 or a PAW3315.
    fn lod_codes(self) -> Vec<(Lod, u8)> {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra | Sensor::Paw3395Se | Sensor::Core26k => {
                vec![(Lod::MID, 1), (Lod::HIGH, 2)]
            }
            Sensor::Paw3950 | Sensor::Paw3950Dm => vec![(Lod::LOW, 3), (Lod::MID, 1), (Lod::HIGH, 2)],
            Sensor::Paw3950Ultra | Sensor::Paw3955 => (0..11).map(|i| (Lod(7 + i), 1 + i)).collect(),
            Sensor::Paw3311 | Sensor::Paw3315 => Vec::new(),
        }
    }

    /// The lift-off distances offered, lowest first; none where ATK's app offers none.
    fn lod(self) -> Option<Vec<Lod>> {
        let codes = self.lod_codes();
        (!codes.is_empty()).then(|| codes.into_iter().map(|(lod, _)| lod).collect())
    }
}

/// The lift-off distance `image` holds, as `m`'s sensor keeps it; none where it isn't offered, its
/// check byte is off, or its code isn't one of the sensor's.
pub fn lod(m: &Model, image: &[u8]) -> Option<Lod> {
    let code = p::group_valid(&image[LOD..LOD + 2]).then(|| image[LOD])?;
    m.sensor.lod_codes().into_iter().find(|(_, c)| *c == code).map(|(lod, _)| lod)
}

/// `lod` as the lift-off distance, into `target`, as `m`'s sensor keeps it. One it doesn't offer
/// is refused.
pub fn put_lod(m: &Model, target: &mut [u8], lod: Lod) -> Result<(), String> {
    let (_, code) = m
        .sensor
        .lod_codes()
        .into_iter()
        .find(|(l, _)| *l == lod)
        .ok_or_else(|| format!("This mouse doesn't have a lift-off distance of {lod}."))?;
    target[LOD] = code;
    target[LOD + 1] = p::checksum(&[code]);
    Ok(())
}

/// An ATK model hrtz runs.
#[derive(Debug, PartialEq)]
pub struct Model {
    pub cid: u8,
    pub mid: u8,
    /// As ATK's app names it: ATK, VXE or MAD.
    pub brand: &'static str,
    pub name: &'static str,
    pub sensor: Sensor,
    /// It has a DPI light, with a colour per stage ("DpiRgb" in ATK's app).
    pub light: bool,
    /// 0 ms is among its debounce times.
    pub zero_debounce: bool,
    /// It has a button underneath, the DPI button out of the box ("BottomButton" in ATK's app),
    /// set like the five on top.
    pub bottom_button: bool,
    /// The firmware from which ATK's app offers the sensor's top mode, in hundredths (1.25 is
    /// 125): 0 where every firmware has it, none where none does. ATK's own mice (CID 1) have it on
    /// any; the others by ATK's list of upgrades ("cordedMode" in its mouse-upgrade.json).
    pub corded_from: Option<u16>,
    /// Likewise the sensor angle: ATK's own on any firmware, the others from the version that
    /// list gives the angle or the top mode; none where ATK's app says the model has none.
    pub angle_from: Option<u16>,
    /// It has long range ("FarDistance", ATK's "Ultra Long Range Mode"): on unless ATK's app says
    /// the model has none.
    pub long_range: bool,
    /// ATK's own battery figures, in hours of use a charge lasts: at 1,000 and at 8,000 Hz in each
    /// sensor mode (low power, high performance, the top one). ATK's app shows each as a range;
    /// this is its low end, 0 where ATK gives none at that rate. None for a model it gives none for.
    pub battery_h: Option<[[u16; 2]; 3]>,
    /// Tried on the real mouse.
    pub tested: bool,
}

/// ATK's app offers six stages, where a Zaopin has eight slots.
pub const MAX_STAGES: u8 = 6;

pub fn model(cid: u8, mid: u8) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.cid == cid && m.mid == mid)
}

pub fn supported() -> Vec<Supported> {
    MODELS
        .iter()
        .map(|m| Supported {
            family: "compx",
            brand: m.brand,
            model: m.name,
            key: format!("compx/{}-{}", m.cid, m.mid),
            tested: m.tested,
        })
        .collect()
}

/// Whether the connection is a receiver rather than a mouse on its cable: one of ATK's receivers,
/// or named as one ("dongle"); the mice call themselves by name ("ATK U2 Plus Nearlink").
pub fn is_receiver(vendor_id: u16, product_id: u16, product: &str) -> bool {
    let p = product.to_ascii_lowercase();
    RECEIVERS.contains(&(vendor_id, product_id))
        || product_id == NEARLINK_RECEIVER
        || p.contains("dongle")
        || p.contains("receiver")
}

/// Whether a connection under ATK's own vendor ID is one ATK's app runs as these mice: a mouse on
/// a product ID it lists for them, or one of its receivers. ATK's app runs others another way (its
/// "BITMOUSE" mice), and their CID and MID can be one of these models' (in ATK HUB 3.2.33, the F1
/// V3 ULTIMATE+ on 373B:1345 is listed with the A9 Mini's, 1 and 28), so one isn't met at all.
pub fn listed(product_id: u16) -> bool {
    ON_ATK_VID.contains(&product_id) || RECEIVERS.contains(&(VENDOR_ID, product_id))
}

/// Whether a mouse under CompX's own vendor ID (Zaopin's too) that answers as `cid`/`mid` is an
/// ATK hrtz runs: a model it knows, on a product ID ATK's app lists there.
pub fn on_compx_vid(product_id: u16, cid: u8, mid: u8) -> bool {
    ON_COMPX_VID.contains(&product_id) && model(cid, mid).is_some()
}

/// How the mouse is connected, from the handshake's connection type (ATK's app reads the same
/// byte): 0 a 1K receiver, 1 4K, 4 2K, 5 8K, 8 NearLink; 2 a cable at 1K, 3 at 8K, 6 at 2K, 7 at 4K.
/// 0 to 5 mean what they do on a Zaopin.
pub mod link {
    pub const DONGLE_1K: u8 = 0;
    pub const WIRED_1K: u8 = 2;
    pub const NEARLINK: u8 = 8;

    pub fn wired(kind: u8) -> bool {
        matches!(kind, 2 | 3 | 6 | 7)
    }

    /// The fastest polling rate ATK's app offers on it: 8K receivers, NearLink and an 8K cable to
    /// 8,000 Hz, a 4K receiver to 4,000, a 2K one to 2,000; every other kind (a 2K or 4K cable
    /// too, as ATK's app has it) to 1,000.
    pub fn max_polling_hz(kind: u8) -> u32 {
        match kind {
            3 | 5 | 8 => 8000,
            1 => 4000,
            4 => 2000,
            _ => 1000,
        }
    }
}

/// The handshake's connection type, from its answer.
pub fn kind_of(handshake: &p::Packet) -> u8 {
    p::payload(handshake)[6]
}

/// The DPI light's brightness in ATK's app is 16, 128 or 255: a Zaopin's steps 1, 5 and 10.
pub const BRIGHTNESS_STEPS: [u8; 3] = [1, 5, 10];
/// Its breathing speeds: 1, 3 and 5.
pub const SPEEDS: [u8; 3] = [1, 3, 5];
/// Debounce times in ATK's app, in milliseconds (some models start at 1).
pub const DEBOUNCE_MS: [u8; 7] = [0, 1, 2, 4, 8, 15, 20];
/// ATK's app shows four onboard profiles on its CompX mice, read and switched with a Zaopin's
/// commands (0x0E and 0x0F), the index from 0.
pub const PROFILES: u8 = 4;

/// The sleep times ATK's app offers, in seconds, kept at 0xAD in 10 s steps (ATK's app reads its
/// sleep time there). mouse-protocol offers the same on its R1s and gives other ATKs other
/// choices, so hrtz offers these only where the two agree (2026-11-14).
pub const R1_SLEEP_S: [u32; 8] = [30, 60, 120, 180, 300, 1200, 1500, 1800];
const SLEEP: usize = 0xAD;
/// Where ATK's app writes the sleep time a second time: the row of the sensor's own sleep, its
/// switch (0xB5, set to 1), its time (0xB7, in 10 s steps) and the sensor mode (0xB9, written back
/// as held). An owner's R1 PRO held the same 180 at 0xAD and at 0xB7 (2026-11-15).
const SENSOR_SLEEP: usize = 0xB5;
const LOD: usize = 0x0A;
/// The sensor mode, in the row of the sensor's own sleep: 0 ATK's "basic" (low power), 1 its
/// "competitive" (high performance), 2 its "competitive MAX" (the sensor at its limit, "corded" in
/// ATK's code; a Zaopin's is 10 here), and 3 the same held to 13,000 frames a second, on the mice
/// that say they have that choice.
const SENSOR_MODE: usize = 0xB9;
/// The sensor angle: its degrees as a signed byte, then whether it's applied (1), each with its
/// check byte. ATK's app sets the four bytes together.
const ANGLE: usize = 0xBD;
/// ATK's app turns the sensor up to 30 degrees either way.
pub const ANGLE_MAX_DEG: u8 = 30;

/// The sensor mode `image` holds; none where its check byte is off or it's a code hrtz doesn't
/// know. The top mode held to 13,000 frames a second reads as the top mode.
pub fn sensor_mode(image: &[u8]) -> Option<SensorMode> {
    match p::group_valid(&image[SENSOR_MODE..SENSOR_MODE + 2]).then(|| image[SENSOR_MODE])? {
        0 => Some(SensorMode::Lp),
        1 => Some(SensorMode::Hp),
        2 | 3 => Some(SensorMode::Corded),
        _ => None,
    }
}

/// `mode` as the sensor mode, into `target`. The row it's in is written whole, the sensor's sleep
/// beside it as held, as ATK's app writes it.
pub fn put_sensor_mode(target: &mut [u8], mode: SensorMode) {
    let code = match mode {
        SensorMode::Lp => 0,
        SensorMode::Hp => 1,
        SensorMode::Corded => 2,
    };
    target[SENSOR_MODE] = code;
    target[SENSOR_MODE + 1] = p::checksum(&[code]);
}

/// The sensor angle `image` holds; none where a check byte is off, or the degrees are past what
/// ATK's app sets.
pub fn angle(image: &[u8]) -> Option<SensorAngle> {
    let valid = p::group_valid(&image[ANGLE..ANGLE + 2]) && p::group_valid(&image[ANGLE + 2..ANGLE + 4]);
    let degrees = image[ANGLE] as i8;
    (valid && degrees.unsigned_abs() <= ANGLE_MAX_DEG).then_some(SensorAngle { on: image[ANGLE + 2] == 1, degrees })
}

/// `angle` as the sensor angle, into `target`. One past `max` degrees either way is refused.
pub fn put_angle(target: &mut [u8], angle: SensorAngle, max: u8) -> Result<(), String> {
    if angle.degrees.unsigned_abs() > max {
        return Err(format!("The sensor angle goes from -{max} to {max} degrees."));
    }
    for (at, value) in [(ANGLE, angle.degrees as u8), (ANGLE + 2, angle.on as u8)] {
        target[at] = value;
        target[at + 1] = p::checksum(&[value]);
    }
    Ok(())
}

/// A firmware version as hrtz shows it ("1.25"), in hundredths; none where it isn't two numbers.
fn hundredths(firmware: &str) -> Option<u16> {
    let (major, minor) = firmware.split_once('.')?;
    (minor.len() == 2).then_some(())?;
    Some(major.parse::<u16>().ok()?.checked_mul(100)? + minor.parse::<u16>().ok()?)
}

/// How long a charge lasts on `m` in `mode` (low power where it isn't known, as ATK's app has it)
/// at the rates of `rates` ATK gives a figure for, by ATK's own figures; none without one.
pub fn rated_hours(m: &Model, mode: Option<SensorMode>, rates: &[u32]) -> Option<Vec<(u32, f64)>> {
    let [lp, hp, top] = m.battery_h?;
    let [at_1k, at_8k] = match mode.unwrap_or(SensorMode::Lp) {
        SensorMode::Lp => lp,
        SensorMode::Hp => hp,
        SensorMode::Corded => top,
    };
    let hours: Vec<(u32, f64)> = [(1000, at_1k), (8000, at_8k)]
        .into_iter()
        .filter(|(hz, h)| *h > 0 && rates.contains(hz))
        .map(|(hz, h)| (hz, h as f64))
        .collect();
    (!hours.is_empty()).then_some(hours)
}

/// What `m`'s firmware adds to what's offered, as ATK's app decides it: the sensor's top mode and
/// the sensor angle, each from the firmware that has it. A mouse that hasn't said its firmware gets
/// neither.
pub fn with_firmware(m: &Model, caps: &mut Capabilities, firmware: Option<&str>) {
    let version = firmware.and_then(hundredths);
    let has = |from: Option<u16>| matches!((from, version), (Some(from), Some(v)) if v >= from);
    if let Some(mode) = caps.sensor_mode.as_mut() {
        mode.modes = vec![SensorMode::Lp, SensorMode::Hp];
        if has(m.corded_from) {
            mode.modes.push(SensorMode::Corded);
        }
    }
    caps.sensor_angle_deg = has(m.angle_from).then_some(ANGLE_MAX_DEG);
}

/// The models mouse-protocol gives the R1's sleep choices: its R1 family (R1, R1 PRO MAX, R1 SE+)
/// and the two sharing the R1's settings (X1 Pro Max, A9 Plus NK). Any ATK met under CompX's
/// vendor ID counts as an R1 there too (`on_compx_product`).
const R1_SLEEP_MODELS: [(u8, u8); 6] = [(2, 11), (2, 12), (2, 27), (2, 32), (2, 39), (2, 83)];

/// Whether `product_id` is one ATK's app lists under CompX's vendor ID (VXE's R1 line, ATK's
/// Dragonfly F1): mouse-protocol's shared R1 transport.
pub fn on_compx_product(product_id: u16) -> bool {
    ON_COMPX_VID.contains(&product_id)
}

/// The sleep time `image` holds, in seconds; None where its check byte is off.
pub fn sleep_s(image: &[u8]) -> Option<u32> {
    p::group_valid(&image[SLEEP..SLEEP + 2]).then(|| image[SLEEP] as u32 * 10)
}

/// `seconds` as the sleep time, into `target`, in both places ATK's app sets it: the row from
/// 0xA9, and the sensor's own sleep from 0xB5, switched on. Each row is written whole.
pub fn put_sleep(target: &mut [u8], seconds: u32) {
    let v = (seconds / 10) as u8;
    for (at, value) in [(SLEEP, v), (SENSOR_SLEEP, 1), (SENSOR_SLEEP + 2, v)] {
        target[at] = value;
        target[at + 1] = p::checksum(&[value]);
    }
}

/// What hrtz offers on an ATK, before it's tested: what ATK's app offers and keeps where a Zaopin
/// does. The polling rate (to what the connection allows); the DPI stages (packed as a Zaopin's),
/// and on a model with a DPI light, their colours and the light; the lift-off distance its sensor
/// offers; debounce, motion sync, straight-line correction (a Zaopin's angle snapping) and ripple
/// control; the sensor's low-power and high-performance modes; long range, where the model has
/// it; the onboard profiles. The battery is the mouse's own reading, and how long a charge lasts
/// ATK's own figure where it gives one (`rated_hours`, set with the sensor mode it's in). Its top sensor mode and the sensor angle wait for its firmware
/// (`with_firmware`).
pub fn capabilities(m: &Model, kind: u8) -> Capabilities {
    let (min, max) = m.sensor.dpi();
    let debounce: Vec<u8> = DEBOUNCE_MS.iter().copied().filter(|ms| m.zero_debounce || *ms > 0).collect();
    Capabilities {
        wired: link::wired(kind),
        dpi: m.sensor.dpi_method().map(|_| DpiCaps {
            min,
            max,
            step: 50,
            max_stages: MAX_STAGES,
            colors: m.light,
            light: m.light,
            brightness_steps: m.light.then(|| BRIGHTNESS_STEPS.to_vec()),
            speeds: m.light.then(|| SPEEDS.to_vec()),
            above: None,
        }),
        polling: Some(PollingCaps {
            rates: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= link::max_polling_hz(kind)).collect(),
        }),
        lod: m.sensor.lod(),
        sensor_mode: Some(SensorModeCaps {
            modes: vec![SensorMode::Lp, SensorMode::Hp],
            corded_from_hz: None,
            corded_when_wired: false,
            corded_by_choice: true,
        }),
        debounce_ms: Some(Range { min: debounce[0] as u32, max: 20 }),
        debounce_choices_ms: Some(debounce),
        motion_sync: true,
        angle_snapping: true,
        ripple_control: true,
        profiles: Some(PROFILES),
        long_range: m.long_range,
        sleep_times_s: R1_SLEEP_MODELS.contains(&(m.cid, m.mid)).then(|| R1_SLEEP_S.to_vec()),
        battery: true,
        // ATK's app hears of a DPI-button press from the mouse (0x0A), as hrtz does; the driver
        // also reads the stage every second, in case this one doesn't say.
        dpi_button: true,
        buttons: Some(buttons(m)),
        ..Capabilities::default()
    }
}

/// The buttons ATK's app lists on every CompX mouse, by their place in its memory, kept as a
/// Zaopin's are (four bytes each from 0x60, the same codes; a key combination in the slot from
/// 0x100): the five on top, and the one underneath where the model has it.
pub fn buttons(m: &Model) -> Vec<(u8, String)> {
    let names = ["Left", "Right", "Wheel click", "Back", "Forward", "DPI"];
    let count = if m.bottom_button { 6 } else { 5 };
    names.iter().take(count).enumerate().map(|(i, n)| (i as u8, n.to_string())).collect()
}

const BUTTONS: usize = 0x60;
const SHORTCUTS: usize = 0x100;
const SHORTCUT_LEN: usize = 0x20;

/// A button set to a key combination or a media key, as ATK's app writes it: the type alone, with
/// what it sends in its slot. A Zaopin's app also keeps a media key's code beside the type, which
/// ATK's doesn't.
pub fn keys_as_atks_app(target: &mut [u8], m: &Model) {
    for (i, _) in buttons(m) {
        let b = BUTTONS + 4 * i as usize;
        if target[b] == 5 && (target[b + 1], target[b + 2]) != (0, 0) && p::group_valid(&target[b..b + 4]) {
            target[b + 1..b + 4].copy_from_slice(&[0, 0, p::checksum(&[5, 0, 0])]);
        }
    }
}

/// Where what's offered on `m` is kept, as `(address, length)`, in the groups ATK's app writes
/// (mouse-protocol's captures of an R1 Pro Max show the same), so no check byte is left behind
/// for a moment: the system row (polling rate, stage count, stage in use, and what ATK keeps beside
/// them, written back as held); the lift-off distance, where offered; the DPI stages two at a
/// time (a PAW3955's one at a time, from 0x1B00); on a model with a DPI light, the colours two at
/// a time and the light; the row from debounce to ripple control, which holds ATK's sleep time
/// too (0xAD: set where it's offered, elsewhere written back as held); the sensor's own sleep
/// from 0xB5, which ATK's app sets with the sleep time, and which holds the sensor mode; the
/// sensor angle's four bytes, on a model that has it; and each button, with its key-combination
/// slot in two packets of ten. Nothing of the DPI where it isn't offered.
pub fn fields(m: &Model) -> Vec<(usize, usize)> {
    let stages = MAX_STAGES as usize;
    let mut f = vec![(0x00, 10)];
    if m.sensor.lod().is_some() {
        f.push((0x0A, 2));
    }
    match m.sensor.dpi_method() {
        Some(Method::Paw3955) => f.extend((0..stages).map(|i| (dpi::stage_addr(Method::Paw3955, i), 6))),
        Some(_) => f.extend((0..stages / 2).map(|i| (0x0C + 8 * i, 8))),
        None => {}
    }
    if m.light && m.sensor.dpi_method().is_some() {
        f.extend((0..stages / 2).map(|i| (0x2C + 8 * i, 8)));
        f.push((0x4C, 8));
    }
    f.push((0xA9, 10));
    f.push((SENSOR_SLEEP, 6));
    if m.angle_from.is_some() {
        f.push((ANGLE, 4));
    }
    // Each button alone, and its key-combination slot in ATK's two packets of ten.
    for (i, _) in buttons(m) {
        f.push((BUTTONS + 4 * i as usize, 4));
        let slot = SHORTCUTS + SHORTCUT_LEN * i as usize;
        f.extend([(slot, 10), (slot + 10, 10)]);
    }
    f
}

/// The stages' DPI, as `m`'s sensor stores them (a Zaopin's decoding is right only for some):
/// into `stages`, from the mouse's memory. A stage whose check byte is off keeps what it had.
pub fn read_stages(m: &Model, image: &[u8], stages: &mut [DpiStage]) {
    let Some(method) = m.sensor.dpi_method() else { return };
    for (i, st) in stages.iter_mut().enumerate().take(MAX_STAGES as usize) {
        let a = dpi::stage_addr(method, i);
        if let Some(d) = image.get(a..a + dpi::stage_len(method)).and_then(|b| dpi::decode_stage(method, b)) {
            st.dpi = d;
        }
    }
}

/// Writes into `target` each stage whose DPI changed from `was` to `now`, as `m`'s sensor stores
/// it. A DPI it can't hold is refused.
pub fn write_stages(m: &Model, target: &mut [u8], was: &[DpiStage], now: &[DpiStage]) -> Result<(), String> {
    let Some(method) = m.sensor.dpi_method() else { return Ok(()) };
    for (i, st) in now.iter().enumerate().take(MAX_STAGES as usize) {
        if was.get(i).map(|w| w.dpi) == Some(st.dpi) {
            continue;
        }
        let bytes =
            dpi::encode_stage(method, st.dpi).ok_or_else(|| format!("{} DPI can't be set on this mouse.", st.dpi))?;
        let a = dpi::stage_addr(method, i);
        target[a..a + bytes.len()].copy_from_slice(&bytes);
    }
    Ok(())
}

/// How far hrtz keeps the mouse's memory for `m`: a PAW3955's stages are past a Zaopin's flash.
pub fn memory_len(m: &Model) -> usize {
    if m.sensor.dpi_method() == Some(Method::Paw3955) {
        dpi::PAW3955_STAGES + 6 * 8
    } else {
        super::protocol::FLASH_SIZE
    }
}

/// Only what an ATK offers and keeps as a Zaopin does, of `s`: the rest is left as the mouse holds
/// it. A model without a DPI light shows none; one whose DPI method isn't settled shows no stages;
/// one whose lift-off isn't offered, none. Its sensor mode and angle, kept ATK's way, are the
/// driver's to read and set.
pub fn offered(s: &Settings, m: &Model) -> Settings {
    let dpi = m.sensor.dpi_method().is_some();
    Settings {
        polling_hz: s.polling_hz,
        dpi_stage_count: s.dpi_stage_count.filter(|_| dpi),
        dpi_stage: s.dpi_stage.filter(|_| dpi),
        stages: s.stages.clone().filter(|_| dpi),
        dpi_light: s.dpi_light.filter(|_| m.light && dpi),
        lod: s.lod.filter(|_| m.sensor.lod().is_some()),
        debounce_ms: s.debounce_ms,
        motion_sync: s.motion_sync,
        angle_snapping: s.angle_snapping,
        ripple_control: s.ripple_control,
        buttons: s.buttons.clone(),
        ..Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_mouse_in_atks_app_is_a_model() {
        assert_eq!(MODELS.len(), 160);
        assert_eq!(model(2, 78).map(|m| m.name), Some("U2 Plus NK (PAW3395)"));
        assert_eq!(model(2, 7).map(|m| (m.brand, m.name)), Some(("VXE", "R1 PRO (PAW3395)")));
        // The U2 Plus NK with a PAW3950 Ultra: its own name, and its own DPI method.
        let u2u = model(2, 124).unwrap();
        assert_eq!((u2u.name, u2u.sensor.dpi_method()), ("U2 Plus NK (PAW3950 Ultra)", Some(Method::Ultra)));
        // ATK's unreleased "Mouse" entries, and a Zaopin's IDs, mean nothing here.
        assert!(model(1, 86).is_none() && model(7, 4).is_none());
        // One model per CID and MID. Tested: the R1 PRO with a PAW3950 Ultra, whose check passed on
        // its 1K receiver and its cable (hrtz 0.5.2, 2026-11-15); the R1 PRO MAX with a PAW3395,
        // whose check passed on its 4K receiver (hrtz 0.5.5, 2026-11-17, MID 28), with the two other
        // IDs ATK's app gives that name, sensor, USB ID and every feature (27 and 34); the R1 with a
        // PAW3395, whose check passed on its 1K receiver (hrtz 0.5.7, 2026-11-18, MID 12), with the
        // other ID ATK's app gives it (11).
        let tested = [(2, 125), (2, 27), (2, 28), (2, 34), (2, 11), (2, 12)];
        for (i, a) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|b| (a.cid, a.mid) != (b.cid, b.mid)), "{a:?}");
            assert!(["ATK", "VXE", "MAD"].contains(&a.brand));
            assert_eq!(a.tested, tested.contains(&(a.cid, a.mid)), "{a:?}");
        }
    }

    #[test]
    fn the_sleep_time_where_atks_app_and_mouse_protocol_agree() {
        let caps = |cid, mid| capabilities(model(cid, mid).unwrap(), link::DONGLE_1K).sleep_times_s;
        assert_eq!(caps(2, 27).as_deref(), Some(&R1_SLEEP_S[..]), "R1 PRO MAX");
        assert_eq!(caps(2, 83).as_deref(), Some(&R1_SLEEP_S[..]), "A9 Plus NK, with the R1's settings");
        // On ATK's own vendor ID, mouse-protocol offers an R1 PRO (PAW3950 Ultra) other times.
        assert_eq!(caps(2, 125), None);
        assert_eq!(caps(2, 78), None, "U2 Plus");
        // Under CompX's, the driver offers them on any (an owner's receiver).
        assert!(on_compx_product(0xF58A) && !on_compx_product(0x10C9));
    }

    #[test]
    fn a_receiver_is_told_from_the_mouse_on_its_cable() {
        assert!(is_receiver(0x373B, 0x10C9, "NK mouse NANO dongle"));
        assert!(is_receiver(0x373B, 0x101B, "anything"), "listed by ATK's app");
        assert!(is_receiver(0x373B, 0x1234, "Nearlink Mouse Dongle"));
        assert!(!is_receiver(0x373B, 0x110D, "ATK U2 Plus Nearlink"));
    }

    #[test]
    fn under_atks_vid_only_the_ids_atks_app_lists_for_these_mice() {
        assert!(listed(0x1272), "the R1 PRO with a PAW3950 Ultra, on its cable");
        assert!(listed(NEARLINK_RECEIVER) && listed(R1_SE_RECEIVER), "receivers");
        // ATK's other kind of mice: the F1 V3 ULTIMATE+ (listed with the A9 Mini's CID and MID),
        // the U2 Plus NK on 373B:12E3, an R1 S NK on 373B:1310.
        assert!(!listed(0x1345) && !listed(0x12E3) && !listed(0x1310));
    }

    #[test]
    fn under_compx_vid_only_atks_listed_models_on_its_ids() {
        assert!(on_compx_vid(0xF58C, 2, 7), "VXE R1 PRO");
        assert!(!on_compx_vid(0xF58C, 1, 86), "a model hrtz doesn't run (ATK's unreleased \"Mouse\")");
        assert!(!on_compx_vid(0xF5D5, 2, 7), "a product ID ATK's app doesn't list there");
    }

    #[test]
    fn it_offers_what_atks_app_offers_with_its_values() {
        let u2 = model(2, 78).unwrap();
        let caps = capabilities(u2, link::NEARLINK);
        assert!(!caps.wired);
        assert_eq!(caps.polling.unwrap().rates, vec![125, 250, 500, 1000, 2000, 4000, 8000]);
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max, dpi.step, dpi.max_stages), (100, 30000, 50, 6));
        assert!(dpi.colors && dpi.light);
        assert_eq!(dpi.brightness_steps, Some(vec![1, 5, 10]));
        assert_eq!(dpi.speeds, Some(vec![1, 3, 5]));
        assert_eq!(caps.lod, Some(vec![Lod::MID, Lod::HIGH]));
        assert_eq!(caps.debounce_choices_ms, Some(vec![0, 1, 2, 4, 8, 15, 20]));
        assert!(caps.motion_sync && caps.angle_snapping && caps.ripple_control);
        assert_eq!(caps.profiles, Some(4));
        // Its sensor's two modes on any firmware, chosen as ATK's app chooses them. The top one
        // and the sensor angle wait for its firmware.
        let mode = caps.sensor_mode.clone().unwrap();
        assert_eq!(mode.modes, vec![SensorMode::Lp, SensorMode::Hp]);
        assert!(mode.corded_by_choice && !mode.corded_when_wired && mode.corded_from_hz.is_none());
        assert!(caps.sensor_angle_deg.is_none());
        // Kept differently, or not known yet.
        assert!(caps.sleep_times_s.is_none() && caps.peak_timeouts_s.is_none());
        // Its buttons, as ATK's app lists them: the five on top and the one underneath.
        let names: Vec<String> = caps.buttons.unwrap().into_iter().map(|(_, n)| n).collect();
        assert_eq!(names, ["Left", "Right", "Wheel click", "Back", "Forward", "DPI"]);
        // One without the button underneath (the MAD R) lists five.
        assert_eq!(buttons(model(2, 49).unwrap()).len(), 5);
        assert!(!caps.pairing && !caps.factory_reset && !caps.backups);
        // On a cable at 1K, and a 1K receiver.
        let wired = capabilities(u2, link::WIRED_1K);
        assert!(wired.wired && wired.polling.unwrap().rates.last() == Some(&1000));
        assert_eq!(capabilities(u2, link::DONGLE_1K).polling.unwrap().rates.last(), Some(&1000));
    }

    #[test]
    fn the_firmware_settles_the_top_mode_and_the_sensor_angle() {
        // Whether the top mode is offered, and how far the sensor turns.
        let offered = |cid, mid, firmware: Option<&str>| {
            let m = model(cid, mid).unwrap();
            let mut caps = capabilities(m, link::DONGLE_1K);
            with_firmware(m, &mut caps, firmware);
            // Asked again (a profile switch loads again), it's the same.
            let once = caps.clone();
            with_firmware(m, &mut caps, firmware);
            assert_eq!(caps, once);
            (caps.sensor_mode.unwrap().modes.contains(&SensorMode::Corded), caps.sensor_angle_deg)
        };
        // The R1 PRO with a PAW3950 Ultra: both from firmware 1.00 (an owner's had 1.25).
        assert_eq!(offered(2, 125, Some("1.25")), (true, Some(30)));
        assert_eq!(offered(2, 125, Some("1.00")), (true, Some(30)));
        assert_eq!(offered(2, 125, Some("0.99")), (false, None));
        // A mouse that hasn't said its firmware, or says one hrtz can't read, gets neither.
        assert_eq!(offered(2, 125, None), (false, None));
        assert_eq!(offered(2, 125, Some("1.2a")), (false, None));
        // The U2 Plus NK: the angle from 1.30, and no top mode on any.
        assert_eq!(offered(2, 78, Some("1.23")), (false, None));
        assert_eq!(offered(2, 78, Some("1.30")), (false, Some(30)));
        // The U2 Pro Max from 1.77, the F1 PRO from 3.00.
        assert_eq!(offered(2, 100, Some("1.76")), (false, None));
        assert_eq!(offered(2, 100, Some("1.77")), (true, Some(30)));
        assert_eq!(offered(2, 36, Some("2.99")), (false, None));
        assert_eq!(offered(2, 36, Some("3.00")), (true, Some(30)));
        // ATK's own (CID 1): both on any firmware.
        assert_eq!(offered(1, 2, Some("0.01")), (true, Some(30)));
        // One ATK's list of upgrades doesn't name (the R1 PRO with a PAW3395): neither, on any.
        assert_eq!(offered(2, 7, Some("9.99")), (false, None));
        // How many have each: ATK's own 62 and the 38 its list names; the angle on two more.
        assert_eq!(MODELS.iter().filter(|m| m.corded_from.is_some()).count(), 100);
        assert_eq!(MODELS.iter().filter(|m| m.angle_from.is_some()).count(), 102);
        assert!(MODELS.iter().all(|m| m.corded_from.is_none() || m.angle_from.is_some()));
    }

    #[test]
    fn long_range_where_atks_app_has_it() {
        assert!(capabilities(model(2, 125).unwrap(), link::DONGLE_1K).long_range, "the R1 PRO");
        // Its features say no on the U2 Plus NK with a PAW3950 Ultra.
        assert!(!capabilities(model(2, 124).unwrap(), link::NEARLINK).long_range);
        assert_eq!(MODELS.iter().filter(|m| m.long_range).count(), 137);
    }

    #[test]
    fn a_charge_lasts_by_atks_figures_in_the_mode_in_use() {
        // The Duckbill Ultra: 160 to 200 h at 1K in low power, 34 to 42 at 8K; 152 and 33 in high
        // performance; 45 and 27 in its top mode. The low end, at the rates the connection has.
        let m = model(1, 2).unwrap();
        let all = [125, 250, 500, 1000, 2000, 4000, 8000];
        assert_eq!(rated_hours(m, Some(SensorMode::Lp), &all), Some(vec![(1000, 160.0), (8000, 34.0)]));
        assert_eq!(rated_hours(m, None, &all), Some(vec![(1000, 160.0), (8000, 34.0)]), "low power, unknown");
        assert_eq!(rated_hours(m, Some(SensorMode::Hp), &all), Some(vec![(1000, 152.0), (8000, 33.0)]));
        assert_eq!(rated_hours(m, Some(SensorMode::Corded), &all), Some(vec![(1000, 45.0), (8000, 27.0)]));
        assert_eq!(rated_hours(m, Some(SensorMode::Lp), &all[..4]), Some(vec![(1000, 160.0)]), "on 1K, no 8K figure");
        // The U2 Pro Max (2,100): ATK gives a range across both rates only (44 h at 8K to 250 at
        // 1K), so no low figure at 1K.
        let u2pm = model(2, 100).unwrap();
        assert_eq!(u2pm.battery_h.map(|b| b[0]), Some([0, 44]));
        assert_eq!(rated_hours(u2pm, None, &all), Some(vec![(8000, 44.0)]));
        assert_eq!(rated_hours(u2pm, None, &all[..4]), None);
        // None for a model ATK gives none for: the R1 PRO and the U2 Plus NK.
        assert_eq!(rated_hours(model(2, 125).unwrap(), None, &all), None);
        assert_eq!(rated_hours(model(2, 78).unwrap(), None, &all), None);
        assert_eq!(MODELS.iter().filter(|m| m.battery_h.is_some()).count(), 53);
    }

    #[test]
    fn the_sensor_mode_and_angle_as_atks_app_keeps_them() {
        let mut f = vec![0xFF; 0x100];
        assert_eq!((sensor_mode(&f), angle(&f)), (None, None), "blank memory holds neither");
        for (code, mode) in [(0, SensorMode::Lp), (1, SensorMode::Hp), (2, SensorMode::Corded), (3, SensorMode::Corded)]
        {
            f[0xB9..0xBB].copy_from_slice(&[code, p::checksum(&[code])]);
            assert_eq!(sensor_mode(&f), Some(mode));
        }
        f[0xB9..0xBB].copy_from_slice(&[10, p::checksum(&[10])]);
        assert_eq!(sensor_mode(&f), None, "a Zaopin's Corded isn't one of an ATK's");
        put_sensor_mode(&mut f, SensorMode::Corded);
        assert_eq!(f[0xB9..0xBB], [2, 0x53]);
        // An owner's R1 PRO held FF 56 01 54 from 0xBD: one degree to the left, applied.
        f[0xBD..0xC1].copy_from_slice(&[0xFF, 0x56, 0x01, 0x54]);
        assert_eq!(angle(&f), Some(SensorAngle { on: true, degrees: -1 }));
        put_angle(&mut f, SensorAngle { on: false, degrees: 30 }, 30).unwrap();
        assert_eq!(f[0xBD..0xC1], [30, 0x55 - 30, 0, 0x55]);
        assert_eq!(angle(&f), Some(SensorAngle { on: false, degrees: 30 }));
        put_angle(&mut f, SensorAngle { on: true, degrees: -30 }, 30).unwrap();
        assert_eq!(f[0xBD..0xC1], [226, p::checksum(&[226]), 1, 0x54]);
        let held = f.clone();
        assert!(put_angle(&mut f, SensorAngle { on: true, degrees: 31 }, 30).is_err());
        assert_eq!(f, held, "a refused angle writes nothing");
        // Degrees past what ATK's app sets, or a check byte that's off, read as nothing to show.
        f[0xBD..0xBF].copy_from_slice(&[0x80, p::checksum(&[0x80])]);
        assert_eq!(angle(&f), None);
        f[0xBD..0xBF].copy_from_slice(&[5, 0]);
        assert_eq!(angle(&f), None);
    }

    #[test]
    fn each_model_gets_its_sensors_range_and_its_own_light_and_debounce() {
        // A PAW3950: from 50 DPI, and 0.7 mm lift-off.
        let caps = capabilities(model(2, 36).unwrap(), link::DONGLE_1K);
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max), (50, 30000));
        assert_eq!(caps.lod, Some(vec![Lod::LOW, Lod::MID, Lod::HIGH]));
        // The A9 Plus NK has no DPI light: its colours and light aren't offered or written.
        let a9 = model(2, 83).unwrap();
        let dpi = capabilities(a9, link::DONGLE_1K).dpi.unwrap();
        assert!(!dpi.light && !dpi.colors && dpi.brightness_steps.is_none());
        assert!(fields(a9).iter().all(|(a, _)| !(0x2C..0x54).contains(a)));
        // The A9 Ultra Max 2.0 has none either, and its debounce starts at 1 ms.
        let a9u = capabilities(model(1, 6).unwrap(), link::DONGLE_1K);
        assert_eq!(a9u.debounce_choices_ms, Some(vec![1, 2, 4, 8, 15, 20]));
        assert_eq!(a9u.debounce_ms.unwrap().min, 1);
    }

    #[test]
    fn where_a_sensors_dpi_isnt_settled_its_stages_arent_offered_but_the_rest_is() {
        // The VXE R1S+, with a PAW3311.
        let r1s = model(2, 63).unwrap();
        assert_eq!(r1s.sensor, Sensor::Paw3311);
        let caps = capabilities(r1s, link::DONGLE_1K);
        assert!(caps.dpi.is_none() && caps.lod.is_none());
        assert!(caps.polling.is_some() && caps.debounce_choices_ms.is_some() && caps.motion_sync);
        assert_eq!(caps.profiles, Some(4));
        let f = fields(r1s);
        assert_eq!(f[..3], [(0x00, 10), (0xA9, 10), (0xB5, 6)], "nothing of the DPI or lift-off is written");
        // Then each button, and its key-combination slot in two packets.
        assert_eq!(f[3..6], [(0x60, 4), (0x100, 10), (0x10A, 10)]);
        assert_eq!(f.len(), 3 + 3 * 6);
        // The sensor angle's four bytes only on a model that has it, whatever its firmware.
        assert!(!f.contains(&(0xBD, 4)));
        assert!(
            fields(model(2, 125).unwrap()).contains(&(0xBD, 4)) && fields(model(2, 78).unwrap()).contains(&(0xBD, 4))
        );
        // A PAW3950 Ultra's lift-off runs 0.7 to 1.7 mm a tenth at a time, as ATK's app has it.
        let u2u = capabilities(model(2, 124).unwrap(), link::NEARLINK);
        assert_eq!(u2u.lod, Some((7..=17).map(Lod).collect()));
        assert!(u2u.dpi.is_some_and(|d| (d.min, d.max) == (50, 30000)));
        // A PAW3955's stages are written one at a time from 0x1B00, and its memory goes that far.
        let p3955 = MODELS.iter().find(|m| m.sensor == Sensor::Paw3955).unwrap();
        assert!(fields(p3955).contains(&(0x1B00, 6)) && !fields(p3955).contains(&(0x0C, 8)));
        assert_eq!(memory_len(p3955), 0x1B30);
    }

    #[test]
    fn the_link_says_wired_and_how_fast() {
        assert!(link::wired(2) && link::wired(3) && link::wired(6) && link::wired(7));
        assert!(!link::wired(0) && !link::wired(5) && !link::wired(8));
        let fastest = [0, 1, 2, 3, 4, 5, 6, 7, 8].map(link::max_polling_hz);
        assert_eq!(fastest, [1000, 4000, 1000, 8000, 2000, 8000, 1000, 1000, 8000]);
    }

    #[test]
    fn every_field_is_whole_and_apart() {
        for m in MODELS.iter() {
            let mut f = fields(m);
            f.sort();
            assert!(f.windows(2).all(|w| w[0].0 + w[0].1 <= w[1].0), "overlapping: {f:?}");
            assert!(f.iter().all(|(_, n)| *n <= super::super::protocol::MAX_CHUNK), "one packet each");
        }
    }
}
