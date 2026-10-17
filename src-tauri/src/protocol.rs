//! Wire format of the Zaopin/JinLeMa vendor HID channel: 17-byte reports, one request at a time.
//! `docs/PROTOCOL.md` has the commands and the memory map.

pub const VENDOR_ID: u16 = 0x3554;
pub const CONFIG_USAGE_PAGE: u16 = 0xFF02;
pub const CONFIG_USAGE: u16 = 0x02;

pub const REPORT_ID: u8 = 0x08;
pub const PACKET_LEN: usize = 17;
pub const MAX_CHUNK: usize = 10;
pub const FLASH_SIZE: usize = 6912;
/// Mouse config, DPI, LEDs and button map live below this address; shortcuts and macros above.
pub const CONFIG_REGION: usize = 0xC0;

pub type Packet = [u8; PACKET_LEN];

pub mod cmd {
    pub const HANDSHAKE: u8 = 0x01;
    pub const PC_DRIVER_STATUS: u8 = 0x02;
    pub const ONLINE: u8 = 0x03;
    pub const BATTERY: u8 = 0x04;
    pub const ENTER_PAIR: u8 = 0x05;
    pub const PAIR_STATE: u8 = 0x06;
    pub const WRITE_FLASH: u8 = 0x07;
    pub const READ_FLASH: u8 = 0x08;
    pub const FACTORY_RESET: u8 = 0x09;
    pub const STATUS_CHANGED: u8 = 0x0A;
    pub const GET_PROFILE: u8 = 0x0E;
    pub const SET_PROFILE: u8 = 0x0F;
    pub const VERSION: u8 = 0x12;
    pub const SET_LONG_RANGE: u8 = 0x16;
    pub const GET_LONG_RANGE: u8 = 0x17;
}

/// Bits of the unsolicited STATUS_CHANGED payload.
pub mod status {
    pub const DPI: u8 = 0x01;
    pub const POLLING: u8 = 0x02;
    pub const PROFILE: u8 = 0x04;
    pub const BATTERY: u8 = 0x40;
}

/// Byte that makes `bytes` plus itself sum to 0x55 (mod 256). Used for packets and flash fields.
pub fn checksum(bytes: &[u8]) -> u8 {
    0x55u8.wrapping_sub(bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)))
}

pub fn group_valid(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0x55
}

pub fn packet(command: u8, addr: u16, data: &[u8], len: Option<u8>) -> Packet {
    assert!(data.len() <= MAX_CHUNK, "at most {MAX_CHUNK} data bytes per packet");
    let mut p = [0u8; PACKET_LEN];
    p[0] = REPORT_ID;
    p[1] = command;
    p[3] = (addr >> 8) as u8;
    p[4] = addr as u8;
    p[5] = len.unwrap_or(data.len() as u8);
    p[6..6 + data.len()].copy_from_slice(data);
    p[16] = checksum(&p[..16]);
    p
}

/// The receiver into pairing, for any mouse of brand `cid`, as the vendor app sends it: length 2,
/// the brand in the third data byte.
pub fn pair_packet(cid: u8) -> Packet {
    packet(cmd::ENTER_PAIR, 0, &[0, 0, cid], Some(2))
}

/// The receiver's answer to `PAIR_STATE`, in its first data byte.
pub mod pair_state {
    pub const PAIRING: u8 = 1;
    pub const FAILED: u8 = 2;
    pub const PAIRED: u8 = 3;
}

pub fn packet_valid(p: &[u8]) -> bool {
    p.len() >= PACKET_LEN && p[0] == REPORT_ID && checksum(&p[..16]) == p[16]
}

pub fn packet_addr(p: &Packet) -> u16 {
    u16::from_be_bytes([p[3], p[4]])
}

pub fn payload(p: &Packet) -> &[u8] {
    &p[6..16]
}

#[cfg(test)]
mod tests {

    #[test]
    fn pairing_is_sent_as_the_vendor_sends_it() {
        let p = pair_packet(7);
        assert_eq!(&p[..9], &[0x08, 0x05, 0, 0, 0, 2, 0, 0, 7]);
        assert!(p[9..16].iter().all(|b| *b == 0));
        assert!(packet_valid(&p));
    }

    use super::*;

    #[test]
    fn matches_captured_handshake_reply() {
        // Reply captured from a Z1 PRO MAX on the 8K dongle.
        let reply = [
            0x08, 0x01, 0x00, 0x00, 0x00, 0x08, 0x49, 0x05, 0x0c, 0xea, 0x07, 0x04, 0x05, 0x00, 0x00, 0x00, 0xf0,
        ];
        assert!(packet_valid(&reply));
    }

    #[test]
    fn builds_read_flash() {
        let p = packet(cmd::READ_FLASH, 0x00A9, &[], Some(2));
        assert_eq!(&p[..6], &[0x08, 0x08, 0x00, 0x00, 0xA9, 0x02]);
        assert!(packet_valid(&p));
    }
}
