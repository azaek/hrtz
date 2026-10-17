//! A Zaopin mouse and its receiver, simulated: it answers the vendor channel's commands the way the
//! real ones do (docs/PROTOCOL.md), from a made-up flash image, so the device thread can be tested
//! without hardware. The test holds the same `SimMouse` and can change it in between: wake it,
//! press its DPI button, switch its profile, pull the receiver out.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use parking_lot::Mutex;

use super::driver::{Opened, connect};
use super::image::sample_image;
use super::protocol::{self as p, Packet, checksum, cmd};
use crate::mice::{Board, Driver, Family, Finder, Link};

pub struct SimMouse {
    pub cid: u8,
    pub mid: u8,
    pub device_type: u8,
    /// Awake and linked to the receiver.
    pub online: bool,
    /// Each profile's memory; reads and writes go to the one in use.
    pub profiles: Vec<Vec<u8>>,
    pub active: u8,
    pub long_range: bool,
    /// Level, charging, millivolts.
    pub battery: (u8, bool, u16),
    pub firmware: [u8; 2],
    /// The receiver's answers to `PAIR_STATE`, in turn; the last one repeats.
    pub pair_states: VecDeque<u8>,
    /// What's waiting to be read: answers, and status packets the mouse sent by itself.
    pub out: VecDeque<Packet>,
    /// Every request received, in order.
    pub requests: Vec<Packet>,
    /// The receiver is unplugged: every read and write fails.
    pub unplugged: bool,
    /// Answers a write at this address with other bytes, as a mouse that refuses it.
    pub reject_write_at: Option<usize>,
    /// Answers writes but keeps nothing, so a read-back finds the old bytes.
    pub forget_writes: bool,
    /// How many receivers (or mice on cables) are plugged in.
    pub receivers: usize,
}

impl Default for SimMouse {
    /// A Z1 PRO MAX on the 8K receiver, awake, on profile 1, with the test image in all four.
    fn default() -> Self {
        Self {
            cid: 7,
            mid: 4,
            device_type: 5,
            online: true,
            profiles: vec![sample_image(); 4],
            active: 0,
            long_range: false,
            battery: (80, false, 3950),
            firmware: [0x02, 0x17],
            pair_states: VecDeque::from([p::pair_state::PAIRING]),
            out: VecDeque::new(),
            requests: Vec::new(),
            unplugged: false,
            reject_write_at: None,
            forget_writes: false,
            receivers: 1,
        }
    }
}

impl SimMouse {
    pub fn image(&self) -> &[u8] {
        &self.profiles[self.active as usize]
    }

    pub fn image_mut(&mut self) -> &mut Vec<u8> {
        let i = self.active as usize;
        &mut self.profiles[i]
    }

    #[cfg(test)]
    /// A one-byte field and its check byte, as the vendor app writes them.
    pub fn put_pair(&mut self, addr: usize, v: u8) {
        let f = self.image_mut();
        f[addr] = v;
        f[addr + 1] = checksum(&[v]);
    }

    /// The mouse says something changed (`protocol::status` bits), as after a button press on it.
    pub fn status(&mut self, bits: u8) {
        self.out.push_back(p::packet(cmd::STATUS_CHANGED, 0, &[bits], None));
    }

    #[cfg(test)]
    /// The commands received since `from`, in order.
    pub fn commands_since(&self, from: usize) -> Vec<u8> {
        self.requests[from..].iter().map(|r| r[1]).collect()
    }

    #[cfg(test)]
    /// The `(address, length)` of every flash read since `from`.
    pub fn reads_since(&self, from: usize) -> Vec<(usize, usize)> {
        self.requests[from..]
            .iter()
            .filter(|r| r[1] == cmd::READ_FLASH)
            .map(|r| (p::packet_addr(r) as usize, r[5] as usize))
            .collect()
    }

    #[cfg(test)]
    /// The address of every flash write since `from`.
    pub fn writes_since(&self, from: usize) -> Vec<usize> {
        self.requests[from..].iter().filter(|r| r[1] == cmd::WRITE_FLASH).map(|r| p::packet_addr(r) as usize).collect()
    }

