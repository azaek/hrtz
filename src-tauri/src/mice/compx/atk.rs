// Facts from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/atk/hid.ts, src/drivers/atk/products.ts,
// src/drivers/atk/device-catalog.generated.ts and src/atk/index.ts. Translated to Rust and modified
// for hrtz, 2026-11-10. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! ATK's mice on CompX's vendor channel (ATK, VXE and MAD, in ATK's own app). They use a Zaopin's
//! packets, commands and memory addresses, and say who they are with their own command (0x10).
//! What's offered is what ATK's own app (ATK HUB 3.2.32) offers and keeps as a Zaopin does, with the
//! values it offers, and the four onboard profiles; the rest (sleep, the sensor mode, the buttons)
//! is kept differently or isn't known yet, so it's left as the mouse holds it. The models are in
//! `atk_catalog`, every named one in ATK's app; what's offered on each sensor is `Sensor`'s. Another
//! ATK, or one hrtz can't name, is left alone.

use super::atk_catalog::{MODELS, ON_COMPX_VID, RECEIVERS};
use super::atk_dpi as dpi;
pub use super::atk_dpi::Method;
use super::image::POLLING;
use super::protocol as p;
use crate::mice::model::{Capabilities, DpiCaps, DpiStage, Lod, PollingCaps, Range, Settings, Supported};

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

    /// Lift-off, where ATK's app offers it as a Zaopin does: 1 or 2 mm (codes 1 and 2), and 0.7 mm
    /// (3) on a PAW3950. A PAW3950 Ultra's and a PAW3955's run 0.7 to 1.7 mm in eleven steps (codes
    /// 1 to 11), which hrtz's choices don't hold yet; a PAW3311's and a PAW3315's aren't offered.
    fn lod(self) -> Option<Vec<Lod>> {
        match self {
            Sensor::Paw3395 | Sensor::Paw3395Ultra | Sensor::Paw3395Se | Sensor::Core26k => {
                Some(vec![Lod::Mid, Lod::High])
            }
            Sensor::Paw3950 | Sensor::Paw3950Dm => Some(vec![Lod::Low, Lod::Mid, Lod::High]),
            Sensor::Paw3950Ultra | Sensor::Paw3955 | Sensor::Paw3311 | Sensor::Paw3315 => None,
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

/// The sleep times ATK's app offers, in seconds, kept at 0xAD in 10 s steps (ATK's app reads its
/// sleep time there). mouse-protocol offers the same on its R1s and gives other ATKs other
/// choices, so hrtz offers these only where the two agree (2026-11-14).
pub const R1_SLEEP_S: [u32; 8] = [30, 60, 120, 180, 300, 1200, 1500, 1800];
const SLEEP: usize = 0xAD;

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

/// `seconds` as the sleep time, into `target` (in the row from 0xA9, which is written whole).
pub fn put_sleep(target: &mut [u8], seconds: u32) {
    let v = (seconds / 10) as u8;
    target[SLEEP] = v;
    target[SLEEP + 1] = p::checksum(&[v]);
}

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
        debounce_ms: Some(Range { min: debounce[0] as u32, max: 20 }),
        debounce_choices_ms: Some(debounce),
        motion_sync: true,
        angle_snapping: true,
        ripple_control: true,
        profiles: Some(PROFILES),
        sleep_times_s: R1_SLEEP_MODELS.contains(&(m.cid, m.mid)).then(|| R1_SLEEP_S.to_vec()),
        battery: true,
        // ATK's app hears of a DPI-button press from the mouse (0x0A), as hrtz does; the driver
        // also reads the stage every second, in case this one doesn't say.
        dpi_button: true,
        ..Capabilities::default()
    }
}

/// Where what's offered on `m` is kept, as `(address, length)`, in the groups ATK's app writes
/// (mouse-protocol's captures of an R1 Pro Max show the same), so no check byte is left behind
/// for a moment: the system row (polling rate, stage count, stage in use, and what ATK keeps beside
/// them, written back as held); the lift-off distance, where offered; the DPI stages two at a
/// time (a PAW3955's one at a time, from 0x1B00); on a model with a DPI light, the colours two at
/// a time and the light; and the row from debounce to ripple control, which holds ATK's sleep time
/// too (0xAD: set where it's offered, elsewhere written back as held). Nothing of the DPI where
/// it isn't offered.
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

/// Only what an ATK offers, of `s`: the rest is left as the mouse holds it. A model without a DPI
/// light shows none; one whose DPI method isn't settled shows no stages; one whose lift-off isn't
/// offered, none.
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
        // One model per CID and MID, and none tested yet.
        for (i, a) in MODELS.iter().enumerate() {
            assert!(MODELS[i + 1..].iter().all(|b| (a.cid, a.mid) != (b.cid, b.mid)), "{a:?}");
            assert!(!a.tested && ["ATK", "VXE", "MAD"].contains(&a.brand));
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
        // Under CompX's, the driver offers them on any (snowyuonA's receiver).
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
        assert_eq!(fields(r1s), vec![(0x00, 10), (0xA9, 10)], "nothing of the DPI or lift-off is written");
        // A PAW3950 Ultra's lift-off scale isn't one of hrtz's choices yet; its DPI is.
        let u2u = capabilities(model(2, 124).unwrap(), link::NEARLINK);
        assert!(u2u.lod.is_none() && u2u.dpi.is_some_and(|d| (d.min, d.max) == (50, 30000)));
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
