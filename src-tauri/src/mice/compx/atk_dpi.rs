// DPI encodings from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/atk/index.ts (atkEncodeDpiAxis,
// atkDecodeDpiAxis, the PAW3395SE codes, the PAW3955 Master stage). Translated to Rust and modified
// for hrtz, 2026-11-13. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! How an ATK sensor stores a DPI stage, for the methods two readings of ATK's own app agree on
//! (mouse-protocol's of ATK HUB 3.2.27, hrtz's of 3.2.32, 2026-11-13). A 4-byte stage is X, Y, a
//! byte of two mode nibbles (X low, Y high) and a check byte; a PAW3955's 6-byte stage is X and Y
//! as 16-bit values, flags and a check byte. Each group sums to 0x55, as a Zaopin's does.

use super::protocol::checksum;

/// How a stage's DPI is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// 50 per step, the count's top bits in the mode nibble: a Zaopin's (PAW3395, PAW3950).
    Step50,
    /// 10 per step to 10,000, then 50 from 10,050 (bit 1), doubled above 30,000 (bit 0): the
    /// PAW3950 Ultra's.
    Ultra,
    /// A table of 200 codes for 50 to 10,000, doubled above (bit 1): the PAW3395 SE's.
    Paw3395Se,
    /// `dpi - 1` as 16 bits, in 6-byte stages from 0x1B00: the PAW3955's.
    Paw3955,
}

/// Where a PAW3955 keeps its stages, 6 bytes each.
pub const PAW3955_STAGES: usize = 0x1B00;

/// The PAW3395 SE's codes, in DPI order from 50: 1 to 235 but these.
const SE_SKIPPED: [u8; 35] = [
    7, 13, 20, 26, 33, 40, 46, 53, 60, 66, 73, 80, 86, 93, 100, 106, 113, 120, 126, 133, 140, 146, 153, 160, 166, 173,
    180, 186, 193, 200, 206, 213, 220, 226, 233,
];

fn se_codes() -> impl Iterator<Item = u8> {
    (1..=235u8).filter(|c| !SE_SKIPPED.contains(c))
}

/// One axis as (value byte, mode nibble).
fn encode_axis(method: Method, dpi: u32) -> Option<(u8, u8)> {
    match method {
        Method::Step50 => {
            let count = (dpi / 50).checked_sub(1)?;
            (dpi.is_multiple_of(50) && count < 1024).then_some((count as u8, ((count >> 8) << 2) as u8))
        }
        Method::Ultra => {
            if dpi > 30000 {
                return None;
            }
            if dpi <= 10000 {
                let code = (dpi / 10).checked_sub(1)?;
                (dpi.is_multiple_of(10)).then_some((code as u8, ((code >> 8) << 2) as u8))
            } else {
                let code = (dpi.is_multiple_of(50)).then(|| dpi.checked_sub(10050)).flatten()? / 50;
                Some((code as u8, (((code >> 8) << 2) | 2) as u8))
            }
        }
        Method::Paw3395Se => {
            let n = (dpi.is_multiple_of(50) && (50..=10000).contains(&dpi)).then_some(dpi / 50)?;
            se_codes().nth(n as usize - 1).map(|code| (code, 0))
        }
        Method::Paw3955 => None,
    }
}

fn decode_axis(method: Method, byte: u8, nibble: u8) -> Option<u32> {
    let count = (((nibble as u32 >> 2) & 3) << 8) | byte as u32;
    match method {
        Method::Step50 => {
            let dpi = (count + 1) * 50;
            Some(if nibble & 1 != 0 { dpi * 2 } else { dpi })
        }
        Method::Ultra => {
            let dpi = if nibble & 2 != 0 { 10050 + count * 50 } else { (count + 1) * 10 };
            Some(if nibble & 1 != 0 { dpi * 2 } else { dpi })
        }
        Method::Paw3395Se => {
            if nibble & !2 != 0 {
                return None;
            }
            let dpi = (se_codes().position(|c| c == byte)? as u32 + 1) * 50;
            Some(if nibble & 2 != 0 { dpi * 2 } else { dpi })
        }
        Method::Paw3955 => None,
    }
}

/// A stage of `dpi` on both axes, as the mouse keeps it: 4 bytes, or 6 for a PAW3955. None for a
/// DPI the method can't hold.
pub fn encode_stage(method: Method, dpi: u32) -> Option<Vec<u8>> {
    if method == Method::Paw3955 {
        let v = dpi.checked_sub(1).filter(|_| dpi <= 40000)?;
        let row = [v as u8, (v >> 8) as u8, v as u8, (v >> 8) as u8, 0];
        return Some([&row[..], &[checksum(&row)]].concat());
    }
    let (x, n) = encode_axis(method, dpi)?;
    let group = [x, x, (n << 4) | n];
    Some([&group[..], &[checksum(&group)]].concat())
}

