// Facts from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/atk/hid.ts, src/drivers/atk/products.ts,
// src/drivers/atk/device-catalog.generated.ts and src/atk/index.ts. Translated to Rust and modified
// for hrtz, 2026-11-10. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! ATK's mice on CompX's vendor channel (ATK, VXE and MAD, in ATK's own app). They use a Zaopin's
//! packets, commands and memory addresses, and say who they are with their own command (0x10).
//! What's offered is what ATK's own app (ATK HUB 3.2.32) offers and keeps as a Zaopin does, with the
//! values it offers, and the four onboard profiles; the rest (sleep, the sensor mode, the buttons)
//! is kept differently or isn't known yet, so it's left as the mouse holds it. The models are in
//! `atk_catalog`, from ATK's app: those whose sensor stores DPI as a Zaopin's does. Another ATK, or
//! one hrtz can't name, is left alone.

use super::atk_catalog::{MODELS, ON_COMPX_VID, RECEIVERS};
use super::image::POLLING;
use super::protocol as p;
use crate::mice::model::{Capabilities, DpiCaps, Lod, PollingCaps, Range, Settings, Supported};

/// ATK's USB vendor ID, on its receivers and on its mice's cables.
pub const VENDOR_ID: u16 = 0x373B;

/// GetMouseCIDMID: the answer's first two data bytes are the maker (CID) and the model (MID). ATK's
/// own app asks this to name the mouse.
pub const CID_MID: u8 = 0x10;

/// ATK's generic NearLink receiver, which the U2 Plus comes with: "NK mouse NANO dongle", named
/// "Nearlink Mouse Dongle" before a receiver update.
pub const NEARLINK_RECEIVER: u16 = 0x10C9;

/// The sensors hrtz runs an ATK with: each stores DPI as a Zaopin's does (50 per step, the top
/// bits beside it), which hrtz's encoder writes up to 30,000.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sensor {
    Paw3395,
    Paw3395Ultra,
    Paw3950,
    Paw3950Dm,
}

impl Sensor {
    /// The DPI range ATK's app offers, in steps of 50: a PAW3395 from 100, a PAW3950 from 50. A
    /// PAW3950 goes on to 36,000 there, past 30,000 with another flag, so hrtz stops at 30,000.
    fn dpi(self) -> (u32, u32) {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra => (100, 30000),
            Sensor::Paw3950 | Sensor::Paw3950Dm => (50, 30000),
        }
    }

    /// Lift-off: 1 or 2 mm, and 0.7 mm on a PAW3950 (codes 1, 2 and 3, a Zaopin's).
    fn lod(self) -> Vec<Lod> {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra => vec![Lod::Mid, Lod::High],
            Sensor::Paw3950 | Sensor::Paw3950Dm => vec![Lod::Low, Lod::Mid, Lod::High],
        }
    }
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

