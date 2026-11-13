//! Zaopin's models: names, and what each can do on each connection. ATK's are in `atk`.

use super::atk_dpi::Method;
use super::image::{MAX_STAGES, PEAK_TIMEOUTS_S, POLLING, SLEEP_TIMES_S};
use crate::mice::model::{
    Capabilities, DpiCaps, Lod, PollingCaps, Range, SensorMode, SensorModeCaps, Settings, StepsAbove, Supported,
};

/// Zaopin's maker ID, from the handshake: every Zaopin model has it.
pub const CID: u8 = 7;

/// Whether the handshake's IDs are Zaopin's. Other makers' mice (VGN, Lamzu's Atlantis, some Pulsar
/// and ATK models) use the same vendor ID, interface and messages, each with its own memory layout,
/// so as in the vendor app only CID 7 is taken. CID 0 with MID 0 is a receiver with no mouse paired,
/// which the vendor app lists too, so a mouse can be paired to it.
pub fn is_zaopin(cid: u8, mid: u8) -> bool {
    cid == CID || (cid == 0 && mid == 0)
}

/// A Zaopin's sensor. MIDs 1 to 6 are the vendor app's list (its Config.ini); the Z11 (MID 9) is
/// from its owner's report (Mayank_j, 2026-11-12) and Zaopin's web app (Control HUB WEB,
/// controlhub.top/zaopin: its cfg.json and sensor.json, 2026-11-13). The web app lists more
/// models (7, 8, 10 to 16) without names; hrtz doesn't know them yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sensor {
    Paw3395,
    Paw3950,
    Paw3311,
}

pub fn sensor(mid: u8) -> Option<Sensor> {
    match mid {
        1..=3 => Some(Sensor::Paw3395),
        4..=6 => Some(Sensor::Paw3950),
        9 => Some(Sensor::Paw3311),
        _ => None,
    }
}

/// Whether hrtz knows this Zaopin model. Another MID is a Zaopin hrtz doesn't know yet: it runs
/// without what depends on its sensor, shows as not supported yet, and its owner can request
/// support from Inspect a mouse.
pub fn known(mid: u8) -> bool {
    sensor(mid).is_some()
}

/// How a model keeps its stages' DPI (and a DPI-lock button's): the vendor app's 50 per step on a
/// PAW3395 or PAW3950, its PAW3311 table on the Z11. A model hrtz doesn't know is read as the
/// first, but its DPI isn't shown or written.
pub fn dpi_method(mid: u8) -> Method {
    match sensor(mid) {
        Some(Sensor::Paw3311) => Method::Table,
        _ => Method::Step50,
    }
}

/// The mice on this channel hrtz runs: Zaopin's models, by MID, then ATK's, each with whether it has
/// been tried on the real mouse (the Z1 PRO MAX only, so far).
pub fn supported() -> Vec<Supported> {
    let mut all: Vec<Supported> = [(4, true), (1, false), (2, false), (5, false), (3, false), (6, false), (9, false)]
        .into_iter()
        .map(|(mid, tested)| Supported {
            family: "compx",
            brand: "Zaopin",
            model: model_name(mid),
            key: format!("compx/{CID}-{mid}"),
            tested,
        })
        .collect();
    all.extend(super::atk::supported());
    all
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
    // PAW3950 models (the MAX line) go to 30K and down to 0.7 mm lift-off; PAW3395 models to 26K;
    // the Z11's PAW3311 to 12,000, as Zaopin's web app offers it: its table in steps of 50 to
    // 10,000, then half of it doubled, in steps of 100. Zaopin's sensor lists (the driver's
    // driver_sensor.h, the web app's sensor.json) offer motion sync on the PAW3395 and PAW3950
    // only, lift-off on those but not the PAW3311, and the sensor mode, ripple control and angle
    // snapping on all three. A model hrtz doesn't know may have any sensor, so none of these, and
    // no DPI: its stages may be stored another way.
    let sensor = sensor(mid);
    let (wired, max_polling_hz) = link(device_type);
    let lod = match sensor {
        Some(Sensor::Paw3311) => None,
        Some(Sensor::Paw3950) => Some(vec![Lod::Low, Lod::Mid, Lod::High]),
        _ => Some(vec![Lod::Mid, Lod::High]),
    };
    let max_dpi = sensor.map(|s| match s {
        Sensor::Paw3395 => 26000,
        Sensor::Paw3950 => 30000,
        Sensor::Paw3311 => 12000,
    });
    // Corded is the PAW3395's and PAW3950's: a PAW3311 has LP and HP.
    let paw3311 = sensor == Some(Sensor::Paw3311);
    Capabilities {
        wired,
        dpi: max_dpi.map(|max| DpiCaps {
            min: 50,
            max,
            step: 50,
            max_stages: MAX_STAGES as u8,
            colors: true,
            // "DPI Light Effects" in the vendor app, for every model.
            light: true,
            brightness_steps: None,
            speeds: None,
            above: (sensor == Some(Sensor::Paw3311)).then_some(StepsAbove { from: 10000, step: 100 }),
        }),
        polling: Some(PollingCaps { rates: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= max_polling_hz).collect() }),
        lod,
        sensor_mode: sensor.map(|_| SensorModeCaps {
            modes: if paw3311 {
                vec![SensorMode::Lp, SensorMode::Hp]
            } else {
                vec![SensorMode::Lp, SensorMode::Hp, SensorMode::Corded]
            },
            corded_from_hz: (!paw3311).then_some(2000),
            corded_when_wired: !paw3311,
        }),
        debounce_ms: Some(Range { min: 0, max: 20 }),
        debounce_choices_ms: None,
        motion_sync: matches!(sensor, Some(Sensor::Paw3395 | Sensor::Paw3950)),
        angle_snapping: sensor.is_some(),
        ripple_control: sensor.is_some(),
        peak_timeouts_s: Some(PEAK_TIMEOUTS_S.to_vec()),
        sleep_times_s: (mid == 4).then(|| SLEEP_TIMES_S.to_vec()),
        buttons: Some(
            ["Left", "Right", "Wheel click", "Back", "Forward", "DPI"]
                .iter()
                .enumerate()
                .map(|(i, n)| (i as u8, n.to_string()))
                .collect(),
        ),
        profiles: Some(4),
        battery: true,
        long_range: true,
        pairing: true,
        factory_reset: true,
        backups: true,
        wake_by_input: false,
        // On the underside of every model.
        dpi_button: true,
        rated_hours: None,
    }
}

