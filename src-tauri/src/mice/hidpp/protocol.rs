// Ported from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/logitech/index.ts, with constants from
// src/drivers/logitech/hidpp.ts and src/drivers/vendors.ts. Translated to Rust and modified for
// hrtz, 2026-10-18. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! HID++ 2.0 on the wire: the two reports, the software ID that tells our replies from anyone
//! else's, the device index, the features hrtz asks for, and the error frames.

/// Logitech's USB vendor ID.
pub const VENDOR_ID: u16 = 0x046D;
/// The vendor page HID++ rides on over USB: usage 1 is the short-report collection, usage 2 the
/// long. Windows opens each collection on its own, so both are opened.
pub const USAGE_PAGE: u16 = 0xFF00;
pub const USAGE_SHORT: u16 = 0x0001;
pub const USAGE_LONG: u16 = 0x0002;

pub const SHORT: u8 = 0x10;
pub const LONG: u8 = 0x11;
/// Report lengths, the report ID included.
pub const SHORT_LEN: usize = 7;
pub const LONG_LEN: usize = 20;
/// Parameter bytes each report carries.
pub const SHORT_PARAMS: usize = SHORT_LEN - 4;
pub const LONG_PARAMS: usize = LONG_LEN - 4;

/// HID++ reserves software ID 0 for what the device sends by itself; a nonzero one keeps our
/// replies apart from those, and from G HUB's when it shares the receiver.
pub const SOFTWARE_ID: u8 = 0x05;

/// A mouse reached through its receiver answers on a pairing slot (1 to 6); a mouse on its own
/// cable answers as itself, on 0xFF.
pub const DIRECT: u8 = 0xFF;
pub const FIRST_SLOT: u8 = 0x01;

/// HID++ 1.0's error frame, and HID++ 2.0's.
pub const ERROR_10: u8 = 0x8F;
pub const ERROR_20: u8 = 0xFF;

/// The features hrtz asks for, by number. The mouse says at which index it keeps each.
pub mod feature {
    pub const ROOT: u16 = 0x0000;
    pub const FIRMWARE: u16 = 0x0003;
    pub const NAME: u16 = 0x0005;
    pub const BATTERY_STATUS: u16 = 0x1000;
    pub const BATTERY_VOLTAGE: u16 = 0x1001;
    pub const UNIFIED_BATTERY: u16 = 0x1004;
    pub const ADC_MEASUREMENT: u16 = 0x1F20;
    pub const ADJUSTABLE_DPI: u16 = 0x2201;
    pub const EXTENDED_DPI: u16 = 0x2202;
    pub const REPORT_RATE: u16 = 0x8060;
    pub const EXTENDED_REPORT_RATE: u16 = 0x8061;
    pub const ONBOARD_PROFILES: u16 = 0x8100;
}

/// Our function byte: the function in the high nibble, our software ID in the low one.
pub fn with_software_id(function: u8) -> u8 {
    (function & 0xF0) | SOFTWARE_ID
}

/// Device indices to try, in order, to find which one the mouse answers on. A receiver forwards to
/// its pairing slots, and G HUB merging a keyboard onto it can push the mouse off the first one, so
/// every slot comes before the direct index. A mouse on its cable answers as itself, with the
/// first slot only a fallback.
pub fn index_candidates(receiver: bool) -> &'static [u8] {
    if receiver { &[FIRST_SLOT, 0x02, 0x03, 0x04, 0x05, 0x06, DIRECT] } else { &[DIRECT, FIRST_SLOT] }
}

/// A request, as written: report ID, device index, feature index, function, then the parameters.
/// Short when they fit in three bytes, long otherwise (or when `long` asks for it).
pub fn request(index: u8, feature_index: u8, function: u8, params: &[u8], long: bool) -> Vec<u8> {
    assert!(params.len() <= LONG_PARAMS, "HID++ requests carry at most 16 parameter bytes");
    let long = long || params.len() > SHORT_PARAMS;
    let mut r = vec![0u8; if long { LONG_LEN } else { SHORT_LEN }];
    r[0] = if long { LONG } else { SHORT };
    r[1] = index;
    r[2] = feature_index;
    r[3] = with_software_id(function);
    r[4..4 + params.len()].copy_from_slice(params);
    r
}

