// Product IDs from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/logitech/index.ts and
// src/drivers/vendors.ts. Translated to Rust and modified for hrtz, 2026-10-18. Copyright (C) the
// OpenMouse contributors. AGPL-3.0-or-later.

//! Logitech's mice hrtz runs: the Superlights, on their LIGHTSPEED receivers or their cables.
//! Other mice that speak HID++ come one at a time, with an owner to try them, so only these are
//! claimed: a keyboard or headset on the same protocol is left alone.

use crate::mice::model::Supported;

/// LIGHTSPEED receivers: 0xC539 (HERO-era, shipped with the first Superlight and the G Pro
/// Wireless), 0xC547 (the first Superlight's later one) and 0xC54D (the Superlight 2's). The mouse
/// answers on one of the receiver's pairing slots.
pub const RECEIVERS: [u16; 3] = [0xC539, 0xC547, 0xC54D];

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
    CABLED.iter().find(|(pid, _)| *pid == usb_id).map(|(_, n)| *n)
}

/// The key a mouse's model shares, by its USB ID: `hidpp/c094`. A connected one adds its unit ID.
pub fn model_key(usb_id: u16) -> String {
    format!("hidpp/{usb_id:04x}")
}

/// Tried on a real mouse: the first Superlight, on its receiver and its cable (2026-10-18).
const TESTED: [u16; 1] = [0xC094];

pub fn supported() -> Vec<Supported> {
    CABLED
        .iter()
        .map(|(pid, name)| Supported {
            family: "hidpp",
            brand: "Logitech",
            model: name,
            key: model_key(*pid),
            tested: TESTED.contains(pid),
        })
        .collect()
}
