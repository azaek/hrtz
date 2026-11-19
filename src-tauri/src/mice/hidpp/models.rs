// Product IDs from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/logitech/index.ts and
// src/drivers/vendors.ts. Translated to Rust and modified for hrtz, 2026-10-18. Copyright (C) the
// OpenMouse contributors. AGPL-3.0-or-later.

//! Logitech's mice hrtz runs: Superlights and the wireless-only G304/G305.
//! Other mice that speak HID++ come one at a time, with an owner to try them, so only these are
//! claimed: a keyboard or headset on the same protocol is left alone.

use crate::mice::model::Supported;

/// LIGHTSPEED receivers: 0xC539 (HERO-era, shipped with the first Superlight and the G Pro
/// Wireless), 0xC547 (the first Superlight's later one), 0xC54D (the Superlight 2's) and 0xC53F
/// (the G304/G305's: G HUB's `receiverPid` 50495 for it). The mouse answers on one of the
/// receiver's pairing slots.
pub const RECEIVERS: [u16; 4] = [0xC539, 0xC547, 0xC54D, 0xC53F];

/// The G304 and G305: one mouse with no cable, so 0x4074 is its wireless ID, not a USB product ID.
/// G HUB lists it as the G305, with each colour's extended model ID called a G304 or a G305
/// (`devices_0002.json`). A read-only probe of a G304 on its C53F receiver (PR #26, 2026-11-18):
/// HID++ on vendor page 0xFF00 (short reports usage 1, long 2); DPI 200 to 12,000 in steps of 50
/// (0x2201); 125, 250, 500 and 1,000 Hz (0x8060); battery by 0x1000, an AA cell; onboard memory
/// format 3, one profile, six buttons, 255-byte sectors. G HUB saved Ctrl+C directly in both
/// button tables in sector 1. For X then Y, the records pointed to copies of the macro in sector
/// 2 at offsets 0x00 and 0x16. Sector 2 had a valid profile-style CRC after this write. Its earlier
/// macro data had FF FF in the CRC field. Switching back to Ctrl+C restored sector 1 byte for byte
/// and left the new macro bytes in sector 2. Sectors 3 to 5 refused reads in every capture.
/// On RQM 68.02, hrtz verified writing both sectors, applying the ROM profile, and restoring the
/// original readable memory byte for byte. Factory reset preserves the macro sector.
pub const G304: u16 = 0x4074;
const WIRELESS: [(u16, &str); 1] = [(G304, "G304 / G305")];

/// The mice on their own cables, which answer as themselves, with their names.
pub const CABLED: [(u16, &str); 3] =
    [(0xC094, "PRO X SUPERLIGHT"), (0xC09B, "PRO X SUPERLIGHT 2"), (0xC0A0, "PRO X SUPERLIGHT 2 DEX")];

pub fn claims(product_id: u16) -> bool {
    is_receiver(product_id) || CABLED.iter().any(|(pid, _)| *pid == product_id)
}

pub fn is_receiver(product_id: u16) -> bool {
    RECEIVERS.contains(&product_id)
}

/// A mouse's name, by its USB ID (which it reports through either link).
pub fn name(usb_id: u16) -> Option<&'static str> {
    CABLED.iter().chain(WIRELESS.iter()).find(|(pid, _)| *pid == usb_id).map(|(_, n)| *n)
}

/// Logitech's power model, from G HUB (`battery.xml`, the notes' GHUB.md): the battery in mWh,
/// what the sensor and the chip draw, and what reporting draws at each rate, in mW.
struct Power {
    usb: u16,
    battery_mwh: f64,
    always_mw: f64,
    rate_mw: [(u32, f64); 4],
}

const POWER: [Power; 1] = [Power {
    usb: 0xC094,
    battery_mwh: 874.0,
    // HERO sensor, STM32 chip.
    always_mw: 1.545_556 + 2.306_444,
    rate_mw: [(125, 2.203_556), (250, 3.073_778), (500, 4.814_222), (1000, 8.295_111)],
}];

/// Hours of use a charge lasts at each rate, by Logitech's model: about 144 h at 125 Hz down to 72
/// at 1,000 for the first Superlight. None for a model it doesn't give.
pub fn rated_hours(usb: u16) -> Option<Vec<(u32, f64)>> {
    let p = POWER.iter().find(|p| p.usb == usb)?;
    Some(p.rate_mw.iter().map(|(hz, mw)| (*hz, p.battery_mwh / (p.always_mw + mw))).collect())
}

/// The key a mouse's model shares, by its USB ID: `hidpp/c094`. A connected one adds its unit ID.
pub fn model_key(usb_id: u16) -> String {
    format!("hidpp/{usb_id:04x}")
}

/// Tried on a real mouse: the first Superlight, on its receiver and its cable (2026-10-18), and a
/// G304 on its receiver, whose check passed (PR #26, 2026-11-18).
const TESTED: [u16; 2] = [0xC094, G304];

pub fn supported() -> Vec<Supported> {
    CABLED
        .iter()
        .chain(WIRELESS.iter())
        .map(|(pid, name)| Supported {
            family: "hidpp",
            brand: "Logitech",
            model: name,
            key: model_key(*pid),
            tested: TESTED.contains(pid),
        })
        .collect()
}
