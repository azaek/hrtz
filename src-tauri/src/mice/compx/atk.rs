// Facts from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/atk/hid.ts, src/drivers/atk/products.ts,
// src/drivers/atk/device-catalog.generated.ts and src/atk/index.ts. Translated to Rust and modified
// for hrtz, 2026-11-10. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! ATK's mice on CompX's vendor channel. They use a Zaopin's packets, commands and memory addresses,
//! but ask for the model with their own command (0x10). What's offered is what ATK's own app
//! (ATK HUB 3.2.32) offers on the model and keeps as a Zaopin does, with the values it offers, and
//! its four onboard profiles; the rest (sleep, the sensor mode, the buttons) is kept differently or
//! isn't known yet, so it's left as the mouse holds it. hrtz claims only the models an owner has helped with: the U2 Plus with a
//! PAW3395, from snowyuonA's two reports (2026-11-10). Another ATK model, or one hrtz can't name,
//! is left alone.

use super::image::POLLING;
use crate::mice::model::{Capabilities, DpiCaps, Lod, PollingCaps, Range, Settings, Supported};

/// ATK's USB vendor ID, on its receivers and on its mice's cables.
pub const VENDOR_ID: u16 = 0x373B;

/// GetMouseCIDMID: the answer's first two data bytes are the maker (CID) and the model (MID). ATK's
/// own app asks this; a Zaopin's handshake (0x01) isn't used.
pub const CID_MID: u8 = 0x10;

/// ATK's generic NearLink receiver, which the U2 Plus comes with: "NK mouse NANO dongle", named
/// "Nearlink Mouse Dongle" before a receiver update.
pub const NEARLINK_RECEIVER: u16 = 0x10C9;

/// The vendor's connection kinds (see `models::link`), for the UI's name of the link.
const WIRELESS_1K: u8 = 0;
const WIRED_1K: u8 = 2;

/// An ATK model hrtz runs.
#[derive(Debug, PartialEq)]
pub struct Model {
    pub cid: u8,
    pub mid: u8,
    pub name: &'static str,
    /// Its sensor's DPI range in ATK's app, in steps of 50.
    pub min_dpi: u32,
    pub max_dpi: u32,
    pub max_polling_hz: u32,
    /// Tried on the real mouse.
    pub tested: bool,
}

/// ATK's app offers six stages, where a Zaopin has eight slots.
pub const MAX_STAGES: u8 = 6;

/// The U2 Plus has a PAW3395, a PAW3950 Ultra or a NearLink PAW3950 Ultra, each with its own MID
/// (78, 124 and 25 in ATK's catalog). snowyuonA's cable says `373B:110D`, the catalog's PAW3395
/// one. The PAW3395 packs its DPI as a Zaopin's does: 50-DPI steps, up to 30,000 in ATK's app.
/// 1,000 Hz is the most the U2 Plus reports.
pub const MODELS: [Model; 1] = [Model {
    cid: 2,
    mid: 78,
    name: "U2 Plus (PAW3395)",
    min_dpi: 100,
    max_dpi: 30000,
    max_polling_hz: 1000,
    tested: false,
}];

pub fn model(cid: u8, mid: u8) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.cid == cid && m.mid == mid)
}

pub fn supported() -> Vec<Supported> {
    MODELS
        .iter()
        .map(|m| Supported {
            family: "compx",
            brand: "ATK",
            model: m.name,
            key: format!("compx/{}-{}", m.cid, m.mid),
            tested: m.tested,
        })
        .collect()
}

/// Whether the connection is a receiver rather than the mouse on its cable. The mice call
/// themselves by name ("ATK U2 Plus Nearlink"); the receivers are "dongles".
pub fn is_receiver(product_id: u16, product: &str) -> bool {
    let p = product.to_ascii_lowercase();
    product_id == NEARLINK_RECEIVER || p.contains("dongle") || p.contains("receiver")
}

/// The connection kind the UI names the link by.
pub fn device_type(receiver: bool) -> u8 {
    if receiver { WIRELESS_1K } else { WIRED_1K }
}

/// The DPI light's brightness in ATK's app is 16, 128 or 255: a Zaopin's steps 1, 5 and 10.
pub const BRIGHTNESS_STEPS: [u8; 3] = [1, 5, 10];
/// Its breathing speeds: 1, 3 and 5.
pub const SPEEDS: [u8; 3] = [1, 3, 5];
/// Debounce times in ATK's app, in milliseconds.
pub const DEBOUNCE_MS: [u8; 7] = [0, 1, 2, 4, 8, 15, 20];
/// ATK's app shows four onboard profiles on its CompX mice, read and switched with a Zaopin's
/// commands (0x0E and 0x0F), the index from 0.
pub const PROFILES: u8 = 4;

