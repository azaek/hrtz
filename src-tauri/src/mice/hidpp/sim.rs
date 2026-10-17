//! A Logitech mouse and its receiver, simulated: it answers HID++ 2.0 the way OpenMouse found the
//! Superlights do, from profile memory read off real ones (`fixtures`), so the driver and the
//! engine can be tested without hardware. The receiver answers empty slots, and a mouse it can't
//! reach, with HID++ 1.0 refusals. The test holds the same `SimMouse` and can change it in between.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use parking_lot::Mutex;

use super::driver::{Opened, connect};
use super::fixtures;
use super::onboard::{self as ob, func};
use super::protocol::{self as p, feature};
use crate::mice::{Board, Driver, Family, Finder, Link};

/// How the mouse reports its battery.
#[derive(Clone, Copy, Debug)]
pub enum Battery {
    /// 0x1004: a percentage and a charging status.
    Unified { percent: u8, status: u8 },
    /// 0x1001: a voltage and flags (0x80 charging).
    Voltage { mv: u16, flags: u8 },
}

pub struct SimMouse {
    /// What's plugged in: a receiver's product ID, or the mouse's own on its cable.
    pub product_id: u16,
    /// The device index the mouse answers on.
    pub slot: u8,
    /// 0xFF answers HID++ 2.0 with no sensor behind it, as the first Superlight's does.
    pub empty_direct: bool,
    /// A keyboard paired on this slot too.
    pub keyboard: Option<u8>,
    /// Awake and linked to the receiver.
    pub online: bool,
    pub unit: [u8; 4],
    pub wireless_id: u16,
    pub usb_id: u16,
    pub name: &'static str,
    pub battery: Battery,
    /// 0x2202 and 0x8061, as the Superlight 2; else 0x2201 and 0x8060, as the first.
    pub extended: bool,
    pub live_dpi: u16,
    /// 0x8061's rate index on each link.
    pub rate: u8,
    /// getInfo's parameters.
    pub info: Vec<u8>,
    pub sectors: BTreeMap<u16, Vec<u8>>,
    pub current: u16,
    pub mode: u8,
    /// A sector being written: where, how long, and what came so far.
    pub writing: Option<(u16, usize, Vec<u8>)>,
    /// What's waiting to be read.
    pub out: VecDeque<Vec<u8>>,
    /// Every request received, in order.
    pub requests: Vec<Vec<u8>>,
    pub unplugged: bool,
    /// Answers memoryWriteEnd but keeps nothing.
    pub forget_writes: bool,
    /// Another program's answer comes first, every time (G HUB, on the same receiver).
    pub chatter: bool,
}

impl SimMouse {
    /// A PRO X SUPERLIGHT on its LIGHTSPEED receiver (0xC539), slot 1: profile format 4, its
    /// factory profile in all five, battery by voltage.
    pub fn superlight() -> Self {
        let mut sectors = BTreeMap::new();
        let mut directory = vec![0xFF; 255];
        for s in 1..=5u16 {
            directory[(s as usize - 1) * 4..s as usize * 4].copy_from_slice(&[0, s as u8, (s == 1) as u8, 0xFF]);
            sectors.insert(s, fixtures::superlight_sector_1());
        }
        ob::apply_crc(&mut directory);
        sectors.insert(0, directory);
        Self {
            product_id: 0xC539,
            slot: p::FIRST_SLOT,
            empty_direct: false,
            keyboard: None,
            online: true,
            unit: [0x1A, 0x2B, 0x3C, 0x4D],
            wireless_id: 0x4093,
            usb_id: 0xC094,
            name: "PRO X Wireless",
            battery: Battery::Voltage { mv: 3989, flags: 0x00 },
            extended: false,
            live_dpi: 800,
            rate: 3,
            info: fixtures::superlight_info(),
            sectors,
            current: 1,
            mode: ob::MODE_ONBOARD,
            writing: None,
            out: VecDeque::new(),
            requests: Vec::new(),
            unplugged: false,
            forget_writes: false,
            chatter: false,
        }
    }

    #[cfg(test)]
    /// The same mouse on its cable (0xC094): its own 0xFF answers with nothing behind it, and the
    /// mouse is on 1.
    pub fn superlight_cabled() -> Self {
        Self { product_id: 0xC094, empty_direct: true, ..Self::superlight() }
    }

