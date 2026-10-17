//! Zaopin's models: names, and what each can do on each connection.

use super::image::{PEAK_TIMEOUTS_S, POLLING, SLEEP_TIMES_S};
use crate::mice::model::Capabilities;

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
