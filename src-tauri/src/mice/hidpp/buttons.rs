// Ported from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/logitech/onboard-profiles.ts (the
// button records, ACTION_RECORDS and encodeButtonAssignment, and where each profile format keeps
// them). Translated to Rust and modified for hrtz, 2026-10-18. Copyright (C) the OpenMouse
// contributors. AGPL-3.0-or-later.

//! What a button does, as a Logitech profile keeps it: four bytes a button, a table of them from
//! 0x20 (formats 1 to 5, sixteen) or 0x30 (6 to 10, twelve), the G-Shift layer's after it. A record
//! hrtz doesn't edit (a macro, G-Shift, DPI Shift) is kept as `Vendor` and written back as it was.

use super::onboard as ob;
use crate::mice::model::{ButtonAction, DpiAction, MouseButton, ProfileAction, ScrollDirection};

/// Where a profile format keeps its buttons, and how many it has room for.
pub fn table(format: u8) -> (usize, usize) {
    if ob::v6(format) { (0x30, 12) } else { (0x20, 16) }
}

/// The physical buttons, by index, as getInfo counts them: the Superlights' five.
pub fn names(count: u8) -> Vec<(u8, String)> {
    const NAMES: [&str; 5] = ["Left", "Right", "Wheel click", "Back", "Forward"];
    (0..count)
        .map(|i| (i, NAMES.get(i as usize).map_or_else(|| format!("Button {}", i + 1), |n| (*n).to_string())))
        .collect()
}

/// HID mouse-button bits, as both Zaopin and Logitech number them.
const MOUSE: [(MouseButton, u8); 5] = [
    (MouseButton::Left, 0x01),
    (MouseButton::Right, 0x02),
    (MouseButton::Middle, 0x04),
    (MouseButton::Back, 0x08),
    (MouseButton::Forward, 0x10),
];

/// The special functions (`0x90`, then a number).
const TILT_LEFT: u8 = 0x01;
const TILT_RIGHT: u8 = 0x02;
const DPI_UP: u8 = 0x03;
const DPI_DOWN: u8 = 0x04;
const DPI_CYCLE: u8 = 0x05;
const DPI_DEFAULT: u8 = 0x06;
const PROFILE_NEXT: u8 = 0x08;
const PROFILE_PREVIOUS: u8 = 0x09;
const PROFILE_CYCLE: u8 = 0x0A;

pub fn decode(r: &[u8]) -> ButtonAction {
    let r: [u8; 4] = r.get(..4).and_then(|b| b.try_into().ok()).unwrap_or([0xFF; 4]);
    let vendor = ButtonAction::Vendor { bytes: r.to_vec() };
    match r {
        [0xFF, 0xFF, 0xFF, 0xFF] => ButtonAction::Disabled,
        [0x80, 0x01, 0x00, bit] => {
            MOUSE.iter().find(|(_, b)| *b == bit).map_or(vendor, |(button, _)| ButtonAction::Mouse { button: *button })
        }
        [0x80, 0x02, modifiers, key] => {
            ButtonAction::Keys { modifiers, keys: if key == 0 { vec![] } else { vec![key] } }
        }
        [0x80, 0x03, hi, lo] => ButtonAction::Media { usage: u16::from_be_bytes([hi, lo]) },
        [0x90, f, 0x00, 0x00] => match f {
            TILT_LEFT => ButtonAction::Scroll { direction: ScrollDirection::Left },
            TILT_RIGHT => ButtonAction::Scroll { direction: ScrollDirection::Right },
            DPI_UP => ButtonAction::Dpi { action: DpiAction::Up },
            DPI_DOWN => ButtonAction::Dpi { action: DpiAction::Down },
            DPI_CYCLE => ButtonAction::Dpi { action: DpiAction::Cycle },
            DPI_DEFAULT => ButtonAction::Dpi { action: DpiAction::Default },
            PROFILE_NEXT => ButtonAction::Profile { action: ProfileAction::Next },
            PROFILE_PREVIOUS => ButtonAction::Profile { action: ProfileAction::Previous },
            PROFILE_CYCLE => ButtonAction::Profile { action: ProfileAction::Cycle },
            _ => vendor,
        },
        _ => vendor,
    }
}