/// Why the mouse (or its receiver) refused a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// HID++ 1.0, from the receiver: an empty slot, a mouse it can't reach.
    V10(u8),
    /// HID++ 2.0, from the mouse.
    V20(u8),
}

impl Refusal {
    /// HID++ 1.0's "resource error" and "connection request failed": the slot holds a mouse the
    /// receiver can't reach now, asleep or switched off.
    pub fn unreachable(self) -> bool {
        matches!(self, Refusal::V10(0x04 | 0x09))
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (version, code, reason) = match *self {
            Refusal::V10(c) => ("1.0", c, hidpp10_reason(c)),
            Refusal::V20(c) => ("2.0", c, hidpp20_reason(c)),
        };
        match reason {
            Some(r) => write!(f, "The mouse rejected that ({r})."),
            None => write!(f, "The mouse rejected that (HID++ {version} error 0x{code:02X})."),
        }
    }
}

fn hidpp20_reason(code: u8) -> Option<&'static str> {
    Some(match code {
        0x01 => "unknown request",
        0x02 => "invalid argument",
        0x03 => "value out of range",
        0x04 => "hardware error",
        0x05 => "Logitech internal error",
        0x06 => "invalid feature index",
        0x07 => "invalid function",
        0x08 => "device busy",
        0x09 => "unsupported",
        _ => return None,
    })
}

fn hidpp10_reason(code: u8) -> Option<&'static str> {
    Some(match code {
        0x01 => "invalid command",
        0x02 => "invalid address",
        0x03 => "invalid value",
        0x04 => "connection request failed",
        0x05 => "too many devices",
        0x06 => "already exists",
        0x07 => "device busy",
        0x08 => "unknown device",
        0x09 => "resource error",
        0x0A => "request unavailable",
        0x0B => "unsupported parameter value",
        0x0C => "wrong PIN code",
        _ => return None,
    })
}

/// A short or long HID++ report, whole.
fn hidpp(report: &[u8]) -> bool {
    (report.first() == Some(&SHORT) && report.len() >= SHORT_LEN)
        || (report.first() == Some(&LONG) && report.len() >= LONG_LEN)
}

/// The refusal in `report` (report ID first), if it answers our request to `feature_index` and
/// `function` on device `index`. HID++ 1.0 is `[dev, 0x8F, sub-id, address, error]`, HID++ 2.0
/// `[dev, 0xFF, feature index, function + software ID, error]`.
pub fn refusal_for(report: &[u8], index: u8, feature_index: u8, function: u8) -> Option<Refusal> {
    if !hidpp(report) || report[1] != index || report[3] != feature_index || report[4] != with_software_id(function) {
        return None;
    }
    match report[2] {
        ERROR_10 => Some(Refusal::V10(report[5])),
        ERROR_20 => Some(Refusal::V20(report[5])),
        _ => None,
    }
}

/// Whether `report` is the answer to our request to `feature_index` and `function` on `index`.
/// Anyone else's answers (G HUB's, on the same receiver) carry another software ID.
pub fn answers(report: &[u8], index: u8, feature_index: u8, function: u8) -> bool {
    hidpp(report) && report[1] == index && report[2] == feature_index && report[3] == with_software_id(function)
}

/// Whether `report` is something device `index` sent by itself: a HID++ 2.0 event, from one of its
/// features (`report[2]`), with software ID 0. The receiver's own HID++ 1.0 notices number from
/// 0x40 and aren't these.
pub fn is_event(report: &[u8], index: u8) -> bool {
    hidpp(report) && report[1] == index && report[2] < 0x40 && report[3] & 0x0F == 0
}

