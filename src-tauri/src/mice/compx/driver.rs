//! A connected Zaopin mouse, through its receiver or its cable: one command at a time, the flash
//! image read in regions, and the status packets the mouse sends by itself. An ATK mouse runs here
//! too, on what it shares with a Zaopin (`atk`).

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use hidapi::HidApi;

use super::atk;
use super::image;
use super::models;
use super::protocol::{self as p, Packet, cmd, status};
use crate::mice::model::Settings;
use crate::mice::{Board, DeviceEvent, DeviceState, Driver, Family, Finder, Follow, Link, Reading, Waiting};

/// Everything below this address is read on connect: config, buttons and shortcut slots.
pub(crate) const LOADED_REGION: usize = 0x300;
/// How often an ATK's DPI stage and polling rate are read, as it doesn't say when they change.
const ATK_LOOK_EVERY: Duration = Duration::from_secs(1);
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
const RESET_TIMEOUT: Duration = Duration::from_millis(5000);
/// How long the receiver is given to find a mouse to pair with, as in the vendor app.
const PAIR_FOR: Duration = Duration::from_secs(30);

/// Zaopin's family: its receivers, and its mice on their cables, each a connection of its own.
pub struct Compx;

impl Family for Compx {
    fn connections(&self, devices: &[hidapi::DeviceInfo]) -> Vec<String> {
        devices.iter().filter(|d| is_config_interface(d)).map(|d| d.path().to_string_lossy().into_owned()).collect()
    }

    fn finder(&self, id: &str) -> Box<dyn crate::mice::Finder> {
        Box::new(HidFinder { api: None, path: id.to_string(), refused: None })
    }
}

/// One Zaopin receiver, or a mouse on its cable, through hidapi: the config collection at `path`.
pub struct HidFinder {
    api: Option<HidApi>,
    path: String,
    /// Set once the receiver turns out to be another maker's or model's, or not the settings channel:
    /// it isn't asked again while it stays plugged in, so hrtz doesn't keep interrupting that maker's
    /// own app.
    refused: Option<LeftAlone>,
}

/// A receiver, a mouse or a channel of one that hrtz doesn't run, and why.
#[derive(Debug, Clone)]
pub(crate) struct LeftAlone(String);

impl LeftAlone {
    /// Another maker's mouse on Zaopin's vendor ID.
    fn not_zaopin(cid: u8, mid: u8) -> Self {
        Self(format!("not a Zaopin mouse (CID {cid}, MID {mid}), so it's left alone"))
    }

    /// An ATK model hrtz doesn't run yet.
    fn other_atk(cid: u8, mid: u8) -> Self {
        Self(format!("an ATK mouse hrtz doesn't support yet (CID {cid}, MID {mid}), so it's left alone"))
    }
}

impl std::fmt::Display for LeftAlone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LeftAlone {}

/// Whose mouse a connection is, by its vendor ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Maker {
    Zaopin,
    /// Through its receiver (`true`) or on its cable.
    Atk {
        receiver: bool,
    },
}

/// The report IDs a report descriptor declares (its Report ID items).
fn report_ids(descriptor: &[u8]) -> Vec<u8> {
    let mut ids = Vec::new();
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        // A long item: its size is in the next byte.
        if prefix == 0xFE {
            i += 3 + descriptor.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let size = [0, 1, 2, 4][(prefix & 0x03) as usize];
        if prefix & 0xFC == 0x84
            && size == 1
            && let Some(&id) = descriptor.get(i + 1)
        {
            ids.push(id);
        }
        i += 1 + size;
    }
    ids
}

impl HidFinder {
    fn api(&mut self) -> Result<&mut HidApi> {
        if self.api.is_none() {
            self.api = Some(HidApi::new()?);
        }
        Ok(self.api.as_mut().unwrap())
    }
}

fn is_config_interface(d: &hidapi::DeviceInfo) -> bool {
    (d.vendor_id() == p::VENDOR_ID || d.vendor_id() == atk::VENDOR_ID)
        && d.usage_page() == p::CONFIG_USAGE_PAGE
        && d.usage() == p::CONFIG_USAGE
}

