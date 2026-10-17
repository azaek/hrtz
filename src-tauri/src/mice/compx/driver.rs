//! A connected Zaopin mouse, through its receiver or its cable: one command at a time, the flash
//! image read in regions, and the status packets the mouse sends by itself.

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use hidapi::HidApi;

use super::image;
use super::models;
use super::protocol::{self as p, Packet, cmd, status};
use crate::mice::model::Settings;
use crate::mice::{Board, DeviceEvent, DeviceState, Driver, Family, Finder, Follow, Link, Reading};

/// Everything below this address is read on connect: config, buttons and shortcut slots.
pub(crate) const LOADED_REGION: usize = 0x300;
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
        Box::new(HidFinder { api: None, path: id.to_string() })
    }
}

/// One Zaopin receiver, or a mouse on its cable, through hidapi: the config collection at `path`.
pub struct HidFinder {
    api: Option<HidApi>,
    path: String,
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
    d.vendor_id() == p::VENDOR_ID && d.usage_page() == p::CONFIG_USAGE_PAGE && d.usage() == p::CONFIG_USAGE
}

impl Finder for HidFinder {
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>> {
        let path = self.path.clone();
        let api = self.api()?;
        api.refresh_devices()?;
        let info = api
            .device_list()
            .find(|d| is_config_interface(d) && d.path().to_string_lossy() == *path)
            .ok_or_else(|| anyhow!("no Zaopin receiver found"))?
            .clone();
        let dev = info.open_device(api).context("open HID device")?;
        let opened = Opened {
            link: Box::new(dev),
            // The vendor driver keeps at least 8 ms between writes; the receiver drops packets otherwise.
            gap: Duration::from_millis(8),
            #[cfg(windows)]
            port: crate::usb::port_of(&info.path().to_string_lossy()),
        };
        connect(opened, board)
    }

    fn count(&mut self) -> usize {
        let Ok(api) = self.api() else { return 1 };
        if api.refresh_devices().is_err() {
            return 1;
        }
        api.device_list().filter(|d| is_config_interface(d)).count()
    }
}

/// A receiver or a mouse on its cable, opened.
pub(crate) struct Opened {
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
    };
    let nonce: [u8; 4] = rand_bytes();
    let r = m.transact(&p::packet(cmd::HANDSHAKE, 0, &nonce, Some(8)), REPLY_TIMEOUT)?;
    let d = p::payload(&r);
    let (cid, mid, device_type) = (d[4], d[5], d[6]);
    board.set(|s| {
        *s = DeviceState {
            key: Some(format!("compx/{cid}-{mid}")),
            connected: true,
            model: Some(models::model_name(mid).to_string()),
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

/// A Zaopin mouse, met.
struct Connected {
    link: Box<dyn Link>,
    gap: Duration,
    /// What's known of the mouse's memory: the first `LOADED_REGION` bytes once loaded, all of it
    /// after a backup.
    image: Vec<u8>,
    pending_status: Vec<u8>,
    lost: bool,
}

impl Connected {
    fn refresh_settings(&mut self, board: &Board) {
        let decoded = image::decode(&self.image);
        board.set(|s| s.settings = Some(decoded));
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
            let r = self.transact(&pkt, REPLY_TIMEOUT)?;
            if r != pkt {
                bail!("The mouse rejected a write at 0x{a:04X}.");
            }
            self.image[a..a + chunk.len()].copy_from_slice(chunk);
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
            let s = board.read().settings.clone();
            if let Some(s) = s {
                board.publish(Some(DeviceEvent::Dpi { stage: s.dpi_stage.unwrap_or(0), dpi: s.dpi().unwrap_or(0) }));
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
        Ok(self.transact_simple(cmd::ONLINE)?[6] != 0)
    }

    /// Read everything the UI shows. Called on connect, on wake, and after a profile switch.
    fn load(&mut self, board: &Board) -> Result<()> {
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
            return Ok(Follow::default());
        }
        if !loaded {
            return Ok(Follow { load: true, ..Default::default() });
        }
        self.handle_status(board, bits)
    }

    fn apply(&mut self, board: &Board, new: &Settings) -> Result<()> {
        let caps = board.read().capabilities.clone().ok_or_else(|| anyhow!("unknown device"))?;
        image::validate(new, &self.image, &caps).map_err(|e| anyhow!(e))?;
        let target = image::encode(new, &self.image);
        for (addr, bytes) in image::diff(&self.image, &target, p::MAX_CHUNK) {
            self.write_flash(addr, &bytes)?;
        }
        self.refresh_settings(board);
        Ok(())
    }

    fn set_profile(&mut self, board: &Board, index: u8) -> Result<()> {
        if index > 3 {
            bail!("profile must be 1-4");
        }
        self.transact(&p::packet(cmd::SET_PROFILE, 0, &[index], None), REPLY_TIMEOUT)?;
        thread::sleep(Duration::from_millis(150));
        self.pending_status.clear();
        // The new profile's settings first (a fifth of a second), so the pop-up and the HUD say it
        // at once; then its key-combination slots, in the same job, so nothing writes to the
        // mouse in between. The firmware, long-range mode and battery don't change with a profile.
        let config = self.read_flash(0, p::CONFIG_REGION)?;
        self.image[..p::CONFIG_REGION].copy_from_slice(&config);
        board.set(|s| s.profile = Some(index));
        self.refresh_settings(board);
        board.publish(Some(DeviceEvent::Profile { index }));
        let slots = self.read_flash(p::CONFIG_REGION, LOADED_REGION - p::CONFIG_REGION)?;
        self.image[p::CONFIG_REGION..LOADED_REGION].copy_from_slice(&slots);
        self.refresh_settings(board);
        board.publish(None);
        Ok(())
    }

    fn set_long_range(&mut self, board: &Board, on: bool) -> Result<()> {
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

    /// Tell the mouse no configuration app is listening any more.
    fn goodbye(&mut self) {
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