    /// A PRO X SUPERLIGHT 2 on its receiver (0xC54D): format 7, the profile set up in G HUB in use
    /// (sector 3) and factory ones around it, battery by percentage.
    pub fn superlight2() -> Self {
        let mut sectors = BTreeMap::new();
        sectors.insert(0, fixtures::superlight2_directory());
        for s in [1, 2, 4, 5] {
            sectors.insert(s, fixtures::superlight2_factory());
        }
        sectors.insert(3, fixtures::superlight2_sector_3());
        Self {
            product_id: 0xC54D,
            unit: [0x5E, 0x6F, 0x70, 0x81],
            wireless_id: 0x40A9,
            usb_id: 0xC09B,
            name: "PRO X SUPERLIGHT 2",
            battery: Battery::Unified { percent: 72, status: 0 },
            extended: true,
            live_dpi: 1600,
            info: fixtures::superlight2_info(),
            sectors,
            current: 3,
            ..Self::superlight()
        }
    }

    /// The features, each at its index.
    fn features(&self) -> Vec<u16> {
        let battery = match self.battery {
            Battery::Unified { .. } => feature::UNIFIED_BATTERY,
            Battery::Voltage { .. } => feature::BATTERY_VOLTAGE,
        };
        let (dpi, rate) = if self.extended {
            (feature::EXTENDED_DPI, feature::EXTENDED_REPORT_RATE)
        } else {
            (feature::ADJUSTABLE_DPI, feature::REPORT_RATE)
        };
        vec![feature::ROOT, 0x0001, feature::FIRMWARE, feature::NAME, battery, dpi, rate, feature::ONBOARD_PROFILES]
    }

    #[cfg(test)]
    pub fn index_of(&self, id: u16) -> u8 {
        self.features().iter().position(|f| *f == id).unwrap_or(0) as u8
    }

    #[cfg(test)]
    /// The mouse switched profile by itself, and says so.
    pub fn switch_on_mouse(&mut self, sector: u16) {
        self.current = sector;
        let fi = self.index_of(feature::ONBOARD_PROFILES);
        let [hi, lo] = sector.to_be_bytes();
        self.out.push_back(long(self.slot, fi, 0x00, &[hi, lo]));
    }

    #[cfg(test)]
    /// The battery changed, and the mouse says so.
    pub fn battery_event(&mut self) {
        let fi = match self.battery {
            Battery::Unified { .. } => self.index_of(feature::UNIFIED_BATTERY),
            Battery::Voltage { .. } => self.index_of(feature::BATTERY_VOLTAGE),
        };
        self.out.push_back(long(self.slot, fi, 0x00, &[]));
    }

    #[cfg(test)]
    /// The function bytes of every request to feature `id` since `from`.
    pub fn calls_since(&self, from: usize, id: u16) -> Vec<u8> {
        let fi = self.index_of(id);
        self.requests[from..].iter().filter(|r| r[1] == self.slot && r[2] == fi).map(|r| r[3] & 0xF0).collect()
    }