/// A stage's DPI (its X axis), from its bytes; None when its check byte is off.
pub fn decode_stage(method: Method, bytes: &[u8]) -> Option<u32> {
    let (data, check) = bytes.split_at(bytes.len() - 1);
    if checksum(data) != check[0] {
        return None;
    }
    if method == Method::Paw3955 {
        let dpi = (data[0] as u32 | (data[1] as u32) << 8) + 1;
        return Some(if data[4] & 1 != 0 { dpi * 2 } else { dpi });
    }
    decode_axis(method, data[0], data[2] & 0x0F)
}

/// How many bytes a stage takes.
pub fn stage_len(method: Method) -> usize {
    if method == Method::Paw3955 { 6 } else { 4 }
}

/// Where stage `i` is kept.
pub fn stage_addr(method: Method, i: usize) -> usize {
    if method == Method::Paw3955 { PAW3955_STAGES + 6 * i } else { 0x0C + 4 * i }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step50_is_a_zaopins() {
        for dpi in [50, 800, 1600, 12800, 26000, 30000] {
            let (x, ex) = super::super::image::dpi_encode(dpi);
            assert_eq!(encode_stage(Method::Step50, dpi).unwrap(), [x, x, ex, checksum(&[x, x, ex])], "{dpi}");
        }
    }

    #[test]
    fn ultra_steps_by_10_then_by_50() {
        // 800: code 79; 10,000: code 999 (top bits 3); 26,000: (26000 - 10050) / 50 = 319, bit 1.
        assert_eq!(encode_stage(Method::Ultra, 800).unwrap()[..3], [79, 79, 0]);
        assert_eq!(encode_stage(Method::Ultra, 10000).unwrap()[..3], [0xE7, 0xE7, 0xCC]);
        assert_eq!(encode_stage(Method::Ultra, 26000).unwrap()[..3], [0x3F, 0x3F, 0x66]);
        assert!(encode_stage(Method::Ultra, 10025).is_none() && encode_stage(Method::Ultra, 30050).is_none());
        // Doubled above 30,000, as the mouse may hold it.
        assert_eq!(decode_axis(Method::Ultra, 0x3F, 0x07), Some(52000));
    }

    #[test]
    fn the_se_table_is_atks() {
        // ATK's app writes 400 DPI as code 9 (codes 7 and 13 are skipped).
        assert_eq!(encode_stage(Method::Paw3395Se, 400).unwrap()[0], 9);
        assert_eq!(se_codes().count(), 200);
        assert_eq!(encode_stage(Method::Paw3395Se, 10000).unwrap()[0], 235);
        assert!(encode_stage(Method::Paw3395Se, 10050).is_none());
        assert_eq!(decode_axis(Method::Paw3395Se, 235, 2), Some(20000));
        assert_eq!(decode_axis(Method::Paw3395Se, 7, 0), None, "a skipped code");
    }

    #[test]
    fn a_paw3955_stage_is_6_bytes_from_0x1b00() {
        let s = encode_stage(Method::Paw3955, 1600).unwrap();
        assert_eq!(s[..5], [0x3F, 0x06, 0x3F, 0x06, 0]);
        assert_eq!(s.iter().map(|b| *b as u32).sum::<u32>() & 0xFF, 0x55);
        assert_eq!((stage_addr(Method::Paw3955, 2), stage_len(Method::Paw3955)), (0x1B0C, 6));
        assert!(encode_stage(Method::Paw3955, 40050).is_none());
    }

    #[test]
    fn every_method_reads_back_what_it_writes() {
        for (method, range) in [
            (Method::Step50, (50, 30000)),
            (Method::Ultra, (50, 30000)),
            (Method::Paw3395Se, (200, 10000)),
            (Method::Paw3955, (50, 40000)),
        ] {
            for dpi in (range.0..=range.1).step_by(50) {
                let s = encode_stage(method, dpi).unwrap_or_else(|| panic!("{method:?} {dpi}"));
                assert_eq!(decode_stage(method, &s), Some(dpi), "{method:?} {dpi}");
            }
        }
        let mut bad = encode_stage(Method::Ultra, 800).unwrap();
        bad[3] ^= 1;
        assert_eq!(decode_stage(Method::Ultra, &bad), None, "a stage whose check byte is off");
    }
}