impl Finder for HidFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        if let Some(n) = &self.refused {
            return Err(n.clone().into());
        }
        let path = self.path.clone();
        let api = self.api()?;
        api.refresh_devices()?;
        let info = api
            .device_list()
            .find(|d| is_config_interface(d) && d.path().to_string_lossy() == *path)
            .ok_or_else(|| anyhow!("no Zaopin receiver found"))?
            .clone();
        let dev = info.open_device(api).context("open HID device")?;
        let maker = if info.vendor_id() == atk::VENDOR_ID {
            Maker::Atk {
                receiver: atk::is_receiver(
                    info.vendor_id(),
                    info.product_id(),
                    info.product_string().unwrap_or_default(),
                ),
            }
        } else {
            Maker::Zaopin
        };
        // An ATK's receiver and mouse have a second collection on the same usage, for 19-byte
        // reports: only the one with report 8 is the settings channel. Asking Windows for the
        // descriptor sends nothing to the device.
        if maker != Maker::Zaopin {
            let mut buf = [0u8; 4096];
            if dev.get_report_descriptor(&mut buf).is_ok_and(|n| !report_ids(&buf[..n]).contains(&p::REPORT_ID)) {
                let why = LeftAlone("not the settings channel, so it's left alone".into());
                board.set(|s| *s = DeviceState { foreign: true, ..DeviceState::default() });
                self.refused = Some(why.clone());
                return Err(why.into());
            }
        }
        let opened = Opened {
            family: "compx",
            maker,
            product_id: info.product_id(),
            link: Box::new(dev),
            // The vendor driver keeps at least 8 ms between writes; the receiver drops packets otherwise.
            gap: Duration::from_millis(8),
            #[cfg(windows)]
            port: crate::usb::port_of(&info.path().to_string_lossy()),
        };
        connect(opened, board).inspect_err(|e| self.refused = e.downcast_ref::<LeftAlone>().cloned())
    }

    fn count(&mut self) -> usize {
        let Ok(api) = self.api() else { return 1 };
        if api.refresh_devices().is_err() {
            return 1;
        }
        // Pairing wants one Zaopin receiver: an ATK's (with two such collections) isn't one.
        api.device_list().filter(|d| is_config_interface(d) && d.vendor_id() == p::VENDOR_ID).count()
    }
}

/// A receiver or a mouse on its cable, opened.
pub(crate) struct Opened {
    /// The key's first part: `compx`, or `sim` for a simulated mouse in a dev build, so it never
    /// takes over what a real one keeps.
    pub family: &'static str,
    pub maker: Maker,
    /// Its USB product ID: under CompX's vendor ID, an ATK hrtz runs is only taken on one of ATK's.
    pub product_id: u16,
    pub link: Box<dyn Link>,
    /// The least time between two writes.
    pub gap: Duration,
    #[cfg(windows)]
    pub port: Option<crate::usb::UsbPort>,
}

/// Meet the mouse: who it is and how it's connected, then ask it for status notifications.
pub(crate) fn connect(opened: Opened, board: &Board) -> Result<Box<dyn Driver>> {
    let mut m = Connected {
        link: opened.link,
        gap: opened.gap,
        image: vec![0xFF; p::FLASH_SIZE],
        pending_status: Vec::new(),
        lost: false,
        atk: None,
    };
    if let Maker::Atk { receiver } = opened.maker {
        #[cfg(windows)]
        let port = opened.port;
        #[cfg(not(windows))]
        let port = ();
        return connect_atk(m, opened.family, receiver, None, opened.product_id, port, board);
    }
    let nonce: [u8; 4] = rand_bytes();
    let r = m.transact(&p::packet(cmd::HANDSHAKE, 0, &nonce, Some(8)), REPLY_TIMEOUT)?;
    let d = p::payload(&r);
    let (cid, mid, device_type) = (d[4], d[5], d[6]);
    // An ATK under CompX's vendor ID (VXE's R1 line, ATK's Dragonfly F1): met as ATK's app meets it,
    // its connection type from this handshake.
    if atk::on_compx_vid(opened.product_id, cid, mid) {
        #[cfg(windows)]
        let port = opened.port;
        #[cfg(not(windows))]
        let port = ();
        let wired = atk::link::wired(device_type);
        return connect_atk(m, opened.family, !wired, Some(device_type), opened.product_id, port, board);
    }
    if !models::is_zaopin(cid, mid) {
        board.set(|s| *s = DeviceState { cid, mid, foreign: true, ..DeviceState::default() });
        return Err(LeftAlone::not_zaopin(cid, mid).into());
    }
    board.set(|s| {
        *s = DeviceState {
            key: Some(format!("{}/{cid}-{mid}", opened.family)),
            connected: true,
            model: Some(models::model_name(mid).to_string()),
            brand: Some("Zaopin".into()),
            cid,
            mid,
            device_type,
            capabilities: Some(models::capabilities(mid, device_type)),
            #[cfg(windows)]
            port: opened.port,
            ..Default::default()
        }
    });
    // Tells the mouse a configuration app is listening, which enables status notifications.
    let _ = m.transact(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[1], None), REPLY_TIMEOUT);
    board.publish(None);
    Ok(Box::new(m))
}

