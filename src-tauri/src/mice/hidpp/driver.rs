// Ported from OpenMouse's mouse-protocol (https://github.com/OpenMouse-Project/mouse-protocol),
// commit c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/logitech/hidpp.ts (the request and
// reply matching, finding the device index, identity, name, firmware, battery, DPI and report-rate
// reads, and reading and writing profile sectors). Translated to Rust and modified for hrtz,
// 2026-10-18. Copyright (C) the OpenMouse contributors. AGPL-3.0-or-later.

//! A connected Logitech mouse, on its LIGHTSPEED receiver or its cable. HID++ asks the mouse for
//! each feature by number, then talks to the index it keeps it at. Settings live in the mouse's
//! onboard profile (`onboard`), which hrtz reads and rewrites: never host mode, whose settings are
//! lost at power-off (PLAN-mice.md, section 7).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use hidapi::{HidApi, HidDevice};

use super::backup;
use super::buttons;
use super::models;
use super::onboard::{self as ob, Entry, Info, Plan, Stage};
use super::protocol::{self as p, Charge, Refusal, feature};
use crate::mice::model::{Capabilities, DpiCaps, DpiStage, PollingCaps, Settings};
use crate::mice::{Board, DeviceEvent, DeviceState, Driver, Family, Finder, Follow, Link, Reading};

const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
/// An empty slot is answered at once by the receiver; a quiet one isn't waited on long.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);
/// Writing a sector erases it first.
const WRITE_TIMEOUT: Duration = Duration::from_millis(3000);
/// How long a receiver with no mouse hrtz can run is left alone before it's asked again.
const NO_MOUSE_PAUSE: Duration = Duration::from_secs(30);
/// A receiver whose mouse didn't answer (asleep, most likely) is asked again once a mouse is used
/// on its port, or after this, should raw input not be followed.
const QUIET_RETRY: Duration = Duration::from_secs(30);
/// What a stage no profile holds shows, should the stages grow: the vendor's five.
const LADDER: [u32; ob::STAGE_SLOTS] = [800, 1200, 1600, 2400, 3200];
/// The Superlights have no light for a DPI stage.
const NO_LIGHT: &str = "#FFFFFF";
/// The first Superlight refused a stage's lift-off byte on a format 7 profile (OpenMouse).
const NO_LOD_WRITES: [u16; 1] = [0xC094];

/// Logitech's family: the receivers and cables `models` claims.
pub struct Logitech;

impl Family for Logitech {
    /// One connection per HID++ interface, by its long-report collection's path.
    fn connections(&self, devices: &[hidapi::DeviceInfo]) -> Vec<String> {
        devices.iter().filter(|d| is_hidpp(d, p::USAGE_LONG)).map(|d| d.path().to_string_lossy().into_owned()).collect()
    }

    fn finder(&self, id: &str) -> Box<dyn Finder> {
        Box::new(HidppFinder {
            api: None,
            path: id.to_string(),
            last_index: None,
            no_mouse_until: None,
            quiet_since: None,
        })
    }
}

pub(super) fn is_hidpp(d: &hidapi::DeviceInfo, usage: u16) -> bool {
    d.vendor_id() == p::VENDOR_ID
        && models::claims(d.product_id())
        && d.usage_page() == p::USAGE_PAGE
        && d.usage() == usage
}

/// Windows names each top-level collection of an interface with `&ColNN`, and its instance after
/// the interface's own (`…&mi_02&col02#8&2d9b0a4f&0&0001#{…}`). Without both, the short and long
/// collections of one receiver read the same.
pub(super) fn interface_of(path: &str) -> String {
    let path = path.to_ascii_lowercase();
    let mut parts: Vec<String> = path.split('#').map(str::to_string).collect();
    if parts.len() >= 3 {
        if let Some(at) = parts[1].find("&col") {
            let end = parts[1][at + 1..].find('&').map_or(parts[1].len(), |e| at + 1 + e);
            parts[1].replace_range(at..end, "");
        }
        if let Some((instance, _)) = parts[2].rsplit_once('&') {
            parts[2] = instance.to_string();
        }
    }
    parts.join("#")
}

/// One Logitech receiver, or a mouse on its cable: its HID++ interface at `path` (the long
/// collection), with the short one beside it.
pub struct HidppFinder {
    api: Option<HidApi>,
    path: String,
    /// Where the mouse answered last time, asked first.
    last_index: Option<u8>,
    /// Nothing here was a mouse: not asked again before this.
    no_mouse_until: Option<Instant>,
    /// When the mouse last didn't answer.
    quiet_since: Option<Instant>,
}

impl HidppFinder {
    fn api(&mut self) -> Result<&mut HidApi> {
        if self.api.is_none() {
            self.api = Some(HidApi::new()?);
        }
        Ok(self.api.as_mut().unwrap())
    }
}

impl Finder for HidppFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        if self.no_mouse_until.is_some_and(|t| Instant::now() < t) {
            return Err(NotAMouse.into());
        }
        let path = self.path.clone();
        #[cfg(windows)]
        let port = crate::usb::port_of(&path);
        // Asking a sleeping mouse can wake it: wait until it's used, as a met one does.
        if let Some(t) = self.quiet_since {
            #[cfg(windows)]
            let used = port.as_ref().is_some_and(|p| crate::mice::input::since(&p.path, t));
            #[cfg(not(windows))]
            let used = false;
            if !used && t.elapsed() < QUIET_RETRY {
                return Err(crate::mice::Waiting.into());
            }
        }
        let api = self.api()?;
        api.refresh_devices()?;
        let long = api
            .device_list()
            .find(|d| is_hidpp(d, p::USAGE_LONG) && d.path().to_string_lossy() == *path)
            .ok_or_else(|| anyhow!("no Logitech receiver found"))?
            .clone();
        let interface = interface_of(&path);
        let short = api
            .device_list()
            .find(|d| {
                is_hidpp(d, p::USAGE_SHORT)
                    && d.product_id() == long.product_id()
                    && interface_of(&d.path().to_string_lossy()) == interface
            })
            .cloned();
        let link = HidppLink {
            long: long.open_device(api).context("open the HID++ long collection")?,
            short: match short {
                Some(s) => Some(s.open_device(api).context("open the HID++ short collection")?),
                None => None,
            },
        };
        let opened = Opened {
            family: "hidpp",
            long_only: link.short.is_none(),
            link: Box::new(link),
            product_id: long.product_id(),
            #[cfg(windows)]
            port,
        };
        match connect(opened, board, &mut self.last_index) {
            Err(e) if e.is::<NotAMouse>() => {
                self.no_mouse_until = Some(Instant::now() + NO_MOUSE_PAUSE);
                Err(e)
            }
            Err(e) => {
                self.quiet_since = Some(Instant::now());
                Err(e)
            }
            Ok(d) => {
                self.quiet_since = None;
                Ok(d)
            }
        }
    }

    fn count(&mut self) -> usize {
        let Ok(api) = self.api() else { return 1 };
        if api.refresh_devices().is_err() {
            return 1;
        }
        api.device_list().filter(|d| is_hidpp(d, p::USAGE_LONG)).count()
    }
}

/// Something on this receiver speaks HID++ but has no sensor: a keyboard, say.
#[derive(Debug)]
struct NotAMouse;