/// What hrtz offers on an ATK, before it's tested: what ATK's app offers and keeps where a Zaopin
/// does. The polling rate (to what the connection allows); the DPI stages (packed as a Zaopin's),
/// and on a model with a DPI light, their colours and the light; the lift-off distance its sensor
/// offers; debounce, motion sync, straight-line correction (a Zaopin's angle snapping) and ripple
/// control; the onboard profiles. The battery is the mouse's own reading.
pub fn capabilities(m: &Model, kind: u8) -> Capabilities {
    let (min, max) = m.sensor.dpi();
    let debounce: Vec<u8> = DEBOUNCE_MS.iter().copied().filter(|ms| m.zero_debounce || *ms > 0).collect();
    Capabilities {
        wired: link::wired(kind),
        dpi: Some(DpiCaps {
            min,
            max,
            step: 50,
            max_stages: MAX_STAGES,
            colors: m.light,
            light: m.light,
            brightness_steps: m.light.then(|| BRIGHTNESS_STEPS.to_vec()),
            speeds: m.light.then(|| SPEEDS.to_vec()),
        }),
        polling: Some(PollingCaps {
            rates: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= link::max_polling_hz(kind)).collect(),
        }),
        lod: Some(m.sensor.lod()),
        debounce_ms: Some(Range { min: debounce[0] as u32, max: 20 }),
        debounce_choices_ms: Some(debounce),
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

/// Where what's offered on `m` is kept, as `(address, length)`. Each is written whole, as ATK's
/// app writes it, so no check byte is left behind for a moment: the polling rate, the stage count
/// and the stage in use (one row of three pairs); the lift-off distance; each stage's DPI; on a
/// model with a DPI light, each stage's colour and the light (effect, brightness, speed, on); and
/// the row from debounce to ripple control, which holds ATK's sleep time too (0xAD), written back
/// as the mouse holds it.
pub fn fields(m: &Model) -> impl Iterator<Item = (usize, usize)> {
    let stages = MAX_STAGES as usize;
    let light = if m.light { stages } else { 0 };
    [(0x00, 6), (0x0A, 2)]
        .into_iter()
        .chain((0..stages).map(|i| (0x0C + 4 * i, 4)))
        .chain((0..light).map(|i| (0x2C + 4 * i, 4)))
        .chain(m.light.then_some((0x4C, 8)))
        .chain([(0xA9, 10)])
}

/// Only what an ATK offers, of `s`: the rest is left as the mouse holds it. A model without a DPI
/// light shows none.
pub fn offered(s: &Settings, m: &Model) -> Settings {
    Settings {
        polling_hz: s.polling_hz,
        dpi_stage_count: s.dpi_stage_count,
        dpi_stage: s.dpi_stage,
        stages: s.stages.clone(),
        dpi_light: if m.light { s.dpi_light } else { None },
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
    fn the_models_are_atks_with_a_zaopins_dpi() {
        assert_eq!(MODELS.len(), 51);
        assert_eq!(model(2, 78).map(|m| m.name), Some("U2 Plus NK (PAW3395)"));
        assert_eq!(model(2, 7).map(|m| (m.brand, m.name)), Some(("VXE", "R1 PRO (PAW3395)")));
        // The U2 Plus NK with a PAW3950 Ultra stores DPI otherwise: not yet.
        assert!(model(2, 124).is_none() && model(2, 25).is_none());
        // A Zaopin's IDs mean nothing here.
        assert!(model(7, 4).is_none());
        // One model per CID and MID, and none tested yet.
        for (i, a) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|b| (a.cid, a.mid) != (b.cid, b.mid)), "{a:?}");
            assert!(!a.tested && ["ATK", "VXE", "MAD"].contains(&a.brand));
        }
    }

    #[test]
    fn a_receiver_is_told_from_the_mouse_on_its_cable() {
        assert!(is_receiver(0x373B, 0x10C9, "NK mouse NANO dongle"));
        assert!(is_receiver(0x373B, 0x101B, "anything"), "listed by ATK's app");
        assert!(is_receiver(0x373B, 0x1234, "Nearlink Mouse Dongle"));
        assert!(!is_receiver(0x373B, 0x110D, "ATK U2 Plus Nearlink"));
    }

    #[test]
    fn under_compx_vid_only_atks_listed_models_on_its_ids() {
        assert!(on_compx_vid(0xF58C, 2, 7), "VXE R1 PRO");
        assert!(!on_compx_vid(0xF58C, 2, 124), "a model hrtz doesn't run");
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
        assert_eq!(caps.lod, Some(vec![Lod::Mid, Lod::High]));
        assert_eq!(caps.debounce_choices_ms, Some(vec![0, 1, 2, 4, 8, 15, 20]));
        assert!(caps.motion_sync && caps.angle_snapping && caps.ripple_control);
        assert_eq!(caps.profiles, Some(4));
        // Kept differently, or not known yet.
        assert!(caps.sensor_mode.is_none() && caps.sleep_times_s.is_none() && caps.peak_timeouts_s.is_none());
        assert!(caps.buttons.is_none());
        assert!(!caps.long_range && !caps.pairing && !caps.factory_reset && !caps.backups);
        // On a cable at 1K, and a 1K receiver.
        let wired = capabilities(u2, link::WIRED_1K);
        assert!(wired.wired && wired.polling.unwrap().rates.last() == Some(&1000));
        assert_eq!(capabilities(u2, link::DONGLE_1K).polling.unwrap().rates.last(), Some(&1000));
    }

    #[test]
    fn each_model_gets_its_sensors_range_and_its_own_light_and_debounce() {
        // A PAW3950: from 50 DPI, and 0.7 mm lift-off.
        let caps = capabilities(model(2, 36).unwrap(), link::DONGLE_1K);
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max), (50, 30000));
        assert_eq!(caps.lod, Some(vec![Lod::Low, Lod::Mid, Lod::High]));
        // The A9 Plus NK has no DPI light: its colours and light aren't offered or written.
        let a9 = model(2, 83).unwrap();
        let dpi = capabilities(a9, link::DONGLE_1K).dpi.unwrap();
        assert!(!dpi.light && !dpi.colors && dpi.brightness_steps.is_none());
        assert!(fields(a9).all(|(a, _)| !(0x2C..0x54).contains(&a)));
        // The A9 Ultra Max 2.0 has none either, and its debounce starts at 1 ms.
        let a9u = capabilities(model(1, 6).unwrap(), link::DONGLE_1K);
        assert_eq!(a9u.debounce_choices_ms, Some(vec![1, 2, 4, 8, 15, 20]));
        assert_eq!(a9u.debounce_ms.unwrap().min, 1);
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
            let mut f: Vec<(usize, usize)> = fields(m).collect();
            f.sort();
            assert!(f.windows(2).all(|w| w[0].0 + w[0].1 <= w[1].0), "overlapping: {f:?}");
            assert!(f.iter().all(|(_, n)| *n <= super::super::protocol::MAX_CHUNK), "one packet each");
        }
    }
}