/// Meet an ATK: through its receiver only once the mouse is linked (a receiver answers for itself,
/// but who the mouse is goes unanswered while it sleeps), then how it's connected (the handshake's
/// connection type, as ATK's app reads it; `kind` when it's been asked already), then which model it
/// is. Nothing is sent that ATK's own app doesn't send.
fn connect_atk(
    mut m: Connected,
    family: &'static str,
    receiver: bool,
    kind: Option<u8>,
    product_id: u16,
    #[allow(unused_variables)] port: PortOf,
    board: &Board,
) -> Result<Box<dyn Driver>> {
    m.atk = Some(Atk { receiver, looked: Instant::now(), model: None });
    if receiver && !m.online()? {
        return Err(Waiting.into());
    }
    // A mouse that doesn't answer it is taken as on a 1K receiver or cable.
    let kind = kind.unwrap_or_else(|| {
        let nonce: [u8; 4] = rand_bytes();
        m.transact(&p::packet(cmd::HANDSHAKE, 0, &nonce, Some(8)), REPLY_TIMEOUT)
            .map(|r| atk::kind_of(&r))
            .unwrap_or(if receiver { atk::link::DONGLE_1K } else { atk::link::WIRED_1K })
    });
    let r = m.transact_simple(atk::CID_MID)?;
    let (cid, mid) = (r[6], r[7]);
    let Some(model) = atk::model(cid, mid) else {
        board.set(|s| *s = DeviceState { cid, mid, foreign: true, ..DeviceState::default() });
        return Err(LeftAlone::other_atk(cid, mid).into());
    };
    if let Some(a) = m.atk.as_mut() {
        a.model = Some(model);
    }
    // A PAW3955's stages are kept past a Zaopin's flash.
    m.image.resize(atk::memory_len(model), 0xFF);
    let mut caps = atk::capabilities(model, kind);
    if product_id == atk::R1_SE_RECEIVER {
        caps.polling = None;
    }
    board.set(|s| {
        *s = DeviceState {
            key: Some(format!("{family}/{cid}-{mid}")),
            connected: true,
            model: Some(model.name.to_string()),
            brand: Some(model.brand.into()),
            cid,
            mid,
            device_type: kind,
            capabilities: Some(caps),
            #[cfg(windows)]
            port,
            ..Default::default()
        }
    });
    board.publish(None);
    Ok(Box::new(m))
}

#[cfg(windows)]
type PortOf = Option<crate::usb::UsbPort>;
#[cfg(not(windows))]
type PortOf = ();

/// What's an ATK's own on a connection.
struct Atk {
    /// Through its receiver, rather than on its cable.
    receiver: bool,
    /// When the DPI stage and polling rate were last read.
    looked: Instant,
    /// Which model it is, once it's said.
    model: Option<&'static atk::Model>,
}

/// A Zaopin mouse, met, or an ATK.
struct Connected {
    link: Box<dyn Link>,
    gap: Duration,
    /// What's known of the mouse's memory: the first `LOADED_REGION` bytes once loaded, all of it
    /// after a backup.
    image: Vec<u8>,
    pending_status: Vec<u8>,
    lost: bool,
    /// Set for an ATK.
    atk: Option<Atk>,
}

impl Connected {
    fn refresh_settings(&mut self, board: &Board) {
        let decoded = match &self.atk {
            Some(a) => {
                let mut decoded = image::decode(&self.image);
                if let Some(model) = a.model {
                    decoded = atk::offered(&decoded, model);
                    // Its stages' DPI as its sensor stores them.
                    if let Some(stages) = decoded.stages.as_mut() {
                        atk::read_stages(model, &self.image, stages);
                    }
                }
                decoded
            }
            // A Zaopin's DPI as its model keeps it, and only what hrtz offers on the model.
            None => {
                let (mid, caps) = {
                    let s = board.read();
                    (s.mid, s.capabilities.clone())
                };
                let decoded = image::decode_as(&self.image, models::dpi_method(mid));
                match caps {
                    Some(caps) => models::offered(decoded, &caps),
                    None => decoded,
                }
            }
        };
        board.set(|s| s.settings = Some(decoded));
    }

    /// Refuse what an ATK isn't offered, should it be asked anyway.
    fn zaopin_only(&self) -> Result<()> {
        if self.atk.is_some() {
            bail!("This mouse can't do that.");
        }
        Ok(())
    }

