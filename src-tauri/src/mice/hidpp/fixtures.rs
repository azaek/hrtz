// Ported from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: the captures in
// src/drivers/logitech/onboard-profiles.test.ts. Translated to Rust and modified for hrtz,
// 2026-10-18. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! Profile memory read from real Logitech mice, for the tests and the simulated mice.

/// "01 0d 00" into bytes.
pub fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().map(|b| u8::from_str_radix(b, 16).expect("a hex byte")).collect()
}

/// `len` erased bytes (0xFF) with `parts` laid over them at their offsets.
fn erased_with(len: usize, parts: &[(usize, &str)]) -> Vec<u8> {
    let mut out = vec![0xFF; len];
    for (at, bytes) in parts {
        let b = hex(bytes);
        out[*at..*at + b.len()].copy_from_slice(&b);
    }
    out
}

/// A PRO X SUPERLIGHT (0xC094, main application MPM25.01_B0018): getInfo's parameters. Format 4,
/// five profiles, 16 sectors of 255 bytes.
pub fn superlight_info() -> Vec<u8> {
    hex("01 04 01 05 01 05 10 00 ff 0a 04 00 00")
}

/// Its sector 1, untouched from the factory: one 800 DPI stage, 1,000 Hz, CRC `dd 75`.
pub fn superlight_sector_1() -> Vec<u8> {
    erased_with(
        255,
        &[
            (0x00, "01 00 00 20 03 00 00 00 00 00 00 00 00 ff ff ff"),
            (0x10, "ff 00 ff ff ff ff ff ff ff ff ff ff ff ff ff ff"),
            (0x20, "80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08"),
            (0x30, "80 01 00 10 ff ff ff ff ff ff ff ff ff ff ff ff"),
            (0xD0, "00 00 00 00 00 00 1f 40 00 00 00 00 00 00 00 00"),
            (0xE0, "00 1f 40 00 00 00 ff ff ff ff ff ff ff ff ff ff"),
            (0xF0, "ff ff ff ff ff ff ff ff ff ff ff ff ff dd 75"),
        ],
    )
}

/// A PRO X SUPERLIGHT 2: getInfo's parameters. Format 7, five profiles, 16 sectors of 255 bytes.
pub fn superlight2_info() -> Vec<u8> {
    hex("01 07 01 05 01 05 10 00 ff 0a 04 00 00")
}

/// Its directory: five profiles, sector 3's flag set.
pub fn superlight2_directory() -> Vec<u8> {
    erased_with(255, &[(0x00, "00 01 00 ff 00 02 00 ff 00 03 01 ff 00 04 00 ff 00 05 00 ff")])
}

/// Its sector 3, set up in G HUB: named "myp", one 1,600 DPI stage at medium lift-off, 1,000 Hz on
/// both links, G-shift buttons set. CRC 0x20c7.
pub fn superlight2_sector_3() -> Vec<u8> {
    hex("03 03 00 00 40 06 40 06 02 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
         00 ff 00 ff ff 00 ff ff ff ff ff ff 3c 00 2c 01
         80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08
         80 01 00 10 ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08
         80 01 00 10 ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         6d 00 79 00 70 00 00 00 00 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
         03 00 00 00 00 00 1f 40 00 00 00 03 00 00 00 00
         00 1f 40 00 00 00 03 00 00 00 00 00 1f 40 32 00
         00 03 00 00 00 00 00 1f 40 32 00 00 03 20 c7")
}

/// Its untouched factory profile (sectors 1, 2, 4 and 5): no name, stages 800, 1,200, 1,600,
/// 2,400 and 3,200 at medium lift-off. CRC 0x84db.
pub fn superlight2_factory() -> Vec<u8> {
    hex("03 03 00 00 20 03 20 03 02 b0 04 b0 04 02 40 06
         40 06 02 60 09 60 09 02 80 0c 80 0c 02 00 00 00
         00 ff 00 ff ff ff ff ff ff ff ff ff 3c 00 2c 01
         80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08
         80 01 00 10 ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         03 00 00 00 00 00 1f 40 00 00 00 03 00 00 00 00
         00 1f 40 00 00 00 03 00 00 00 00 00 1f 40 32 00
         00 03 00 00 00 00 00 1f 40 32 00 00 03 84 db")
}

/// A G102 LIGHTSYNC (0xC092), format 4 like the first Superlight: getInfo's parameters.
#[cfg(test)]
pub fn g102_info() -> Vec<u8> {
    hex("01 04 01 01 01 06 10 00 ff 0a 01 00 00 00 00 00")
}

/// Its profile: stages 50, 2,000 and 1,600, the second the default, 1,000 Hz, named
/// "PROFILE_NAME_DEFAULT".
#[cfg(test)]
pub fn g102_profile() -> Vec<u8> {
    hex("01 01 01 32 00 d0 07 40 06 00 00 00 00 ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         80 01 00 01 80 01 00 02 80 01 00 04 80 01 00 08
         80 01 00 10 90 05 ff 00 ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
         50 00 52 00 4f 00 46 00 49 00 4c 00 45 00 5f 00
         4e 00 41 00 4d 00 45 00 5f 00 44 00 45 00 46 00
         41 00 55 00 4c 00 54 00 00 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
         00 00 00 00 00 00 00 00 00 00 00 00 ff 68 27")
}

/// Its directory: one profile, enabled. CRC 0x587c.
#[cfg(test)]
pub fn g102_directory() -> Vec<u8> {
    erased_with(255, &[(0x00, "00 01 01 00 ff ff 00 00"), (253, "58 7c")])
}

/// A PRO X 3 SUPERSTRIKE's sector 1 (format 8, as the PRO X 2's): stages 800 to 3,200, all at
/// medium lift-off, and the HITS block. CRC 0x2a38.
#[cfg(test)]
pub fn superstrike_sector_1() -> Vec<u8> {
    let mut s = superlight2_factory();
    s[0x26..0x2C].copy_from_slice(&hex("14 08 0c 14 08 0c"));
    s[253] = 0x2A;
    s[254] = 0x38;
    s
}