/// The record for `a`, or why this mouse's buttons can't do it.
pub fn encode(a: &ButtonAction) -> Result<[u8; 4], String> {
    let special = |f: u8| Ok([0x90, f, 0x00, 0x00]);
    match a {
        ButtonAction::Disabled => Ok([0xFF; 4]),
        ButtonAction::Mouse { button } => {
            Ok([0x80, 0x01, 0x00, MOUSE.iter().find(|(b, _)| b == button).map_or(0x01, |(_, bit)| *bit)])
        }
        ButtonAction::Keys { modifiers, keys } => match keys[..] {
            [] if *modifiers != 0 => Ok([0x80, 0x02, *modifiers, 0x00]),
            [key] => Ok([0x80, 0x02, *modifiers, key]),
            [] => Err("Record a key for it first.".into()),
            _ => Err("This mouse's buttons take one key, with modifiers.".into()),
        },
        ButtonAction::Media { usage } => {
            let [hi, lo] = usage.to_be_bytes();
            Ok([0x80, 0x03, hi, lo])
        }
        ButtonAction::Scroll { direction: ScrollDirection::Left } => special(TILT_LEFT),
        ButtonAction::Scroll { direction: ScrollDirection::Right } => special(TILT_RIGHT),
        ButtonAction::Dpi { action } => special(match action {
            DpiAction::Up => DPI_UP,
            DpiAction::Down => DPI_DOWN,
            DpiAction::Cycle => DPI_CYCLE,
            DpiAction::Default => DPI_DEFAULT,
        }),
        ButtonAction::Profile { action } => special(match action {
            ProfileAction::Next => PROFILE_NEXT,
            ProfileAction::Previous => PROFILE_PREVIOUS,
            ProfileAction::Cycle => PROFILE_CYCLE,
        }),
        ButtonAction::Vendor { bytes } => {
            bytes.get(..4).and_then(|b| b.try_into().ok()).ok_or_else(|| "A button record is four bytes.".into())
        }
        ButtonAction::Scroll { .. } => Err("This mouse's buttons scroll left and right only.".into()),
        ButtonAction::Fire { .. }
        | ButtonAction::PollingSwitch
        | ButtonAction::DpiLock { .. }
        | ButtonAction::Macro { .. }
        | ButtonAction::Raw { .. } => Err("This mouse's buttons can't do that.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_record_hrtz_edits_reads_back_as_written() {
        let all = [
            ButtonAction::Disabled,
            ButtonAction::Mouse { button: MouseButton::Back },
            ButtonAction::Keys { modifiers: 0x01, keys: vec![0x06] },
            ButtonAction::Keys { modifiers: 0x02, keys: vec![] },
            ButtonAction::Media { usage: 0x00CD },
            ButtonAction::Scroll { direction: ScrollDirection::Right },
            ButtonAction::Dpi { action: DpiAction::Default },
            ButtonAction::Profile { action: ProfileAction::Cycle },
            ButtonAction::Vendor { bytes: vec![0x90, 0x0B, 0x00, 0x00] },
        ];
        for a in all {
            assert_eq!(decode(&encode(&a).unwrap()), a);
        }
        // OpenMouse's records.
        assert_eq!(encode(&ButtonAction::Mouse { button: MouseButton::Forward }).unwrap(), [0x80, 0x01, 0x00, 0x10]);
        assert_eq!(encode(&ButtonAction::Dpi { action: DpiAction::Cycle }).unwrap(), [0x90, 0x05, 0x00, 0x00]);
        assert_eq!(
            decode(&[0x00, 0x03, 0x07, 0x00]),
            ButtonAction::Vendor { bytes: vec![0x00, 0x03, 0x07, 0x00] },
            "a macro"
        );
    }

    #[test]
    fn what_the_buttons_cant_do_is_said() {
        assert!(encode(&ButtonAction::Scroll { direction: ScrollDirection::Up }).is_err());
        assert!(encode(&ButtonAction::Keys { modifiers: 0, keys: vec![4, 5] }).is_err());
        assert!(encode(&ButtonAction::PollingSwitch).is_err());
    }
}