    fn answer(&mut self, r: &[u8]) {
        let (index, fi, function) = (r[1], r[2], r[3]);
        let params = r[4..].to_vec();
        if self.chatter {
            self.out.push_back(long(index, fi, (function & 0xF0) | 0x0A, &[0xEE; 4]));
        }
        let refuse10 = |code: u8| vec![p::SHORT, index, p::ERROR_10, fi, function, code, 0];
        if index != self.slot {
            let sensorless = (index == p::DIRECT && self.empty_direct) || Some(index) == self.keyboard;
            if sensorless {
                // HID++ 2.0, with firmware but no DPI feature.
                let found = if fi == 0 && function & 0xF0 == 0 && params[..2] == feature::FIRMWARE.to_be_bytes() {
                    1
                } else {
                    0
                };
                self.out.push_back(reply(index, fi, function, &[found, 0, 0]));
            } else if crate::mice::hidpp::models::is_receiver(self.product_id) {
                // The receiver itself on 0xFF; an empty slot otherwise.
                self.out.push_back(refuse10(if index == p::DIRECT { 0x01 } else { 0x08 }));
            }
            return;
        }
        if !self.online {
            self.out.push_back(refuse10(0x09));
            return;
        }
        let features = self.features();
        let Some(&id) = features.get(fi as usize) else {
            self.out.push_back(refuse20(index, fi, function, 0x06));
            return;
        };
        let at = |i: usize| params.get(i).copied().unwrap_or(0);
        let answer: Result<Vec<u8>, u8> = match (id, function & 0xF0) {
            (feature::ROOT, 0x00) => {
                let want = u16::from_be_bytes([at(0), at(1)]);
                Ok(vec![features.iter().position(|f| *f == want).unwrap_or(0) as u8, 0, 0])
            }
            (feature::ROOT, 0x10) => Ok(vec![4, 2, at(2)]),
            (feature::FIRMWARE, 0x00) => {
                let [w0, w1] = self.wireless_id.to_be_bytes();
                let [u0, u1] = self.usb_id.to_be_bytes();
                let mut v = vec![3];
                v.extend_from_slice(&self.unit);
                v.extend_from_slice(&[0, 0x0C, w0, w1, u0, u1, 0, 0]);
                Ok(v)
            }
            (feature::FIRMWARE, 0x10) => Ok(match at(0) {
                0 => vec![0, b'M', b'P', b'M', 0x25, 0x01, 0x00, 0x18],
                1 => vec![1, b'B', b'O', b'T', 0x01, 0x02, 0x00, 0x05],
                _ => vec![2, 0, 0, 0, 0x01, 0x00, 0x00, 0x00],
            }),
            (feature::NAME, 0x00) => Ok(vec![self.name.len() as u8]),
            (feature::NAME, 0x10) => Ok(self.name.as_bytes().iter().skip(at(0) as usize).take(16).copied().collect()),
            (feature::UNIFIED_BATTERY, 0x10) => match self.battery {
                Battery::Unified { percent, status } => Ok(vec![percent, 0x08, status, (status != 0) as u8]),
                _ => Err(0x07),
            },
            (feature::BATTERY_VOLTAGE, 0x00) => match self.battery {
                Battery::Voltage { mv, flags } => {
                    let [hi, lo] = mv.to_be_bytes();
                    Ok(vec![hi, lo, flags])
                }
                _ => Err(0x07),
            },
            // 100 to 25,600 in steps of 50, as a range.
            (feature::ADJUSTABLE_DPI, 0x10) => Ok(vec![0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00, 0, 0]),
            (feature::ADJUSTABLE_DPI, 0x20) => {
                let [hi, lo] = self.live_dpi.to_be_bytes();
                Ok(vec![0, hi, lo, 0x03, 0x20])
            }
            // 100 to 32,000 in steps of 50, on page 0.
            (feature::EXTENDED_DPI, 0x20) => {
                let mut v = vec![0, 0, at(2)];
                if at(2) == 0 {
                    v.extend_from_slice(&[0x00, 0x64, 0xE0, 0x32, 0x7D, 0x00]);
                }
                v.resize(16, 0);
                Ok(v)
            }
            (feature::EXTENDED_DPI, 0x50) => {
                let [hi, lo] = self.live_dpi.to_be_bytes();
                Ok(vec![0, hi, lo, 0, 0, hi, lo, 0, 0, 2])
            }
            // 1, 2, 4 and 8 ms.
            (feature::REPORT_RATE, 0x00) => Ok(vec![0x8B]),
            (feature::REPORT_RATE, 0x10) => Ok(vec![1]),
            // The cable's list stops at 1,000 Hz; LIGHTSPEED's runs to 8,000.
            (feature::EXTENDED_REPORT_RATE, 0x10) => Ok(if at(0) == 0 { vec![0x00, 0x0F] } else { vec![0x00, 0x7F] }),
            (feature::EXTENDED_REPORT_RATE, 0x20) => Ok(vec![self.rate]),
            (feature::ONBOARD_PROFILES, f) => self.onboard(f, &params),
            _ => Err(0x07),
        };
        self.out.push_back(match answer {
            Ok(data) => reply(index, fi, function, &data),
            Err(code) => refuse20(index, fi, function, code),
        });
    }