impl std::fmt::Display for NotAMouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Nothing on this Logitech receiver is a mouse.")
    }
}

impl std::error::Error for NotAMouse {}

/// One HID++ interface, as Windows opens it: its short-report collection and its long one.
/// Requests go out on their report's collection; answers can come on either.
pub(super) struct HidppLink {
    pub short: Option<HidDevice>,
    pub long: HidDevice,
}

impl Link for HidppLink {
    fn write(&mut self, report: &[u8]) -> Result<()> {
        let dev = match (report[0], &self.short) {
            (p::SHORT, Some(short)) => short,
            _ => &self.long,
        };
        dev.write(report)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        let Some(short) = &self.short else {
            return Ok(self.long.read_timeout(buf, timeout.as_millis().max(1) as i32)?);
        };
        let deadline = Instant::now() + timeout;
        loop {
            let n = short.read_timeout(buf, 0)?;
            if n > 0 {
                return Ok(n);
            }
            let left = deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(5));
            let n = self.long.read_timeout(buf, left.as_millis() as i32)?;
            if n > 0 || Instant::now() >= deadline {
                return Ok(n);
            }
        }
    }
}

/// A receiver or a mouse on its cable, opened.
pub(crate) struct Opened {
    /// The key's first part: `hidpp`, or `sim` for a simulated mouse in a dev build, so it never
    /// takes over what a real one keeps.
    pub family: &'static str,
    pub link: Box<dyn Link>,
    /// No short-report collection: every request goes long.
    pub long_only: bool,
    /// The USB product ID of what's plugged in: a receiver's, or the mouse's own.
    pub product_id: u16,
    #[cfg(windows)]
    pub port: Option<crate::usb::UsbPort>,
}

/// Why a request got no answer.
enum Fail {
    Refused(Refusal),
    /// Nothing answered in time.
    Silent,
    /// The connection is gone.
    Lost(anyhow::Error),
}

/// Requests and their answers, on one device index.
struct Hidpp {
    link: Box<dyn Link>,
    index: u8,
    long_only: bool,
    lost: bool,
    /// Feature numbers, to the index the mouse keeps each at (0 where it has none).
    features: HashMap<u16, u8>,
    /// What the mouse sent by itself, kept for `changes`.
    events: Vec<Vec<u8>>,
}

impl Hidpp {
    fn exchange(&mut self, feature_index: u8, function: u8, params: &[u8], timeout: Duration) -> Result<Vec<u8>, Fail> {
        let request = p::request(self.index, feature_index, function, params, self.long_only);
        self.write(&request).map_err(Fail::Lost)?;
        let deadline = Instant::now() + timeout;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Some(r) = self.read(left).map_err(Fail::Lost)? else { break };
            if p::answers(&r, self.index, feature_index, function) {
                return Ok(r[4..].to_vec());
            }
            if let Some(why) = p::refusal_for(&r, self.index, feature_index, function) {
                return Err(Fail::Refused(why));
            }
            // A status the mouse sent between our request and its answer; anything else is
            // someone else's (G HUB's), or another slot's.
            if p::is_event(&r, self.index) {
                self.events.push(r);
            }
        }
        Err(Fail::Silent)
    }

    fn request(&mut self, feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.request_for(feature_index, function, params, REPLY_TIMEOUT)
    }

    fn request_for(&mut self, feature_index: u8, function: u8, params: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        self.exchange(feature_index, function, params, timeout).map_err(|f| match f {
            Fail::Refused(why) => anyhow!("{why}"),
            Fail::Silent => anyhow!("The mouse did not answer. Move it, or close Logitech G HUB, then try again."),
            Fail::Lost(e) => e,
        })
    }

    /// Where the mouse keeps feature `id`; 0 when it hasn't it.
    fn feature(&mut self, id: u16) -> Result<u8> {
        if let Some(i) = self.features.get(&id) {
            return Ok(*i);
        }
        let index = match self.exchange(0x00, 0x00, &id.to_be_bytes(), REPLY_TIMEOUT) {
            Ok(r) => r.first().copied().unwrap_or(0),
            Err(Fail::Refused(Refusal::V20(_))) => 0,
            Err(Fail::Refused(why)) => bail!("{why}"),
            Err(Fail::Silent) => bail!("The mouse did not answer. Move it, then try again."),
            Err(Fail::Lost(e)) => return Err(e),
        };
        self.features.insert(id, index);
        Ok(index)
    }

    /// Which feature the mouse keeps at `index`, among those asked for.
    fn feature_at(&self, index: u8) -> Option<u16> {
        self.features.iter().find(|(_, i)| **i == index && index != 0).map(|(id, _)| *id)
    }

    fn write(&mut self, report: &[u8]) -> Result<()> {
        if self.lost {
            bail!("not connected");
        }
        if let Err(e) = self.link.write(report) {
            self.lost = true;
            bail!("Lost the receiver: {e}");
        }
        Ok(())
    }

    /// The next report, whatever it is; `None` when nothing came in `timeout`.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        if self.lost {
            bail!("not connected");
        }
        let mut buf = [0u8; p::LONG_LEN + 1];
        match self.link.read(&mut buf, timeout) {
            Ok(0) => Ok(None),
            Ok(n) => Ok(Some(buf[..n.min(p::LONG_LEN)].to_vec())),
            Err(e) => {
                self.lost = true;
                bail!("Lost the receiver: {e}")
            }
        }
    }
}

/// What answering on one index says.
enum Probe {
    Mouse,
    /// HID++ 2.0, but no sensor: a keyboard, or the first Superlight's empty 0xFF.
    NoSensor,
    /// A mouse the receiver can't reach now.
    Asleep,
    Absent,
}

/// Whether the mouse is on index `i`: it answers HID++ 2.0 (a reply, or a 2.0 refusal; a receiver
/// answers empty slots with 1.0 refusals) and has a DPI feature, as keyboards don't.
fn probe(t: &mut Hidpp, i: u8) -> Result<Probe> {
    t.index = i;
    t.features.clear();
    match t.exchange(0x00, 0x00, &feature::FIRMWARE.to_be_bytes(), PROBE_TIMEOUT) {
        Ok(_) | Err(Fail::Refused(Refusal::V20(_))) => {}
        Err(Fail::Refused(why)) if why.unreachable() => return Ok(Probe::Asleep),
        Err(Fail::Refused(_) | Fail::Silent) => return Ok(Probe::Absent),
        Err(Fail::Lost(e)) => return Err(e),
    }
    for id in [feature::EXTENDED_DPI, feature::ADJUSTABLE_DPI] {
        match t.feature(id) {
            Ok(0) => {}
            Ok(_) => return Ok(Probe::Mouse),
            Err(e) if t.lost => return Err(e),
            Err(_) => {}
        }
    }
    Ok(Probe::NoSensor)
}