    /// An ATK says nothing when its DPI button or a button combination changes the stage, the
    /// polling rate or the profile, so they're read every second while it's awake, and told as a
    /// Zaopin's are. A profile switch loads the new profile's settings.
    fn look_atk(&mut self, board: &Board) -> Result<Follow> {
        let Some(a) = self.atk.as_mut() else { return Ok(Follow::default()) };
        if a.looked.elapsed() < ATK_LOOK_EVERY || !board.read().online {
            return Ok(Follow::default());
        }
        a.looked = Instant::now();
        // Only when it said which profile it's on at load: one that doesn't answer isn't asked
        // every second. (Read into a binding first: the board is written below.)
        let was = board.read().profile;
        if let Some(was) = was {
            let r = self.transact_simple(cmd::GET_PROFILE)?;
            if r[5] > 0 && r[6] != was {
                self.load_atk(board)?;
                let index = board.read().profile.unwrap_or(was);
                board.publish(Some(DeviceEvent::Profile { index }));
                return Ok(Follow { settings: true, battery: true, ..Default::default() });
            }
        }
        let row = self.read_flash(0x00, 6)?;
        if row == self.image[0x00..0x06] {
            return Ok(Follow::default());
        }
        let was = board.read().settings.clone();
        self.image[0x00..0x06].copy_from_slice(&row);
        self.refresh_settings(board);
        let now = board.read().settings.clone();
        if let Some(s) = &now {
            if was.as_ref().and_then(|w| w.polling_hz) != s.polling_hz {
                board.publish(Some(DeviceEvent::Polling { hz: s.polling_hz.unwrap_or(0) }));
            }
            if was.as_ref().map(|w| (w.dpi_stage, w.dpi())) != Some((s.dpi_stage, s.dpi())) {
                board.publish(Some(DeviceEvent::Dpi { stage: s.dpi_stage.unwrap_or(0), dpi: s.dpi().unwrap_or(0) }));
            }
        }
        Ok(Follow { settings: true, ..Default::default() })
    }

    /// An ATK's load: its firmware, the profile in use and the settings region, as ATK's app reads
    /// them. No long-range mode: it isn't offered. A mouse that doesn't say its profile shows none.
    fn load_atk(&mut self, board: &Board) -> Result<()> {
        let fw = self.transact_simple(cmd::VERSION).ok();
        let profile = self.transact_simple(cmd::GET_PROFILE).ok();
        let region = self.read_flash(0, p::CONFIG_REGION)?;
        self.image[..p::CONFIG_REGION].copy_from_slice(&region);
        // A PAW3955's stages, from 0x1B00.
        if self.image.len() > p::FLASH_SIZE {
            let rows = self.read_flash(p::FLASH_SIZE, self.image.len() - p::FLASH_SIZE)?;
            self.image[p::FLASH_SIZE..].copy_from_slice(&rows);
        }
        if let Some(a) = self.atk.as_mut() {
            a.looked = Instant::now();
        }
        board.set(|s| {
            s.online = true;
            // Binary-coded decimal: 0x01 0x23 is 1.23, written as a Zaopin's is.
            s.firmware = fw.map(|r| format!("{:x}.{:02x}", r[6], r[7]));
            s.profile = profile.filter(|r| r[5] > 0 && r[6] < atk::PROFILES).map(|r| r[6]);
            s.long_range = None;
        });
        self.refresh_settings(board);
        Ok(())
    }

    // ---- transport ------------------------------------------------------------------------

    fn transact_simple(&mut self, command: u8) -> Result<Packet> {
        self.transact(&p::packet(command, 0, &[], None), REPLY_TIMEOUT)
    }