/// The report rates 0x8061 (and the profiles of format 6 on) number from 0.
pub const REPORT_RATE_HZ: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// 0x8061's rate mask: bit N is `REPORT_RATE_HZ[N]`.
pub fn rate_mask(mask: u16) -> Vec<u32> {
    REPORT_RATE_HZ.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, hz)| *hz).collect()
}

/// 0x8060's rate list: bit i is an interval of (i + 1) ms. The G402 generation reports 0x8B: 1, 2,
/// 4 and 8 ms, so 1,000, 500, 250 and 125 Hz. Lowest first.
pub fn rate_bitmap(bits: u8) -> Vec<u32> {
    let mut rates: Vec<u32> =
        (0..8).filter(|b| bits & (1 << b) != 0).map(|b| (1000.0 / (b + 1) as f64).round() as u32).collect();
    rates.sort_unstable();
    rates
}

/// How the battery is charging, as the mouse says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Charge {
    Discharging,
    Charging,
    ChargingSlowly,
    AlmostFull,
    Full,
    Unknown,
}

impl Charge {
    pub fn charging(self) -> bool {
        matches!(self, Charge::Charging | Charge::ChargingSlowly | Charge::AlmostFull | Charge::Full)
    }
}

/// 0x1004's charging status. Not 0x1000's: the two number their states differently, and reading
/// one with the other's table calls a slow charge "almost full". 4 is a charging fault.
pub fn unified_battery_state(status: u8) -> Charge {
    match status {
        0 => Charge::Discharging,
        1 => Charge::Charging,
        2 => Charge::ChargingSlowly,
        3 => Charge::Full,
        _ => Charge::Unknown,
    }
}

/// 0x1000's battery status. 5 is an invalid battery, 6 a thermal error, 7 another charging error.
pub fn battery_level_state(status: u8) -> Charge {
    match status {
        0 => Charge::Discharging,
        1 => Charge::Charging,
        2 => Charge::AlmostFull,
        3 => Charge::Full,
        4 => Charge::ChargingSlowly,
        _ => Charge::Unknown,
    }
}

/// Voltage (mV) to percent for the mice that report only a voltage (0x1001, 0x1F20).
const VOLTAGE_CURVE: [(u16, u8); 13] = [
    (4186, 100),
    (4067, 90),
    (3989, 80),
    (3922, 70),
    (3859, 60),
    (3811, 50),
    (3778, 40),
    (3751, 30),
    (3717, 20),
    (3671, 10),
    (3646, 5),
    (3579, 2),
    (3500, 0),
];

pub fn voltage_percent(mv: u16) -> u8 {
    if mv >= VOLTAGE_CURVE[0].0 {
        return 100;
    }
    if mv <= VOLTAGE_CURVE[VOLTAGE_CURVE.len() - 1].0 {
        return 0;
    }
    for w in VOLTAGE_CURVE.windows(2) {
        let ((high_mv, high), (low_mv, low)) = (w[0], w[1]);
        if mv >= low_mv {
            let span = (high_mv - low_mv) as f64;
            return (low as f64 + (high - low) as f64 * (mv - low_mv) as f64 / span).round() as u8;
        }
    }
    0
}

/// A DPI list as 0x2201 and 0x2202 send it: big-endian values, where one with its top three bits
/// set is a range, `0xE000 | step` between the value before it and the one after. Ends at 0.
pub fn dpi_list(bytes: &[u8]) -> Option<Vec<u32>> {
    let mut out: Vec<u32> = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let v = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32;
        if v == 0 {
            break;
        }
        if v >> 13 == 0b111 {
            let step = v & 0x1FFF;
            let last = bytes.get(i + 2..i + 4).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as u32);
            let first = *out.last()?;
            if step == 0 || last <= first {
                return None;
            }
            out.extend((first + step..=last).step_by(step as usize));
            i += 4;
        } else {
            out.push(v);
            i += 2;
        }
    }
    Some(out)
}