/// Find which index the mouse answers on, `last` first. Every one gets the sensor check: the first
/// Superlight can answer on 0xFF with nothing behind it, the mouse being on slot 1.
fn find_index(t: &mut Hidpp, receiver: bool, last: Option<u8>) -> Result<u8> {
    let order = last.into_iter().chain(p::index_candidates(receiver).iter().copied().filter(|i| Some(*i) != last));
    let (mut answered, mut asleep) = (false, false);
    for i in order.collect::<Vec<_>>() {
        match probe(t, i)? {
            Probe::Mouse => return Ok(i),
            Probe::NoSensor => answered = true,
            Probe::Asleep => {
                asleep = true;
                // The mouse met here before is asleep: the other slots aren't it.
                if Some(i) == last {
                    break;
                }
            }
            Probe::Absent => {}
        }
    }
    if asleep {
        bail!("The mouse is asleep. Move it to wake it.");
    }
    if answered {
        return Err(NotAMouse.into());
    }
    bail!("The mouse did not answer. Move it, or close Logitech G HUB, then try again.")
}

/// Who the mouse is, from 0x0003: its unit ID, and its product ID on each link it has.
#[derive(Default)]
struct Identity {
    unit: Option<String>,
    wireless: Option<u16>,
    usb: Option<u16>,
    /// The extended model ID: the colour, for a model that comes in several (0 is the first).
    variant: Option<u8>,
}

fn identity(t: &mut Hidpp) -> Result<Identity> {
    let fi = t.feature(feature::FIRMWARE)?;
    if fi == 0 {
        return Ok(Identity::default());
    }
    let r = t.request(fi, 0x00, &[])?;
    if r.len() < 13 {
        return Ok(Identity::default());
    }
    let unit: String = r[1..5].iter().map(|b| format!("{b:02x}")).collect();
    let (flags, ids) = (r[6], &r[7..13]);
    let mut id =
        Identity { unit: (unit != "00000000").then_some(unit), variant: r.get(13).copied(), ..Default::default() };
    let mut at = 0;
    // Bluetooth, Bluetooth LE, LIGHTSPEED, USB: each the link has takes the next two bytes.
    for flag in [0x01, 0x02, 0x04, 0x08] {
        if flags & flag != 0 && at + 2 <= ids.len() {
            let pid = u16::from_be_bytes([ids[at], ids[at + 1]]);
            match flag {
                0x04 => id.wireless = Some(pid),
                0x08 => id.usb = Some(pid),
                _ => {}
            }
            at += 2;
        }
    }
    Ok(id)
}

/// From 0x0005.
fn device_name(t: &mut Hidpp) -> Result<Option<String>> {
    let fi = t.feature(feature::NAME)?;
    if fi == 0 {
        return Ok(None);
    }
    let len = t.request(fi, 0x00, &[])?.first().copied().unwrap_or(0) as usize;
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        let r = t.request(fi, 0x10, &[bytes.len() as u8])?;
        let take = (len - bytes.len()).min(r.len());
        if take == 0 {
            break;
        }
        bytes.extend_from_slice(&r[..take]);
    }
    let name = String::from_utf8_lossy(&bytes).trim_end_matches('\0').trim().to_string();
    Ok((!name.is_empty()).then_some(name))
}

/// The main application's version, from 0x0003: "MPM 25.01".
fn firmware(t: &mut Hidpp) -> Result<Option<String>> {
    let fi = t.feature(feature::FIRMWARE)?;
    if fi == 0 {
        return Ok(None);
    }
    let count = t.request(fi, 0x00, &[])?.first().copied().unwrap_or(0);
    let mut first = None;
    for i in 0..count.min(8) {
        let r = t.request(fi, 0x10, &[i])?;
        if r.len() < 6 {
            continue;
        }
        let prefix: String = r[1..4].iter().filter(|b| **b != 0).map(|b| *b as char).collect();
        let version = format!("{prefix} {:02X}.{:02X}", r[4], r[5]).trim().to_string();
        // Type 0 is the main application; the others are the bootloader, the hardware, …
        if r[0] == 0 {
            return Ok(Some(version));
        }
        first.get_or_insert(version);
    }
    Ok(first)
}

/// Meet the mouse: find its index, who it is and how it's connected.
pub(crate) fn connect(opened: Opened, board: &Board, last: &mut Option<u8>) -> Result<Box<dyn Driver>> {
    let receiver = models::is_receiver(opened.product_id);
    let mut t = Hidpp {
        link: opened.link,
        index: p::DIRECT,
        long_only: opened.long_only,
        lost: false,
        features: HashMap::new(),
        events: Vec::new(),
    };
    let index = find_index(&mut t, receiver, *last)?;
    *last = Some(index);
    let id = identity(&mut t)?;
    let name = device_name(&mut t).ok().flatten();
    // On its cable, a mouse that doesn't say is the product plugged in.
    let usb = id.usb.or((!receiver).then_some(opened.product_id));
    // The link in use is the one whose ID is what's plugged in; a receiver's is never the mouse's.
    let wired = match (id.usb, id.wireless) {
        (Some(u), _) if u == opened.product_id => true,
        (_, Some(w)) if w == opened.product_id => false,
        _ => index == p::DIRECT,
    };
    let model = usb.or(id.wireless).unwrap_or(opened.product_id);
    let key = format!("{}/{model:04x}/{}", opened.family, id.unit.as_deref().unwrap_or("0"));
    let model = usb.and_then(models::name).map(str::to_string).or(name).unwrap_or_else(|| "Logitech mouse".into());
    let mut battery = false;
    for f in BATTERIES {
        battery |= t.feature(f)? != 0;
    }
    // Looked up so its word that the mouse linked up again is known for what it is.
    t.feature(feature::WIRELESS_STATUS)?;
    #[cfg(windows)]
    let port = opened.port.as_ref().map(|p| p.path.clone());
    #[cfg(not(windows))]
    let port: Option<String> = None;
    // Through the receiver, it's told awake by being used, where its port is known.
    let wake_by_input = !wired && port.is_some();
    board.set(|s| {
        *s = DeviceState {
            key: Some(key),
            connected: true,
            model: Some(model),
            variant: id.variant,
            capabilities: Some(Capabilities {
                wired,
                battery,
                wake_by_input,
                rated_hours: usb.and_then(models::rated_hours),
                ..Default::default()
            }),
            #[cfg(windows)]
            port: opened.port,
            ..Default::default()
        }
    });
    board.publish(None);
    Ok(Box::new(Connected {
        t,
        wired,
        usb,
        port,
        asleep_since: None,
        stage_index: None,
        onboard: None,
        dpi_options: None,
        rates: None,
        mode_read: None,
    }))
}

/// The battery features, the most direct first: 0x1004 and 0x1000 give a percentage, the others
/// only a voltage to estimate it from.
const BATTERIES: [u16; 4] =
    [feature::UNIFIED_BATTERY, feature::BATTERY_STATUS, feature::BATTERY_VOLTAGE, feature::ADC_MEASUREMENT];

/// The profile in use, as read.
struct Onboard {
    feature: u8,
    info: Info,
    directory: Vec<Entry>,
    /// The sector of the profile in use, and its bytes.
    sector: u16,
    bytes: Vec<u8>,
}

impl Onboard {
    fn profile(&self) -> ob::Profile {
        ob::decode(&self.bytes, self.info.format)
    }

    fn index(&self) -> Option<u8> {
        self.directory.iter().position(|e| e.sector == self.sector).map(|i| i as u8)
    }
}

/// What the mouse runs now: the DPI, and which stage that is, where it says.
#[derive(Clone, Copy, Default)]
struct Live {
    dpi: Option<u32>,
    stage: Option<u8>,
}