/// Only what hrtz offers on this model, of `s` (`caps` are its capabilities): what it leaves out
/// stays as the mouse holds it, and isn't shown.
pub fn offered(s: Settings, caps: &Capabilities) -> Settings {
    let dpi = caps.dpi.is_some();
    Settings {
        dpi_stage_count: s.dpi_stage_count.filter(|_| dpi),
        dpi_stage: s.dpi_stage.filter(|_| dpi),
        stages: s.stages.filter(|_| dpi),
        dpi_light: s.dpi_light.filter(|_| dpi),
        lod: s.lod.filter(|_| caps.lod.is_some()),
        sensor_mode: s.sensor_mode.filter(|_| caps.sensor_mode.is_some()),
        motion_sync: s.motion_sync.filter(|_| caps.motion_sync),
        angle_snapping: s.angle_snapping.filter(|_| caps.angle_snapping),
        ripple_control: s.ripple_control.filter(|_| caps.ripple_control),
        ..s
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
        9 => "Z11",
        _ => "Zaopin mouse",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_hrtz_doesnt_know_runs_without_what_its_sensor_decides() {
        assert!(known(1) && known(6) && known(9) && !known(7) && !known(0));
        let caps = capabilities(10, 0);
        assert!(caps.dpi.is_none() && caps.sensor_mode.is_none());
        assert!(!caps.motion_sync && !caps.angle_snapping && !caps.ripple_control);
        assert_eq!(caps.lod, Some(vec![Lod::Mid, Lod::High]));
        assert!(caps.polling.is_some() && caps.debounce_ms.is_some() && caps.buttons.is_some());
        assert_eq!(model_name(10), "Zaopin mouse");
        // The ones it knows keep theirs.
        assert_eq!(capabilities(1, 0).dpi.unwrap().max, 26000);
        assert_eq!(capabilities(4, 5).dpi.unwrap().max, 30000);
        let z1 = capabilities(4, 5);
        assert!(z1.sensor_mode.is_some() && z1.motion_sync && z1.angle_snapping && z1.ripple_control);
    }

    #[test]
    fn the_z11_has_a_paw3311() {
        let caps = capabilities(9, 2);
        assert_eq!(model_name(9), "Z11");
        assert_eq!(dpi_method(9), Method::Table);
        assert_eq!(dpi_method(4), Method::Step50);
        let dpi = caps.dpi.unwrap();
        assert_eq!((dpi.min, dpi.max, dpi.step), (50, 12000, 50));
        assert!(dpi.holds(10000) && dpi.holds(10100) && dpi.holds(12000));
        assert!(!dpi.holds(10050) && !dpi.holds(12100) && !dpi.holds(25));
        // No motion sync or lift-off on a PAW3311, as in Zaopin's sensor lists; no Corded mode.
        assert!(!caps.motion_sync && caps.angle_snapping && caps.ripple_control);
        let mode = caps.sensor_mode.unwrap();
        assert_eq!(mode.modes, vec![SensorMode::Lp, SensorMode::Hp]);
        assert!(!mode.corded_when_wired && mode.corded_from_hz.is_none());
        assert_eq!(caps.lod, None);
        assert!(supported().iter().any(|m| m.key == "compx/7-9" && m.model == "Z11" && !m.tested));
    }

    #[test]
    fn what_isnt_offered_isnt_shown() {
        let s = Settings {
            dpi_stage: Some(0),
            motion_sync: Some(true),
            angle_snapping: Some(false),
            polling_hz: Some(1000),
            ..Settings::default()
        };
        let z11 = offered(s.clone(), &capabilities(9, 2));
        assert_eq!((z11.dpi_stage, z11.motion_sync, z11.angle_snapping), (Some(0), None, Some(false)));
        let other = offered(s, &capabilities(10, 2));
        assert_eq!((other.dpi_stage, other.angle_snapping, other.polling_hz), (None, None, Some(1000)));
    }
}
