//! Zaopin's models: names, and what each can do on each connection.

use super::image::{MAX_STAGES, PEAK_TIMEOUTS_S, POLLING, SLEEP_TIMES_S};
use crate::mice::model::{Capabilities, DpiCaps, Lod, PollingCaps, Range, SensorMode, SensorModeCaps, Supported};

/// Zaopin's models, by MID, and whether each has been tried on the real mouse (the Z1 PRO MAX
/// only, so far). The CID of every one is 7.
pub fn supported() -> Vec<Supported> {
    [(4, true), (1, false), (2, false), (5, false), (3, false), (6, false)]
        .into_iter()
        .map(|(mid, tested)| Supported {
            family: "compx",
            brand: "Zaopin",
            model: model_name(mid),
            key: format!("compx/7-{mid}"),
            tested,
        })
        .collect()
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
    // PAW3950 models (the MAX line) go to 30K and down to 0.7 mm lift-off; PAW3395 models to 26K.
    let paw3950 = (4..=6).contains(&mid);
    let (wired, max_polling_hz) = link(device_type);
    let mut lod = vec![Lod::Mid, Lod::High];
    if paw3950 {
        lod.insert(0, Lod::Low);
    }
    Capabilities {
        wired,
        dpi: Some(DpiCaps {
            min: 50,
            max: if paw3950 { 30000 } else { 26000 },
            step: 50,
            max_stages: MAX_STAGES as u8,
            colors: true,
        }),
        polling: Some(PollingCaps { rates: POLLING.iter().map(|p| p.0).filter(|hz| *hz <= max_polling_hz).collect() }),
        lod: Some(lod),
        sensor_mode: Some(SensorModeCaps {
            modes: vec![SensorMode::Lp, SensorMode::Hp, SensorMode::Corded],
            corded_from_hz: Some(2000),
            corded_when_wired: true,
        }),
        debounce_ms: Some(Range { min: 0, max: 20 }),
        motion_sync: true,
        angle_snapping: true,
        ripple_control: true,
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