/// A Logitech mouse, met.
struct Connected {
    t: Hidpp,
    wired: bool,
    /// Its USB product ID, which says which model it is.
    usb: Option<u16>,
    /// The USB port its receiver is on, where raw input shows it used.
    port: Option<String>,
    /// When it was found asleep: it isn't asked again until it's used after this.
    asleep_since: Option<Instant>,
    /// Whether it answers 0x8100's DPI index functions; `None` until asked.
    stage_index: Option<bool>,
    onboard: Option<Onboard>,
    /// What the sensor offers, read once.
    dpi_options: Option<Vec<u32>>,
    /// The rates this link carries, read once.
    rates: Option<Vec<u32>>,
    /// When it was last asked whether software drives it (host mode).
    mode_read: Option<Instant>,
}

/// How often a connected mouse is asked whether software has taken it over: G HUB does when it
/// starts, or the mouse reconnects, with its On-board memory mode off.
const MODE_EVERY: Duration = Duration::from_secs(5);

impl Connected {
    /// Each profile's name: the name bytes alone of each, the profile in use from what's read.
    fn read_names(&mut self) -> Result<Option<Vec<Option<String>>>> {
        let Some(o) = &self.onboard else { return Ok(None) };
        let (fi, sector, bytes, entries) = (o.feature, o.sector, o.bytes.clone(), o.directory.clone());
        let mut names = Vec::with_capacity(entries.len());
        for e in entries {
            let region = if e.sector == sector {
                bytes.get(ob::NAME_AT..ob::NAME_AT + ob::NAME_LEN).map(<[u8]>::to_vec).unwrap_or_default()
            } else {
                let mut r = Vec::with_capacity(ob::NAME_LEN);
                for at in (ob::NAME_AT..ob::NAME_AT + ob::NAME_LEN).step_by(ob::CHUNK) {
                    let [s_hi, s_lo] = e.sector.to_be_bytes();
                    let [a_hi, a_lo] = (at as u16).to_be_bytes();
                    let chunk = self.t.request(fi, ob::func::READ, &[s_hi, s_lo, a_hi, a_lo])?;
                    r.extend(chunk.iter().take(ob::CHUNK));
                }
                r
            };
            names.push(ob::decode_name(&region));
        }
        Ok(Some(names))
    }

    /// Whether software drives the mouse live (host mode) instead of its onboard profile, posted
    /// for the UI. Returns whether that changed.
    fn read_mode(&mut self, board: &Board) -> Result<bool> {
        self.mode_read = Some(Instant::now());
        let Some(fi) = self.onboard.as_ref().map(|o| o.feature) else { return Ok(false) };
        let host = self.t.request(fi, ob::func::GET_MODE, &[])?.first() == Some(&ob::MODE_HOST);
        if board.read().host_mode == host {
            return Ok(false);
        }
        board.set(|s| s.host_mode = host);
        Ok(true)
    }

    /// memoryRead hands back 16 bytes and refuses to run past the sector's end, so a length that
    /// isn't a multiple of 16 has its tail read from `len - 16`, over the chunk before it.
    fn read_sector(&mut self, fi: u8, sector: u16, len: usize) -> Result<Vec<u8>> {
        let mut out = vec![0u8; len];
        let mut offsets: Vec<usize> = (0..len / ob::CHUNK).map(|i| i * ob::CHUNK).collect();
        if !len.is_multiple_of(ob::CHUNK) {
            offsets.push(len.saturating_sub(ob::CHUNK));
        }
        for at in offsets {
            let [s_hi, s_lo] = sector.to_be_bytes();
            let [o_hi, o_lo] = (at as u16).to_be_bytes();
            let r = self.t.request(fi, ob::func::READ, &[s_hi, s_lo, o_hi, o_lo])?;
            let n = ob::CHUNK.min(len - at);
            if r.len() < n {
                bail!("The mouse sent part of its profile.");
            }
            out[at..at + n].copy_from_slice(&r[..n]);
        }
        Ok(out)
    }

    /// memoryAddrWrite says where and how much, memoryWrite sends 16 bytes at a time, and
    /// memoryWriteEnd commits, checking the CRC.
    fn write_sector(&mut self, fi: u8, sector: u16, bytes: &[u8]) -> Result<()> {
        let [s_hi, s_lo] = sector.to_be_bytes();
        let [l_hi, l_lo] = (bytes.len() as u16).to_be_bytes();
        self.t.request_for(fi, ob::func::ADDR_WRITE, &[s_hi, s_lo, 0, 0, l_hi, l_lo], WRITE_TIMEOUT)?;
        for chunk in bytes.chunks(ob::CHUNK) {
            let mut c = [0xFF; ob::CHUNK];
            c[..chunk.len()].copy_from_slice(chunk);
            self.t.request_for(fi, ob::func::WRITE, &c, WRITE_TIMEOUT)?;
        }
        self.t.request_for(fi, ob::func::WRITE_END, &[], WRITE_TIMEOUT)?;
        Ok(())
    }

    /// The profile in use: the directory, which one, and its sector.
    fn read_onboard(&mut self) -> Result<Option<Onboard>> {
        let fi = self.t.feature(feature::ONBOARD_PROFILES)?;
        if fi == 0 {
            return Ok(None);
        }
        let info = Info::parse(&self.t.request(fi, ob::func::INFO, &[])?);
        if info.format == 0 {
            return Ok(None);
        }
        // Sixteen entries are plenty, and a quarter of the reads.
        let directory = ob::directory(&self.read_sector(fi, 0, info.sector_len().min(64))?);
        let current = self.current_sector(fi)?;
        // In host mode (G HUB's doing) the mouse may report no profile; its first one is shown.
        let Some(sector) =
            directory.iter().map(|e| e.sector).find(|s| *s == current).or(directory.first().map(|e| e.sector))
        else {
            return Ok(None);
        };
        let bytes = self.read_sector(fi, sector, info.sector_len())?;
        Ok(Some(Onboard { feature: fi, info, directory, sector, bytes }))
    }

    fn current_sector(&mut self, fi: u8) -> Result<u16> {
        let r = self.t.request(fi, ob::func::GET_CURRENT, &[])?;
        Ok(u16::from_be_bytes([r.first().copied().unwrap_or(0), r.get(1).copied().unwrap_or(0)]))
    }

    /// The DPI the mouse runs now, and the stage, where it says (0x8100's getCurrentDpiIndex): a
    /// DPI alone can't tell two stages of the same value apart.
    fn live(&mut self) -> Result<Live> {
        Ok(Live { dpi: self.live_dpi()?, stage: self.current_stage()? })
    }

    fn current_stage(&mut self) -> Result<Option<u8>> {
        let Some(fi) = self.onboard.as_ref().map(|o| o.feature) else { return Ok(None) };
        if self.stage_index == Some(false) {
            return Ok(None);
        }
        match self.t.exchange(fi, ob::func::GET_DPI_INDEX, &[], REPLY_TIMEOUT) {
            Ok(r) => {
                self.stage_index = Some(true);
                Ok(r.first().copied())
            }
            Err(Fail::Refused(_)) => {
                self.stage_index = Some(false);
                Ok(None)
            }
            Err(Fail::Silent) => Ok(None),
            Err(Fail::Lost(e)) => Err(e),
        }
    }