    fn onboard(&mut self, function: u8, params: &[u8]) -> Result<Vec<u8>, u8> {
        let at = |i: usize| params.get(i).copied().unwrap_or(0);
        let sector = u16::from_be_bytes([at(0), at(1)]);
        match function {
            func::INFO => Ok(self.info.clone()),
            func::GET_MODE => Ok(vec![self.mode]),
            func::GET_CURRENT => Ok(self.current.to_be_bytes().to_vec()),
            func::SET_CURRENT if sector != 0 && self.sectors.contains_key(&sector) => {
                self.current = sector;
                // The mouse runs the profile from its default stage.
                let profile = ob::decode(&self.sectors[&sector], self.info[1]);
                let default = profile.default_index.unwrap_or(0) as usize;
                if let Some(stage) = profile.stages.get(default) {
                    self.live_dpi = stage.x as u16;
                }
                Ok(vec![])
            }
            func::READ => {
                let offset = u16::from_be_bytes([at(2), at(3)]) as usize;
                let bytes = self.sectors.get(&sector).ok_or(0x02)?;
                // memoryRead refuses a read past the sector's end.
                bytes.get(offset..offset + ob::CHUNK).map(<[u8]>::to_vec).ok_or(0x02)
            }
            func::ADDR_WRITE => {
                self.writing = Some((sector, u16::from_be_bytes([at(4), at(5)]) as usize, Vec::new()));
                Ok(vec![])
            }
            func::WRITE => {
                let (_, _, buf) = self.writing.as_mut().ok_or(0x02)?;
                buf.extend_from_slice(&params[..ob::CHUNK]);
                Ok(vec![])
            }
            func::WRITE_END => {
                let (sector, len, buf) = self.writing.take().ok_or(0x02)?;
                let data = buf.get(..len).ok_or(0x02)?.to_vec();
                // The mouse checks the CRC before it commits.
                if !ob::crc_ok(&data) {
                    return Err(0x02);
                }
                if !self.forget_writes {
                    self.sectors.insert(sector, data);
                }
                Ok(vec![])
            }
            _ => Err(0x02),
        }
    }
}

/// A long report from device `index`: feature index, function byte, then `data`.
fn long(index: u8, fi: u8, function: u8, data: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; p::LONG_LEN];
    r[0] = p::LONG;
    r[1] = index;
    r[2] = fi;
    r[3] = function;
    r[4..4 + data.len()].copy_from_slice(data);
    r
}

fn reply(index: u8, fi: u8, function: u8, data: &[u8]) -> Vec<u8> {
    long(index, fi, function, data)
}

fn refuse20(index: u8, fi: u8, function: u8, code: u8) -> Vec<u8> {
    long(index, p::ERROR_20, fi, &[function, code])
}

/// The simulated receiver (or cable), as the driver sees it.
pub struct SimLink(pub Arc<Mutex<SimMouse>>);

impl Link for SimLink {
    fn write(&mut self, report: &[u8]) -> Result<()> {
        let mut m = self.0.lock();
        if m.unplugged {
            bail!("the device is not connected");
        }
        let expected = if report[0] == p::SHORT { p::SHORT_LEN } else { p::LONG_LEN };
        assert_eq!(report.len(), expected, "the app sent a report of the wrong length: {report:02X?}");
        m.requests.push(report.to_vec());
        m.answer(report);
        Ok(())
    }

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

/// Finds the simulated mouse while it's plugged in.
pub struct SimFinder {
    pub mouse: Arc<Mutex<SimMouse>>,
    last: Option<u8>,
}

impl SimFinder {
    pub fn new(mouse: Arc<Mutex<SimMouse>>) -> Self {
        Self { mouse, last: None }
    }
}

impl Finder for SimFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        if self.mouse.lock().unplugged {
            bail!("no Logitech receiver found");
        }
        let opened = Opened {
            link: Box::new(SimLink(self.mouse.clone())),
            long_only: false,
            product_id: self.mouse.lock().product_id,
            #[cfg(windows)]
            port: None,
        };
        connect(opened, board, &mut self.last)
    }

    fn count(&mut self) -> usize {
        1
    }
}

/// Simulated Superlights beside the real mice, in a dev build: `HRTZ_SIM=superlight` (or
/// `superlight2`, or both, with Zaopin ones too, comma-separated). Their keys start `sim/`.
pub struct SimFamily {
    mice: Vec<(String, Arc<Mutex<SimMouse>>)>,
}

impl SimFamily {
    pub fn from_env() -> Option<Self> {
        let names = std::env::var("HRTZ_SIM").ok()?;
        let mice: Vec<_> = names
            .split(',')
            .filter_map(|n| {
                let m = match n.trim() {
                    "superlight" => SimMouse::superlight(),
                    "superlight2" => SimMouse::superlight2(),
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
        Box::new(DevSimFinder(SimFinder::new(mouse)))
    }
}

/// The simulated mouse under a `sim/` key.
struct DevSimFinder(SimFinder);

impl Finder for DevSimFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        let d = self.0.find(board)?;
        board.set(|s| s.key = s.key.as_ref().map(|k| k.replacen("hidpp/", "sim/", 1)));
        Ok(d)
    }

    fn count(&mut self) -> usize {
        self.0.count()
    }
}