/// What hrtz offers on an ATK, before it's tested: what ATK's app offers and keeps where a Zaopin
/// does. The polling rate; the DPI stages (packed as a Zaopin's), their colours and light; the
/// lift-off distance (1 or 2 mm: 0.7 mm is only for a PAW3950); debounce, motion sync,
/// straight-line correction (a Zaopin's angle snapping) and ripple control; the onboard profiles.
/// The battery is the mouse's own reading.
pub fn capabilities(m: &Model, receiver: bool) -> Capabilities {
    Capabilities {
        wired: !receiver,
        dpi: Some(DpiCaps {
            min: m.min_dpi,
            max: m.max_dpi,
            step: 50,
            max_stages: MAX_STAGES,
            colors: true,
            light: true,
            brightness_steps: Some(BRIGHTNESS_STEPS.to_vec()),
            speeds: Some(SPEEDS.to_vec()),
        }),
        polling: Some(PollingCaps {
            rates: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= m.max_polling_hz).collect(),
        }),
        lod: Some(vec![Lod::Mid, Lod::High]),
        debounce_ms: Some(Range { min: 0, max: 20 }),
        debounce_choices_ms: Some(DEBOUNCE_MS.to_vec()),
        motion_sync: true,
        angle_snapping: true,
        ripple_control: true,
        profiles: Some(PROFILES),
        battery: true,
        // ATK's app hears of a DPI-button press from the mouse (0x0A), as hrtz does; the driver
        // also reads the stage every second, in case this one doesn't say.
        dpi_button: true,
        ..Capabilities::default()
    }
}

/// Where what's offered is kept, as `(address, length)`. Each is written whole, as ATK's app
/// writes it, so no check byte is left behind for a moment: the polling rate, the stage count and
/// the stage in use (one row of three pairs); the lift-off distance; each stage's DPI and its
/// colour; the DPI light (effect, brightness, speed, on); and the row from debounce to ripple
/// control, which holds ATK's sleep time too (0xAD), written back as the mouse holds it.
pub fn fields() -> impl Iterator<Item = (usize, usize)> {
    let stages = MAX_STAGES as usize;
    [(0x00, 6), (0x0A, 2)]
        .into_iter()
        .chain((0..stages).map(|i| (0x0C + 4 * i, 4)))
        .chain((0..stages).map(|i| (0x2C + 4 * i, 4)))
        .chain([(0x4C, 8), (0xA9, 10)])
}

/// Only what an ATK offers, of `s`: the rest is left as the mouse holds it.
pub fn offered(s: &Settings) -> Settings {
    Settings {
        polling_hz: s.polling_hz,
        dpi_stage_count: s.dpi_stage_count,
        dpi_stage: s.dpi_stage,
        stages: s.stages.clone(),
        dpi_light: s.dpi_light,
        lod: s.lod,
        debounce_ms: s.debounce_ms,
        motion_sync: s.motion_sync,
        angle_snapping: s.angle_snapping,
        ripple_control: s.ripple_control,
        ..Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_u2_plus_with_a_paw3395_is_claimed() {
        assert_eq!(model(2, 78).map(|m| m.name), Some("U2 Plus (PAW3395)"));
        // The A9 Plus Nearlink, on the same receiver, and the other U2 Plus versions.
        assert!(model(2, 83).is_none() && model(2, 124).is_none() && model(2, 25).is_none());
        // A Zaopin's IDs mean nothing here.
        assert!(model(7, 4).is_none());
    }

    #[test]
    fn a_receiver_is_told_from_the_mouse_on_its_cable() {
        assert!(is_receiver(0x10C9, "NK mouse NANO dongle"));
        assert!(is_receiver(0x1234, "Nearlink Mouse Dongle"));
        assert!(!is_receiver(0x110D, "ATK U2 Plus Nearlink"));
    }

    #[test]
    fn it_offers_what_atks_app_offers_with_its_values() {
        let caps = capabilities(&MODELS[0], true);
        assert!(!caps.wired);
        assert_eq!(caps.polling.unwrap().rates, vec![125, 250, 500, 1000]);
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max, dpi.step, dpi.max_stages), (100, 30000, 50, 6));
        assert!(dpi.colors && dpi.light);
        assert_eq!(dpi.brightness_steps, Some(vec![1, 5, 10]));
        assert_eq!(dpi.speeds, Some(vec![1, 3, 5]));
        assert_eq!(caps.lod, Some(vec![Lod::Mid, Lod::High]));
        assert_eq!(caps.debounce_choices_ms, Some(vec![0, 1, 2, 4, 8, 15, 20]));
        assert!(caps.motion_sync && caps.angle_snapping && caps.ripple_control);
        // Kept differently, or not known yet.
        assert!(caps.sensor_mode.is_none() && caps.sleep_times_s.is_none() && caps.peak_timeouts_s.is_none());
        assert_eq!(caps.profiles, Some(4));
        assert!(caps.buttons.is_none());
        assert!(!caps.long_range && !caps.pairing && !caps.factory_reset && !caps.backups);
        assert!(capabilities(&MODELS[0], false).wired);
    }

    #[test]
    fn every_field_is_whole_and_apart() {
        let mut f: Vec<(usize, usize)> = fields().collect();
        f.sort();
        assert!(f.windows(2).all(|w| w[0].0 + w[0].1 <= w[1].0), "overlapping: {f:?}");
        assert!(f.iter().all(|(_, n)| *n <= super::super::protocol::MAX_CHUNK), "one packet each");
    }
}