    /// Make stage `i` the one the mouse runs now (0x8100's setCurrentDpiIndex): RAM only. A
    /// rewritten profile keeps the mouse on the stage slot it was on, whatever the default says.
    /// Returns whether the mouse took it.
    fn set_stage(&mut self, i: u8) -> Result<bool> {
        let Some(fi) = self.onboard.as_ref().map(|o| o.feature) else { return Ok(false) };
        if self.stage_index == Some(false) {
            return Ok(false);
        }
        match self.t.exchange(fi, ob::func::SET_DPI_INDEX, &[i], REPLY_TIMEOUT) {
            Ok(_) => Ok(true),
            Err(Fail::Refused(why)) => {
                eprintln!("the mouse refused to switch DPI stage: {why}");
                self.stage_index = Some(false);
                Ok(false)
            }
            Err(Fail::Silent) => Ok(false),
            Err(Fail::Lost(e)) => Err(e),
        }
    }

    /// The DPI the sensor runs now.
    fn live_dpi(&mut self) -> Result<Option<u32>> {
        let ext = self.t.feature(feature::EXTENDED_DPI)?;
        let r = if ext != 0 {
            self.t.request(ext, 0x50, &[])?
        } else {
            let legacy = self.t.feature(feature::ADJUSTABLE_DPI)?;
            if legacy == 0 {
                return Ok(None);
            }
            self.t.request(legacy, 0x20, &[0])?
        };
        Ok(r.get(1..3).map(|b| u16::from_be_bytes([b[0], b[1]]) as u32).filter(|d| *d > 0))
    }

    /// Every DPI the sensor offers.
    fn dpi_options(&mut self) -> Result<Vec<u32>> {
        if let Some(o) = &self.dpi_options {
            return Ok(o.clone());
        }
        let ext = self.t.feature(feature::EXTENDED_DPI)?;
        let bytes = if ext != 0 {
            let mut bytes = Vec::new();
            for page in 0..32u8 {
                let r = self.t.request(ext, 0x20, &[0, 0, page])?;
                bytes.extend_from_slice(r.get(3..).unwrap_or_default());
                if bytes.windows(2).any(|w| w == [0, 0]) {
                    break;
                }
            }
            bytes
        } else {
            match self.t.feature(feature::ADJUSTABLE_DPI)? {
                0 => Vec::new(),
                legacy => self.t.request(legacy, 0x10, &[0])?.get(1..).unwrap_or_default().to_vec(),
            }
        };
        let options = p::dpi_list(&bytes).unwrap_or_default();
        self.dpi_options = Some(options.clone());
        Ok(options)
    }

    /// The rates this link carries, as the mouse says. 0x8061 has a list per link (0 the cable, 1
    /// LIGHTSPEED); some firmware answers its function 0x10 with the cable's even for LIGHTSPEED
    /// and keeps the real list on function 0.
    fn live_rates(&mut self) -> Result<Vec<u32>> {
        if let Some(r) = &self.rates {
            return Ok(r.clone());
        }
        let ext = self.t.feature(feature::EXTENDED_REPORT_RATE)?;
        let rates = if ext != 0 {
            let link = if self.wired { 0 } else { 1 };
            let mask =
                |r: Vec<u8>| u16::from_be_bytes([r.first().copied().unwrap_or(0), r.get(1).copied().unwrap_or(0)]);
            let listed = p::rate_mask(mask(self.t.request(ext, 0x10, &[link])?));
            if link == 0 || listed.iter().any(|hz| *hz > 1000) {
                listed
            } else {
                match self.t.request(ext, 0x00, &[link]) {
                    Ok(r) if p::rate_mask(mask(r.clone())).iter().any(|hz| *hz > 1000) => p::rate_mask(mask(r)),
                    _ => listed,
                }
            }
        } else {
            match self.t.feature(feature::REPORT_RATE)? {
                0 => Vec::new(),
                legacy => p::rate_bitmap(self.t.request(legacy, 0x00, &[])?.first().copied().unwrap_or(0)),
            }
        };
        self.rates = Some(rates.clone());
        Ok(rates)
    }

    fn dpi_limits(&mut self, format: u8) -> Result<Option<ob::DpiLimits>> {
        let options = self.dpi_options()?;
        Ok(ob::dpi_limits_for_sensor(ob::dpi_limits(format), &options))
    }

    /// What the UI shows and may change, from the profile in use and what the mouse runs.
    fn show(&mut self, board: &Board, live: Live) -> Result<()> {
        let mut caps = board.read().capabilities.clone().unwrap_or_default();
        let Some(o) = &self.onboard else {
            board.set(|s| s.settings = Some(Settings::default()));
            return Ok(());
        };
        let (format, profile, index, count) = (o.info.format, o.profile(), o.index(), o.directory.len());
        let mut settings = Settings::default();
        caps.profiles = (count > 0).then_some(count.min(u8::MAX as usize) as u8);
        caps.backups = ob::writable(format);
        caps.factory_reset = ob::writable(format) && o.info.rom_count > 0;
        if ob::writable(format) {
            let (at, room) = buttons::table(format);
            let count = o.info.button_count.min(room as u8);
            caps.buttons = (count > 0).then(|| buttons::names(count));
            settings.buttons = (count > 0).then(|| {
                (0..count as usize).map(|i| buttons::decode(o.bytes.get(at + i * 4..).unwrap_or(&[]))).collect()
            });
            let limits = self.dpi_limits(format)?;
            let rates =
                ob::rates_offered(ob::rate_limits(format, self.usb_hex().as_deref()), self.wired, &self.live_rates()?);
            caps.dpi = limits.map(|l| DpiCaps {
                min: l.min,
                max: l.max,
                step: l.step,
                max_stages: l.max_stages,
                colors: false,
                light: false,
            });
            caps.polling = (!rates.is_empty()).then_some(PollingCaps { rates });
            settings.polling_hz = if self.wired && ob::v6(format) { profile.rate_wired } else { profile.rate_wireless };
            if let Some(l) = limits {
                let used: Vec<u32> = profile.stages.iter().map(|s| s.x).collect();
                let mut stages: Vec<DpiStage> =
                    used.iter().map(|d| DpiStage { dpi: *d, color: NO_LIGHT.into() }).collect();
                // The empty slots, should the stages grow: the vendor's five, less those in use.
                let mut spare = LADDER.iter().map(|v| ((*v).clamp(l.min, l.max) / l.step * l.step).max(l.min));
                while stages.len() < ob::STAGE_SLOTS {
                    let v = spare.find(|v| !used.contains(v)).unwrap_or(l.min);
                    stages.push(DpiStage { dpi: v, color: NO_LIGHT.into() });
                }
                let in_use = live
                    .stage
                    .map(|i| i as usize)
                    .filter(|i| *i < used.len())
                    .or_else(|| live.dpi.and_then(|d| used.iter().position(|u| *u == d)))
                    .or(profile.default_index.map(|i| i as usize).filter(|i| *i < used.len()))
                    .unwrap_or(0);
                settings.dpi_stage_count = Some(used.len() as u8);
                settings.dpi_stage = Some(in_use as u8);
                settings.stages = Some(stages);
            }
        }
        board.set(|s| {
            s.profile = index;
            s.settings = Some(settings);
            s.capabilities = Some(caps);
        });
        Ok(())
    }