    fn transact(&mut self, pkt: &Packet, timeout: Duration) -> Result<Packet> {
        let want = (pkt[1], p::packet_addr(pkt));
        let match_addr = matches!(pkt[1], cmd::READ_FLASH | cmd::WRITE_FLASH);
        for _ in 0..3 {
            self.write(pkt)?;
            let deadline = Instant::now() + timeout;
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                let Some(r) = self.read(left)? else { break };
                if r[1] == cmd::STATUS_CHANGED && pkt[1] != cmd::STATUS_CHANGED {
                    self.pending_status.push(r[6]);
                    continue;
                }
                if r[1] == want.0 && (!match_addr || p::packet_addr(&r) == want.1) {
                    return Ok(r);
                }
            }
        }
        bail!("The receiver did not answer (command 0x{:02X}).", pkt[1])
    }

    fn write(&mut self, pkt: &Packet) -> Result<()> {
        if self.lost {
            bail!("not connected");
        }
        if let Err(e) = self.link.write(pkt) {
            self.lost = true;
            bail!("Lost the receiver: {e}");
        }
        thread::sleep(self.gap);
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        if self.lost {
            bail!("not connected");
        }
        let mut buf = [0u8; p::PACKET_LEN + 1];
        let n = match self.link.read(&mut buf, timeout) {
            Ok(n) => n,
            Err(e) => {
                self.lost = true;
                bail!("Lost the receiver: {e}");
            }
        };
        let mut pkt = [0u8; p::PACKET_LEN];
        match n {
            0 => return Ok(None),
            17.. => pkt.copy_from_slice(&buf[..17]),
            16 => {
                pkt[0] = p::REPORT_ID;
                pkt[1..].copy_from_slice(&buf[..16]);
            }
            _ => return Ok(None),
        }
        Ok(p::packet_valid(&pkt).then_some(pkt))
    }

    fn read_flash(&mut self, addr: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        let mut a = addr;
        while out.len() < len {
            let n = (len - out.len()).min(p::MAX_CHUNK);
            let r = self.transact(&p::packet(cmd::READ_FLASH, a as u16, &[], Some(n as u8)), REPLY_TIMEOUT)?;
            out.extend_from_slice(&r[6..6 + n]);
            a += n;
        }
        Ok(out)
    }

    fn write_flash(&mut self, addr: usize, data: &[u8]) -> Result<()> {
        for (i, chunk) in data.chunks(p::MAX_CHUNK).enumerate() {
            let a = addr + i * p::MAX_CHUNK;
            let pkt = p::packet(cmd::WRITE_FLASH, a as u16, chunk, None);
            if self.atk.is_some() {
                self.write_checked(&pkt, a, chunk)?;
            } else {
                let r = self.transact(&pkt, REPLY_TIMEOUT)?;
                if r != pkt {
                    bail!("The mouse rejected a write at 0x{a:04X}.");
                }
            }
            self.image[a..a + chunk.len()].copy_from_slice(chunk);
        }
        Ok(())
    }

    /// Sends a change to an ATK as its own app does, without counting on an answer: an echo that
    /// comes within a tenth of a second is taken out of the way. The caller reads back what it
    /// changed.
    fn send_unanswered(&mut self, pkt: &Packet) -> Result<()> {
        self.write(pkt)?;
        let at = matches!(pkt[1], cmd::READ_FLASH | cmd::WRITE_FLASH).then(|| p::packet_addr(pkt));
        let deadline = Instant::now() + Duration::from_millis(100);
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.read(left)? {
                None => break,
                Some(r) if r[1] == cmd::STATUS_CHANGED => self.pending_status.push(r[6]),
                Some(r) if r[1] == pkt[1] && at.is_none_or(|a| p::packet_addr(&r) == a) => break,
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// An ATK's write: sent unanswered, then read back.
    fn write_checked(&mut self, pkt: &Packet, a: usize, chunk: &[u8]) -> Result<()> {
        self.send_unanswered(pkt)?;
        if self.read_flash(a, chunk.len())? != chunk {
            bail!("The mouse didn't keep a write at 0x{a:04X}.");
        }
        Ok(())
    }

    // ---- status the mouse sends by itself -------------------------------------------------

    fn handle_status(&mut self, board: &Board, bits: u8) -> Result<Follow> {
        // The mouse reports every profile switch, hrtz's own too, which `set_profile` has loaded
        // already: only a switch made on the mouse needs its profile loaded (and a second load
        // would hold the mouse a second more, and show the pop-up twice).
        // The DPI and polling bits that come with a switch are the new profile's, loaded with it.
        if bits & status::PROFILE != 0 {
            let r = self.transact_simple(cmd::GET_PROFILE)?;
            if (r[5] > 0).then_some(r[6]) != board.read().profile {
                self.load(board)?;
                let index = board.read().profile.unwrap_or(0);
                board.publish(Some(DeviceEvent::Profile { index }));
                return Ok(Follow { settings: true, battery: true, ..Default::default() });
            }
            return Ok(Follow::default());
        }
        if bits & status::DPI != 0 {
            let r = self.read_flash(0x02, 4)?;
            self.image[0x02..0x06].copy_from_slice(&r);
            self.refresh_settings(board);
            // Told only where the DPI is shown: not on an ATK whose DPI method isn't settled.
            let s = board.read().settings.clone();
            if let Some((stage, dpi)) = s.and_then(|s| Some((s.dpi_stage?, s.dpi()?))) {
                board.publish(Some(DeviceEvent::Dpi { stage, dpi }));
            }
        }
        if bits & status::POLLING != 0 {
            let r = self.read_flash(0x00, 2)?;
            self.image[0x00..0x02].copy_from_slice(&r);
            self.refresh_settings(board);
            let hz = board.read().settings.as_ref().and_then(|s| s.polling_hz).unwrap_or(0);
            board.publish(Some(DeviceEvent::Polling { hz }));
        }
        Ok(Follow {
            settings: bits & (status::DPI | status::POLLING) != 0,
            battery: bits & status::BATTERY != 0,
            ..Default::default()
        })
    }
}

impl Driver for Connected {
    fn lost(&self) -> bool {
        self.lost
    }

    fn online(&mut self) -> Result<bool> {
        match &self.atk {
            None => Ok(self.transact_simple(cmd::ONLINE)?[6] != 0),
            // On its cable it's always there; its receiver says 1 while it's linked.
            Some(a) if !a.receiver => Ok(true),
            Some(_) => Ok(self.transact_simple(cmd::ONLINE)?[6] == 1),
        }
    }

    /// Read everything the UI shows. Called on connect, on wake, and after a profile switch.
    fn load(&mut self, board: &Board) -> Result<()> {
        if self.atk.is_some() {
            return self.load_atk(board);
        }
        // A receiver that answered for no mouse (CID 0, MID 0) may have been asked while its mouse
        // slept: a Z11's receiver answered so with a Z11 paired (Mayank_j's report, 2026-11-12; the
        // Z11 sleeps after 10 s). Now that a mouse is there, ask again, and meet it afresh if the
        // receiver names it.
        let met = {
            let s = board.read();
            (s.cid, s.mid)
        };
        if met == (0, 0) {
            let nonce: [u8; 4] = rand_bytes();
            let r = self.transact(&p::packet(cmd::HANDSHAKE, 0, &nonce, Some(8)), REPLY_TIMEOUT)?;
            let d = p::payload(&r);
            if (d[4], d[5]) != (0, 0) {
                self.lost = true;
                bail!("the receiver now names its mouse ({}, {}): meeting it again", d[4], d[5]);
            }
        }
        let fw = self.transact_simple(cmd::VERSION)?;
        let profile = self.transact_simple(cmd::GET_PROFILE)?;
        let long_range = self.transact_simple(cmd::GET_LONG_RANGE).ok();
        let region = self.read_flash(0, LOADED_REGION)?;
        self.image[..LOADED_REGION].copy_from_slice(&region);
        board.set(|s| {
            s.online = true;
            // Same rendering as the vendor app: hex major.minor, e.g. 0x0217 -> "2.17".
            s.firmware = Some(format!("{:x}.{:02x}", fw[6], fw[7]));
            s.profile = (profile[5] > 0).then_some(profile[6]);
            s.long_range = long_range.map(|r| r[6] != 0);
        });
        self.refresh_settings(board);
        Ok(())
    }

    fn battery(&mut self) -> Result<Reading> {
        let r = self.transact_simple(cmd::BATTERY)?;
        let d = p::payload(&r);
        if self.atk.is_some() {
            // Its own level: the voltage curve hrtz knows is a Zaopin's. The millivolts only when
            // its length says they're there.
            return Ok(Reading {
                level: d[0].min(100),
                charging: d[1] != 0,
                millivolts: if r[5] >= 4 { u16::from_be_bytes([d[2], d[3]]) } else { 0 },
                by_voltage: false,
            });
        }
        Ok(Reading {
            level: d[0].min(100),
            charging: d[1] != 0,
            millivolts: u16::from_be_bytes([d[2], d[3]]),
            by_voltage: true,
        })
    }

    fn changes(&mut self, board: &Board, loaded: bool) -> Result<Follow> {
        while let Ok(Some(r)) = self.read(Duration::from_millis(1)) {
            if r[1] == cmd::STATUS_CHANGED {
                self.pending_status.push(r[6]);
            }
        }
        let bits = self.pending_status.drain(..).fold(0u8, |a, b| a | b);
        if bits == 0 {
            return if loaded { self.look_atk(board) } else { Ok(Follow::default()) };
        }
        if !loaded {
            return Ok(Follow { load: true, ..Default::default() });
        }
        self.handle_status(board, bits)
    }

    fn apply(&mut self, board: &Board, new: &Settings) -> Result<()> {
        let caps = board.read().capabilities.clone().ok_or_else(|| anyhow!("unknown device"))?;
        // An ATK's other settings stay as the mouse holds them, whatever comes.
        let model = self.atk.as_ref().and_then(|a| a.model);
        if self.atk.is_some() && model.is_none() {
            bail!("The mouse hasn't said which model it is yet.");
        }
        let mut target;
        if let Some(m) = model {
            // Its stages' DPI goes as its sensor stores it: the rest (their colours too) as a
            // Zaopin's, with each stage's DPI held as the Zaopin's decoding reads it, so that
            // leaves the DPI alone.
            let offered = atk::offered(new, m);
            let was = board.read().settings.as_ref().and_then(|s| s.stages.clone()).unwrap_or_default();
            let held = image::decode(&self.image).stages.unwrap_or_default();
            let mut rest = offered.clone();
            if let Some(stages) = rest.stages.as_mut() {
                for (i, st) in stages.iter_mut().enumerate() {
                    st.dpi = held.get(i).map_or(st.dpi, |h| h.dpi);
                }
            }
            image::validate(&rest, &self.image, &caps).map_err(|e| anyhow!(e))?;
            if let (Some(d), Some(now)) = (&caps.dpi, &offered.stages) {
                for (i, st) in now.iter().enumerate() {
                    let changed = was.get(i).map(|w| w.dpi) != Some(st.dpi);
                    if changed && !d.holds(st.dpi) {
                        bail!("DPI must be between {} and {}, in steps of {}.", d.min, d.max, d.step);
                    }
                }
            }
            target = image::encode(&rest, &self.image);
            target.resize(self.image.len(), 0xFF);
            target[p::FLASH_SIZE..].copy_from_slice(&self.image[p::FLASH_SIZE..]);
            if let Some(now) = &offered.stages {
                atk::write_stages(m, &mut target, &was, now).map_err(|e| anyhow!(e))?;
            }
        } else {
            let method = models::dpi_method(board.read().mid);
            image::validate_as(new, &self.image, &caps, method).map_err(|e| anyhow!(e))?;
            target = image::encode_as(new, &self.image, method);
        }
        if let Some(m) = model {
            for (addr, len) in atk::fields(m) {
                if target[addr..addr + len] != self.image[addr..addr + len] {
                    let bytes = target[addr..addr + len].to_vec();
                    self.write_flash(addr, &bytes)?;
                }
            }
        } else {
            for (addr, bytes) in image::diff(&self.image, &target, p::MAX_CHUNK) {
                self.write_flash(addr, &bytes)?;
            }
        }
        self.refresh_settings(board);
        Ok(())
    }

    fn set_profile(&mut self, board: &Board, index: u8) -> Result<()> {
        if index > 3 {
            bail!("profile must be 1-4");
        }
        let pkt = p::packet(cmd::SET_PROFILE, 0, &[index], None);
        if self.atk.is_some() {
            // As its writes: sent unanswered, then asked, once it has had time to switch.
            self.send_unanswered(&pkt)?;
            thread::sleep(Duration::from_millis(150));
            let r = self.transact_simple(cmd::GET_PROFILE)?;
            if r[5] == 0 || r[6] != index {
                bail!("The mouse didn't switch to profile {}.", index + 1);
            }
        } else {
            self.transact(&pkt, REPLY_TIMEOUT)?;
            thread::sleep(Duration::from_millis(150));
        }
        self.pending_status.clear();
        // The new profile's settings first (a fifth of a second), so the pop-up and the HUD say it
        // at once; then its key-combination slots, in the same job, so nothing writes to the
        // mouse in between. The firmware, long-range mode and battery don't change with a profile.
        // An ATK's slots aren't read: its buttons aren't offered.
        let config = self.read_flash(0, p::CONFIG_REGION)?;
        self.image[..p::CONFIG_REGION].copy_from_slice(&config);
        board.set(|s| s.profile = Some(index));
        self.refresh_settings(board);
        board.publish(Some(DeviceEvent::Profile { index }));
        if self.atk.is_some() {
            return Ok(());
        }
        let slots = self.read_flash(p::CONFIG_REGION, LOADED_REGION - p::CONFIG_REGION)?;
        self.image[p::CONFIG_REGION..LOADED_REGION].copy_from_slice(&slots);
        self.refresh_settings(board);
        board.publish(None);
        Ok(())
    }

    fn set_long_range(&mut self, board: &Board, on: bool) -> Result<()> {
        self.zaopin_only()?;
        let mut data = [0u8; 10];
        data[0] = on as u8;
        self.transact(&p::packet(cmd::SET_LONG_RANGE, 0, &data, None), REPLY_TIMEOUT)?;
        let r = self.transact_simple(cmd::GET_LONG_RANGE)?;
        board.set(|s| s.long_range = Some(r[6] != 0));
        board.publish(None);
        Ok(())
    }

    /// Put the receiver into pairing for any mouse of this brand, as the vendor app does, and wait
    /// up to 30 s for one, asking how it's going once a second (`DeviceEvent::Pairing`). This holds
    /// the device thread, so housekeeping waits meanwhile, as the vendor pauses its own timers.
    /// Returns whether a mouse paired; the receiver is then met again, in case it's another model.
    fn pair(&mut self, board: &Board, connections: usize) -> Result<bool> {
        self.zaopin_only()?;
        if board.read().capabilities.as_ref().is_some_and(|c| c.wired) {
            bail!("Pairing works over the receiver only. Unplug the mouse's cable first.");
        }
        if connections > 1 {
            bail!("More than one receiver is connected. Unplug the others and try again.");
        }
        let cid = match board.read().cid {
            0 => 7,
            c => c,
        };
        self.write(&p::pair_packet(cid))?;
        let start = Instant::now();
        while let Some(left) = PAIR_FOR.checked_sub(start.elapsed()) {
            board.publish(Some(DeviceEvent::Pairing { seconds_left: left.as_secs() as u32 }));
            thread::sleep(Duration::from_secs(1));
            match self.transact_simple(cmd::PAIR_STATE) {
                Ok(r) if r[6] == p::pair_state::PAIRED => {
                    self.lost = true;
                    return Ok(true);
                }
                Ok(r) if r[6] == p::pair_state::FAILED => return Ok(false),
                Ok(_) => {}
                Err(_) if self.lost => {
                    bail!("The receiver disconnected while pairing. Plug it back in and try again.")
                }
                // A dropped answer is asked again next second.
                Err(_) => {}
            }
        }
        Ok(false)
    }

    fn factory_reset(&mut self, board: &Board) -> Result<()> {
        self.zaopin_only()?;
        self.transact(&p::packet(cmd::FACTORY_RESET, 0, &[], None), RESET_TIMEOUT)?;
        thread::sleep(Duration::from_millis(500));
        self.load(board)
    }

    /// Full 6912-byte image of the active profile. Without `progress`, it's a backup hrtz saves by
    /// itself in the background, which the user didn't start and shouldn't see as something loading.
    fn read_image(&mut self, board: &Board, progress: bool) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(p::FLASH_SIZE);
        let step = 200;
        for a in (0..p::FLASH_SIZE).step_by(step) {
            let n = step.min(p::FLASH_SIZE - a);
            out.extend(self.read_flash(a, n)?);
            if progress {
                board.publish(Some(DeviceEvent::Progress { done: out.len(), total: p::FLASH_SIZE }));
            }
        }
        self.image.copy_from_slice(&out);
        self.refresh_settings(board);
        if !progress {
            board.publish(None);
        }
        Ok(out)
    }

    fn write_image(&mut self, board: &Board, current: &[u8], target: &[u8]) -> Result<usize> {
        self.zaopin_only()?;
        if target.len() != p::FLASH_SIZE {
            bail!("backup has {} bytes, expected {}", target.len(), p::FLASH_SIZE);
        }
        let plan = image::diff(current, target, p::MAX_CHUNK);
        let total = plan.len();
        for (i, (addr, bytes)) in plan.iter().enumerate() {
            self.write_flash(*addr, bytes)?;
            board.publish(Some(DeviceEvent::Progress { done: i + 1, total }));
        }
        for (addr, bytes) in &plan {
            if self.read_flash(*addr, bytes.len())? != *bytes {
                bail!("Verification failed at 0x{addr:04X}; the mouse did not keep the restored bytes.");
            }
        }
        self.refresh_settings(board);
        board.publish(None);
        Ok(total)
    }

    /// For a test report: whose it is, and how it's connected.
    fn report(&mut self, board: &Board) -> Result<serde_json::Value> {
        let s = board.read();
        Ok(match &self.atk {
            Some(a) => serde_json::json!({ "maker": "ATK", "cid": s.cid, "mid": s.mid, "receiver": a.receiver }),
            None => serde_json::Value::Null,
        })
    }

    /// Tell the mouse no configuration app is listening any more. An ATK wasn't told it was.
    fn goodbye(&mut self) {
        if self.atk.is_some() {
            return;
        }
        let _ = self.transact(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[0], None), Duration::from_millis(300));
    }
}

fn rand_bytes() -> [u8; 4] {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let mut x = t.as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    (x as u32).to_le_bytes()
}

#[cfg(test)]
mod descriptor_tests {
    use super::report_ids;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn the_settings_channel_is_told_by_its_report_id() {
        // snowyuonA's U2 Plus receiver: collection 4 (19-byte reports) and collection 5.
        let col4 = hex(
            "06 02 FF 09 02 A1 01 85 13 09 02 15 00 26 FF 00 75 08 95 13 81 00 09 02 15 00 26 FF 00 75 08 95 13 91 00 C0",
        );
        let col5 = hex(
            "06 02 FF 09 02 A1 01 85 08 09 02 15 00 26 FF 00 75 08 95 10 81 00 09 02 15 00 26 FF 00 75 08 95 10 91 00 C0",
        );
        assert_eq!(report_ids(&col4), vec![0x13]);
        assert_eq!(report_ids(&col5), vec![0x08]);
        // The report size of 8 bits (75 08) isn't a report ID.
        assert!(report_ids(&hex("75 08 95 10")).is_empty());
    }
}