    fn answer(&mut self, r: &Packet) {
        let reply = |data: &[u8]| p::packet(r[1], 0, data, None);
        // Asleep, the receiver still answers for itself; the mouse doesn't.
        let mouse_command = !matches!(r[1], cmd::HANDSHAKE | cmd::PC_DRIVER_STATUS | cmd::ONLINE | cmd::PAIR_STATE);
        if mouse_command && !self.online {
            return;
        }
        let answer = match r[1] {
            cmd::HANDSHAKE => {
                p::packet(cmd::HANDSHAKE, 0, &[0x49, 0x05, 0x0C, 0xEA, self.cid, self.mid, self.device_type], Some(8))
            }
            cmd::PC_DRIVER_STATUS => reply(&[r[6]]),
            cmd::ONLINE => reply(&[self.online as u8]),
            cmd::BATTERY => {
                let (level, charging, mv) = self.battery;
                let [hi, lo] = mv.to_be_bytes();
                reply(&[level, charging as u8, hi, lo])
            }
            cmd::VERSION => reply(&self.firmware),
            cmd::GET_PROFILE => p::packet(cmd::GET_PROFILE, 0, &[self.active], Some(1)),
            cmd::SET_PROFILE => {
                self.active = r[6].min(3);
                let ack = reply(&[r[6]]);
                // The mouse reports every profile switch, the app's own too, just after the answer.
                self.out.push_back(ack);
                self.status(p::status::PROFILE);
                return;
            }
            cmd::GET_LONG_RANGE => reply(&[self.long_range as u8]),
            cmd::SET_LONG_RANGE => {
                self.long_range = r[6] != 0;
                reply(&[r[6]])
            }
            cmd::READ_FLASH => {
                let (addr, len) = (p::packet_addr(r) as usize, r[5] as usize);
                let data = self.image()[addr..addr + len].to_vec();
                p::packet(cmd::READ_FLASH, addr as u16, &data, None)
            }
            cmd::WRITE_FLASH => {
                let (addr, len) = (p::packet_addr(r) as usize, r[5] as usize);
                if !self.forget_writes {
                    let data = r[6..6 + len].to_vec();
                    self.image_mut()[addr..addr + len].copy_from_slice(&data);
                }
                if self.reject_write_at == Some(addr) {
                    let mut wrong = *r;
                    wrong[6] ^= 0xFF;
                    wrong[16] = checksum(&wrong[..16]);
                    wrong
                } else {
                    *r
                }
            }
            cmd::FACTORY_RESET => {
                let i = self.active as usize;
                self.profiles[i] = sample_image();
                reply(&[])
            }
            cmd::ENTER_PAIR => return,
            cmd::PAIR_STATE => {
                let state = if self.pair_states.len() > 1 {
                    self.pair_states.pop_front()
                } else {
                    self.pair_states.front().copied()
                };
                reply(&[state.unwrap_or(p::pair_state::PAIRING)])
            }
            _ => return,
        };
        self.out.push_back(answer);
    }
}

/// The simulated receiver, as the device thread sees it.
pub struct SimLink(pub Arc<Mutex<SimMouse>>);

impl Link for SimLink {
    fn write(&mut self, report: &[u8]) -> Result<()> {
        let mut m = self.0.lock();
        if m.unplugged {
            bail!("the device is not connected");
        }
        let r: Packet = report.try_into()?;
        assert!(p::packet_valid(&r), "the app sent a packet with a bad checksum: {r:02X?}");
        m.requests.push(r);
        m.answer(&r);
        Ok(())
    }

    /// Answers at once: an empty queue means the receiver had nothing to say.
    fn read(&mut self, buf: &mut [u8], _timeout: Duration) -> Result<usize> {
        let mut m = self.0.lock();
        if m.unplugged {
            bail!("the device is not connected");
        }
        match m.out.pop_front() {
            Some(r) => {
                buf[..r.len()].copy_from_slice(&r);
                Ok(r.len())
            }
            None => Ok(0),
        }
    }
}

/// Finds the simulated receiver while it's plugged in.
pub struct SimFinder(pub Arc<Mutex<SimMouse>>);

impl Finder for SimFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        if self.0.lock().unplugged {
            bail!("no Zaopin receiver found");
        }
        let opened = Opened {
            link: Box::new(SimLink(self.0.clone())),
            gap: Duration::ZERO,
            #[cfg(windows)]
            port: None,
        };
        connect(opened, board)
    }

    fn count(&mut self) -> usize {
        self.0.lock().receivers
    }
}

/// Simulated mice beside the real ones, in a dev build: `HRTZ_SIM=z2mini` (or `z1promax`, or both,
/// comma-separated), to try several mice with one. Their keys start `sim/`, so they never take over
/// what a real Zaopin keeps.
pub struct SimFamily {
    pub(crate) mice: Vec<(String, Arc<Mutex<SimMouse>>)>,
}

impl SimFamily {
    /// These mice, by connection id.
    #[cfg(test)]
    pub fn of(mice: Vec<(&str, SimMouse)>) -> Self {
        Self { mice: mice.into_iter().map(|(id, m)| (id.to_string(), Arc::new(Mutex::new(m)))).collect() }
    }

    pub fn from_env() -> Option<Self> {
        let names = std::env::var("HRTZ_SIM").ok()?;
        let mice: Vec<_> = names
            .split(',')
            .filter_map(|n| {
                let m = match n.trim() {
                    "z2mini" => SimMouse { mid: 3, device_type: 1, battery: (64, false, 3850), ..Default::default() },
                    "z1promax" => SimMouse::default(),
                    _ => return None,
                };
                Some((format!("sim:{}", n.trim()), Arc::new(Mutex::new(m))))
            })
            .collect();
        (!mice.is_empty()).then_some(Self { mice })
    }
}

impl Family for SimFamily {
    fn connections(&self, _devices: &[hidapi::DeviceInfo]) -> Vec<String> {
        self.mice.iter().filter(|(_, m)| !m.lock().unplugged).map(|(id, _)| id.clone()).collect()
    }

    fn finder(&self, id: &str) -> Box<dyn Finder> {
        let mouse = self.mice.iter().find(|(i, _)| i == id).map(|(_, m)| m.clone()).expect("a simulated mouse");
        Box::new(DevSimFinder(SimFinder(mouse)))
    }
}

/// The simulated mouse under a `sim/` key.
struct DevSimFinder(SimFinder);

impl Finder for DevSimFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        let d = self.0.find(board)?;
        board.set(|s| s.key = s.key.as_ref().map(|k| k.replacen("compx/", "sim/", 1)));
        Ok(d)
    }

    fn count(&mut self) -> usize {
        self.0.count()
    }
}