    fn usb_hex(&self) -> Option<String> {
        self.usb.map(|u| format!("{u:04X}"))
    }

    /// The mouse switched profile, or stage, by itself: say which.
    fn follow_mouse(&mut self, board: &Board) -> Result<()> {
        let Some(fi) = self.onboard.as_ref().map(|o| o.feature) else { return Ok(()) };
        let (before, stage_before) = {
            let s = board.read();
            (s.profile, s.settings.as_ref().and_then(|x| x.dpi_stage))
        };
        let current = self.current_sector(fi)?;
        if self.onboard.as_ref().is_some_and(|o| o.sector != current && o.directory.iter().any(|e| e.sector == current))
        {
            let len = self.onboard.as_ref().map_or(255, |o| o.info.sector_len());
            let bytes = self.read_sector(fi, current, len)?;
            if let Some(o) = self.onboard.as_mut() {
                o.sector = current;
                o.bytes = bytes;
            }
        }
        let live = self.live()?;
        self.show(board, live)?;
        let s = board.read().clone();
        if s.profile != before {
            board.publish(Some(DeviceEvent::Profile { index: s.profile.unwrap_or(0) }));
        } else if let Some(x) = s.settings.as_ref().filter(|x| x.dpi_stage != stage_before) {
            board.publish(Some(DeviceEvent::Dpi { stage: x.dpi_stage.unwrap_or(0), dpi: x.dpi().unwrap_or(0) }));
        }
        Ok(())
    }
}

impl Driver for Connected {
    fn lost(&self) -> bool {
        self.t.lost
    }

    /// On its cable the mouse is always there. Through the receiver it's pinged (the root
    /// feature's function 1), which the receiver refuses for a mouse it can't reach. Once asleep,
    /// it isn't asked again until it's used: asking a sleeping Superlight caught or woke it, and
    /// each wake read it whole, which kept it from sleeping.
    fn online(&mut self) -> Result<bool> {
        if self.wired {
            return Ok(true);
        }
        if let (Some(t), Some(port)) = (self.asleep_since, &self.port)
            && !crate::mice::input::since(port, t)
        {
            return Ok(false);
        }
        match self.t.exchange(0x00, 0x10, &[0, 0, 0x5A], PROBE_TIMEOUT) {
            Ok(_) => {
                self.asleep_since = None;
                Ok(true)
            }
            Err(Fail::Lost(e)) => Err(e),
            Err(_) => {
                // Used but not answering yet (still linking): the next use asks again.
                self.asleep_since = Some(Instant::now());
                Ok(false)
            }
        }
    }

    fn load(&mut self, board: &Board) -> Result<()> {
        let firmware = firmware(&mut self.t)?;
        self.onboard = self.read_onboard()?;
        self.read_mode(board)?;
        let names = self.read_names()?;
        board.set(|s| s.profile_names = names);
        let live = self.live()?;
        board.set(|s| {
            s.online = true;
            s.firmware = firmware;
        });
        self.show(board, live)
    }

    fn battery(&mut self) -> Result<Reading> {
        let t = &mut self.t;
        let voltage = |r: &[u8]| u16::from_be_bytes([r.first().copied().unwrap_or(0), r.get(1).copied().unwrap_or(0)]);
        let (unified, status, volts, adc) = (
            t.feature(feature::UNIFIED_BATTERY)?,
            t.feature(feature::BATTERY_STATUS)?,
            t.feature(feature::BATTERY_VOLTAGE)?,
            t.feature(feature::ADC_MEASUREMENT)?,
        );
        let at = |r: &[u8], i: usize| r.get(i).copied().unwrap_or(0xFF);
        let (percent, charge, mut mv) = if unified != 0 {
            // [state of charge, level, charging status, external power]
            let r = t.request(unified, 0x10, &[])?;
            (Some(at(&r, 0)).filter(|v| *v <= 100), p::unified_battery_state(at(&r, 2)), 0)
        } else if status != 0 {
            // [level, next level, status]
            let r = t.request(status, 0x00, &[])?;
            (Some(at(&r, 0)).filter(|v| *v <= 100), p::battery_level_state(at(&r, 2)), 0)
        } else if volts != 0 {
            let r = t.request(volts, 0x00, &[])?;
            let (mv, flags) = (voltage(&r), at(&r, 2));
            let charging = flags & 0x80 != 0;
            let charge = if charging && flags & 0x03 == 0x02 {
                Charge::Full
            } else if flags & 0x10 != 0 {
                Charge::ChargingSlowly
            } else if charging {
                Charge::Charging
            } else {
                Charge::Discharging
            };
            ((mv > 0).then(|| p::voltage_percent(mv)), charge, mv)
        } else if adc != 0 {
            let r = t.request(adc, 0x00, &[])?;
            let (mv, flags) = (voltage(&r), at(&r, 2));
            if flags & 0x01 == 0 || mv == 0 {
                (None, Charge::Unknown, 0)
            } else {
                let charge = if flags & 0x02 != 0 { Charge::Charging } else { Charge::Discharging };
                (Some(p::voltage_percent(mv)), charge, mv)
            }
        } else {
            bail!("This mouse doesn't report its battery.");
        };
        // A voltage too, for the battery page, where the mouse has one beside its percentage.
        if mv == 0 && volts != 0 && (unified != 0 || status != 0) {
            mv = t.request(volts, 0x00, &[]).map(|r| voltage(&r)).unwrap_or(0);
        }
        let level = percent.ok_or_else(|| anyhow!("The mouse didn't say how full its battery is."))?;
        Ok(Reading { level, charging: charge.charging(), millivolts: mv, by_voltage: false })
    }

    fn changes(&mut self, board: &Board, loaded: bool) -> Result<Follow> {
        while let Some(r) = self.t.read(Duration::from_millis(1))? {
            if p::is_event(&r, self.t.index) {
                self.t.events.push(r);
            }
        }
        let events = std::mem::take(&mut self.t.events);
        let mut follow = Follow::default();
        let mut moved = false;
        for e in &events {
            match self.t.feature_at(e[2]) {
                Some(f) if BATTERIES.contains(&f) => follow.battery = true,
                // Linked up again: no need to wait to see it used before asking.
                Some(feature::WIRELESS_STATUS) => self.asleep_since = None,
                // What the Superlights announce for a profile or DPI switch is to be seen on
                // hardware (PLAN-mice.md, section 6): any of these, and the profile is read again.
                Some(feature::ONBOARD_PROFILES | feature::ADJUSTABLE_DPI | feature::EXTENDED_DPI) => moved = true,
                _ => {}
            }
        }
        // Taken over or let go: said at once, and let go, what the mouse now runs is read. Never
        // asked asleep: that would wake it.
        let awake = board.read().online;
        if loaded && awake && self.mode_read.is_none_or(|t| t.elapsed() >= MODE_EVERY) && self.read_mode(board)? {
            board.publish(None);
            moved |= !board.read().host_mode;
        }
        if moved {
            if !loaded {
                return Ok(Follow { load: true, ..Default::default() });
            }
            self.follow_mouse(board)?;
            follow.settings = true;
        }
        Ok(follow)
    }

    /// Rewrite the profile in use with `new`, as G HUB's onboard memory mode does: read it again
    /// (G HUB may have changed it), change it, skip the write if nothing changed, write, read it
    /// back, and put the old one back if the mouse didn't keep it. Then select it again so the mouse
    /// runs it now.
    fn apply(&mut self, board: &Board, new: &Settings) -> Result<()> {
        let o = self.onboard.as_ref().ok_or_else(|| anyhow!("The mouse's settings aren't read yet."))?;
        let (fi, sector, len, format) = (o.feature, o.sector, o.info.sector_len(), o.info.format);
        if !ob::writable(format) {
            bail!("hrtz can't change this mouse's settings yet (profile format {format}).");
        }
        let before = self.read_sector(fi, sector, len)?;
        if !ob::crc_ok(&before) {
            bail!("The profile on the mouse failed its checksum, so hrtz won't write over it.");
        }
        let profile = ob::decode(&before, format);
        let current = board.read().settings.clone().unwrap_or_default();
        let mut bytes = before.clone();
        let mut in_use = None;

        if new.stages.is_some() || new.dpi_stage_count.is_some() || new.dpi_stage.is_some() {
            let slots = new.stages.clone().or(current.stages).unwrap_or_default();
            let count = new.dpi_stage_count.or(current.dpi_stage_count).map_or(slots.len(), |c| c as usize);
            if count == 0 || count > slots.len() {
                bail!("This mouse holds 1 to {} DPI stages.", slots.len().max(1));
            }
            let options = self.dpi_options()?;
            let fallback_lod = profile.stages.first().map_or(2, |s| s.lod).clamp(1, 3);
            let mut stages = Vec::with_capacity(count);
            for (i, s) in slots[..count].iter().enumerate() {
                let was = profile.stages.get(i);
                // Only a changed value is held to the sensor's list.
                if was.is_none_or(|w| w.x != s.dpi) && !options.is_empty() && !options.contains(&s.dpi) {
                    bail!("Stage {}: this mouse doesn't offer {} DPI.", i + 1, thousands(s.dpi));
                }
                // One DPI here; a stage that keeps its X keeps its Y, should G HUB have split them.
                let y = was.filter(|w| w.x == s.dpi).map_or(s.dpi, |w| w.y);
                let lod = if ob::v6(format) {
                    was.map(|w| w.lod).filter(|l| (1..=3).contains(l)).unwrap_or(fallback_lod)
                } else {
                    0
                };
                stages.push(Stage { x: s.dpi, y, lod });
            }
            // The stage in use is the one it starts on, too.
            let plan = Plan { stages, default_index: new.dpi_stage.or(current.dpi_stage).unwrap_or(0) };
            in_use = Some(plan.default_index);
            let write_lod = !self.usb.is_some_and(|u| NO_LOD_WRITES.contains(&u));
            bytes = ob::encode_stages(&bytes, format, &plan, self.dpi_limits(format)?, write_lod)
                .map_err(|e| anyhow!(e))?;
        }
        if let Some(want) = &new.buttons {
            let (at, room) = buttons::table(format);
            let mut changed = false;
            for (i, a) in want.iter().enumerate().take(room) {
                let record = buttons::encode(a).map_err(|e| anyhow!("Button {}: {e}", i + 1))?;
                let slot = &mut bytes[at + i * 4..at + i * 4 + 4];
                if slot != record {
                    slot.copy_from_slice(&record);
                    changed = true;
                }
            }
            if changed {
                ob::apply_crc(&mut bytes);
            }
        }
        if let Some(hz) = new.polling_hz {
            let shown = if self.wired && ob::v6(format) { profile.rate_wired } else { profile.rate_wireless };
            if shown != Some(hz) {
                let offered = board
                    .read()
                    .capabilities
                    .as_ref()
                    .and_then(|c| c.polling.clone())
                    .map(|p| p.rates)
                    .unwrap_or_default();
                if !offered.contains(&hz) {
                    bail!("This connection doesn't carry {} Hz.", thousands(hz));
                }
                let limits = ob::rate_limits(format, self.usb_hex().as_deref());
                bytes = ob::encode_rate(&bytes, format, self.wired, hz, limits).map_err(|e| anyhow!(e))?;
            }
        }

        if bytes != before {
            self.write_sector(fi, sector, &bytes)?;
            let kept = self.read_sector(fi, sector, len)?;
            if kept != bytes {
                let _ = self.write_sector(fi, sector, &before);
                bail!("The mouse didn't keep the new settings, so its profile was put back.");
            }
            // To see on hardware whether the mouse runs a rewritten profile without this.
            let _ = self.t.request(fi, ob::func::SET_CURRENT, &sector.to_be_bytes());
        }
        if let Some(o) = self.onboard.as_mut() {
            o.bytes = bytes;
        }
        // The mouse stays on the stage slot it was on: move it to the one in use.
        if let Some(i) = in_use
            && self.current_stage()? != Some(i)
        {
            self.set_stage(i)?;
        }
        let live = self.live()?;
        self.show(board, live)
    }

    /// The onboard memory for a backup: getInfo's reply, then every sector in order (`backup.rs`).
    /// Erased sectors read as 0xFF.
    fn read_image(&mut self, board: &Board, progress: bool) -> Result<Vec<u8>> {
        let fi = self.t.feature(feature::ONBOARD_PROFILES)?;
        if fi == 0 {
            bail!("This mouse keeps no profiles to back up.");
        }
        let mut image = self.t.request(fi, ob::func::INFO, &[])?;
        image.resize(backup::INFO_LEN, 0);
        let info = Info::parse(&image);
        if !ob::writable(info.format) {
            bail!("hrtz can't back up this mouse yet (profile format {}).", info.format);
        }
        let (count, len) = (info.sector_count as usize, info.sector_len());
        for s in 0..count {
            image.extend(self.read_sector(fi, s as u16, len)?);
            if progress {
                board.publish(Some(DeviceEvent::Progress { done: s + 1, total: count }));
            }
        }
        Ok(image)
    }

    /// Put back a backup: the sectors of `target` that differ from `current` (both as read), each
    /// read back after. An erased one is left as it is: the mouse checks every write's checksum,
    /// which erased bytes don't carry. Then the mouse runs its profile again, and it's read whole.
    fn write_image(&mut self, board: &Board, current: &[u8], target: &[u8]) -> Result<usize> {
        let (now, then) = (backup::split(current)?, backup::split(target)?);
        if now.head != then.head {
            bail!("This backup is from a mouse whose memory is laid out differently.");
        }
        let fi = self.t.feature(feature::ONBOARD_PROFILES)?;
        let changed: Vec<usize> = (0..then.sectors.len())
            .filter(|i| now.sectors[*i] != then.sectors[*i] && !backup::erased(then.sectors[*i]))
            .collect();
        for (n, i) in changed.iter().enumerate() {
            let (sector, bytes) = (*i as u16, then.sectors[*i]);
            self.write_sector(fi, sector, bytes)?;
            if self.read_sector(fi, sector, bytes.len())? != bytes {
                bail!("The mouse didn't keep sector {sector} as written.");
            }
            board.publish(Some(DeviceEvent::Progress { done: n + 1, total: changed.len() }));
        }
        if !changed.is_empty() {
            let current = self.current_sector(fi)?;
            let _ = self.t.request(fi, ob::func::SET_CURRENT, &current.to_be_bytes());
            self.load(board)?;
        }
        Ok(changed.len())
    }

    /// Factory settings: the mouse's own factory profile (its ROM profile, the first for a profile
    /// past the ROM's count) written over each profile it lists, then the first one selected. The
    /// factory profiles are read and checked before anything is written.
    fn factory_reset(&mut self, board: &Board) -> Result<()> {
        let o = self.onboard.as_ref().ok_or_else(|| anyhow!("The mouse's settings aren't read yet."))?;
        let (fi, len, roms, format, directory) =
            (o.feature, o.info.sector_len(), o.info.rom_count, o.info.format, o.directory.clone());
        if !ob::writable(o.info.format) || roms == 0 || directory.is_empty() {
            bail!("hrtz can't restore this mouse's factory settings yet.");
        }
        let mut factory = Vec::new();
        for r in 0..roms as u16 {
            let mut bytes = self
                .read_sector(fi, ob::ROM_FIRST + r, len)
                .context("The mouse didn't hand over its factory profile, so nothing was changed.")?;
            // Kept with its checksum's place erased: the user's Superlight's held what its untouched
            // profiles hold, but FF FF there (the probe, 2026-10-18). Profiles carry one: it's added.
            if ob::stored_crc(&bytes) == 0xFFFF && !backup::erased(&bytes) {
                ob::apply_crc(&mut bytes);
            }
            if !ob::crc_ok(&bytes) || ob::decode(&bytes, format).stages.is_empty() {
                bail!("Its factory profile doesn't read as a profile, so nothing was changed.");
            }
            factory.push(bytes);
        }
        for (i, e) in directory.iter().enumerate() {
            let bytes = &factory[i.min(factory.len() - 1)];
            if self.read_sector(fi, e.sector, len)? != *bytes {
                self.write_sector(fi, e.sector, bytes)?;
                if self.read_sector(fi, e.sector, len)? != *bytes {
                    bail!("The mouse didn't keep profile {} as written.", i + 1);
                }
            }
            board.publish(Some(DeviceEvent::Progress { done: i + 1, total: directory.len() }));
        }
        self.t.request(fi, ob::func::SET_CURRENT, &directory[0].sector.to_be_bytes())?;
        self.load(board)
    }

    /// For a test report: every feature the mouse lists (by the feature set, 0x0001), getInfo's
    /// reply, its mode and the profile in use, and each factory profile as read.
    fn report(&mut self, _board: &Board) -> Result<serde_json::Value> {
        let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ");
        let mut features = Vec::new();
        let set = self.t.feature(0x0001)?;
        if set != 0 {
            let count = self.t.request(set, 0x00, &[])?.first().copied().unwrap_or(0);
            for i in 0..=count {
                let r = self.t.request(set, 0x10, &[i])?;
                if r.len() >= 4 {
                    features.push(serde_json::json!({
                        "index": i,
                        "id": format!("{:04x}", u16::from_be_bytes([r[0], r[1]])),
                        "type": r[2],
                        "version": r[3],
                    }));
                }
            }
        }
        let mut onboard = serde_json::Value::Null;
        let fi = self.t.feature(feature::ONBOARD_PROFILES)?;
        if fi != 0 {
            let info = self.t.request(fi, ob::func::INFO, &[])?;
            let parsed = Info::parse(&info);
            let mode = self.t.request(fi, ob::func::GET_MODE, &[]).ok();
            let current = self.t.request(fi, ob::func::GET_CURRENT, &[]).ok();
            let mut roms = Vec::new();
            for r in 0..parsed.rom_count as u16 {
                let sector = ob::ROM_FIRST + r;
                roms.push(match self.read_sector(fi, sector, parsed.sector_len()) {
                    Ok(b) => serde_json::json!({ "sector": format!("{sector:#06x}"), "crcOk": ob::crc_ok(&b), "bytes": hex(&b) }),
                    Err(e) => serde_json::json!({ "sector": format!("{sector:#06x}"), "refused": format!("{e:#}") }),
                });
            }
            onboard = serde_json::json!({
                "info": hex(&info),
                "mode": mode.as_deref().map(hex),
                "current": current.as_deref().map(hex),
                "rom": roms,
            });
        }
        Ok(serde_json::json!({ "index": format!("{:#04x}", self.t.index), "features": features, "onboard": onboard }))
    }

    /// Save profile names: each profile whose name differs is rewritten, checked, and read again.
    fn set_profile_names(&mut self, board: &Board, names: &[String]) -> Result<()> {
        let o = self.onboard.as_ref().ok_or_else(|| anyhow!("The mouse's settings aren't read yet."))?;
        let (fi, len, current, entries) = (o.feature, o.info.sector_len(), o.sector, o.directory.clone());
        if !ob::writable(o.info.format) {
            bail!("hrtz can't change this mouse's profiles yet (profile format {}).", o.info.format);
        }
        let on_mouse = board.read().profile_names.clone().unwrap_or_default();
        for (i, (e, name)) in entries.iter().zip(names).enumerate() {
            let theirs = on_mouse.get(i).cloned().flatten();
            // The same, or hrtz's own stand-in for a profile the mouse keeps no name for.
            if theirs.as_deref() == Some(name.trim())
                || (theirs.is_none() && name.trim() == format!("Profile {}", i + 1))
            {
                continue;
            }
            let before = self.read_sector(fi, e.sector, len)?;
            if !ob::crc_ok(&before) {
                bail!("Profile {} failed its checksum, so hrtz won't write over it.", i + 1);
            }
            let mut bytes = before.clone();
            ob::encode_name(&mut bytes, name);
            if bytes == before {
                continue;
            }
            self.write_sector(fi, e.sector, &bytes)?;
            if self.read_sector(fi, e.sector, len)? != bytes {
                let _ = self.write_sector(fi, e.sector, &before);
                bail!("The mouse didn't keep profile {}'s name, so it was put back.", i + 1);
            }
            if e.sector == current
                && let Some(o) = self.onboard.as_mut()
            {
                o.bytes = bytes;
            }
        }
        let names = self.read_names()?;
        board.set(|s| s.profile_names = names);
        Ok(())
    }

    /// Switch to the profile at `index` in the directory. This writes nothing to flash.
    fn set_profile(&mut self, board: &Board, index: u8) -> Result<()> {
        let o = self.onboard.as_ref().ok_or_else(|| anyhow!("The mouse's settings aren't read yet."))?;
        let (fi, len) = (o.feature, o.info.sector_len());
        let Some(sector) = o.directory.get(index as usize).map(|e| e.sector) else {
            bail!("This mouse has {} profiles.", o.directory.len());
        };
        self.t.request(fi, ob::func::SET_CURRENT, &sector.to_be_bytes())?;
        let bytes = self.read_sector(fi, sector, len)?;
        if let Some(o) = self.onboard.as_mut() {
            o.sector = sector;
            o.bytes = bytes;
        }
        let live = self.live()?;
        self.show(board, live)?;
        board.publish(Some(DeviceEvent::Profile { index }));
        Ok(())
    }
}

/// 12345 -> "12,345".
fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
