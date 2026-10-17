//! Device engine: one worker thread owns the HID handle, serialises requests, follows the mouse's
//! unsolicited status packets, and publishes a `DeviceState` snapshot after every change.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::{bounded, unbounded, RecvTimeoutError, Sender};
use hidapi::{HidApi, HidDevice};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;

use crate::battery::{Estimator, Saved};
use crate::battery_log::{BatteryLog, Entry};
use crate::protocol::{self as p, cmd, status, Packet};
use crate::settings::{self, Capabilities, Settings};

/// Everything below this address is read on connect: config, buttons and shortcut slots.
const LOADED_REGION: usize = 0x300;
const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
const RESET_TIMEOUT: Duration = Duration::from_millis(5000);
/// How often to ask the receiver whether the mouse is linked. Nothing announces sleep or wake, so
/// this is what the UI's awake/asleep status waits on. While the mouse sleeps the receiver answers
/// on its own, so the fast probe costs the mouse nothing.
const PROBE_AWAKE: Duration = Duration::from_millis(1000);
const PROBE_ASLEEP: Duration = Duration::from_millis(250);
/// How long the receiver is given to find a mouse to pair with, as in the vendor app.
const PAIR_FOR: Duration = Duration::from_secs(30);

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    /// Smoothed estimate (see `battery.rs`); this is what the UI shows.
    pub level: u8,
    /// What the mouse itself reports, which jumps while charging.
    pub raw_level: u8,
    pub charging: bool,
    pub millivolts: u16,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceState {
    /// Goes up with every change, so the UI can tell a stale copy from a fresh one: a reply and an
    /// event can arrive in either order.
    pub rev: u64,
    /// A receiver or wired mouse answered the handshake.
    pub connected: bool,
    /// The mouse itself is awake and linked (always true when wired).
    pub online: bool,
    pub model: Option<String>,
    pub cid: u8,
    pub mid: u8,
    pub device_type: u8,
    pub firmware: Option<String>,
    pub battery: Option<Battery>,
    pub profile: Option<u8>,
    pub long_range: Option<bool>,
    pub settings: Option<Settings>,
    pub capabilities: Option<Capabilities>,
    /// Where the receiver (or the mouse, on its cable) is plugged in.
    #[cfg(windows)]
    pub port: Option<crate::usb::UsbPort>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeviceEvent {
    Dpi { stage: u8, dpi: u32 },
    Polling { hz: u32 },
    Profile { index: u8 },
    Progress { done: usize, total: usize },
    /// The receiver is looking for a mouse to pair with, for this many more seconds.
    Pairing { seconds_left: u32 },
}

pub type Notify = Arc<dyn Fn(&DeviceState, Option<DeviceEvent>) + Send + Sync>;
type Job = Box<dyn FnOnce(&mut Worker) + Send>;

#[derive(Clone)]
pub struct Engine {
    jobs: Sender<Job>,
    pub state: Arc<RwLock<DeviceState>>,
    pub log: Arc<Mutex<BatteryLog>>,
}

impl Engine {
    /// `battery_file` keeps the battery estimate across restarts; `log_file` is the battery log.
    pub fn start(notify: Notify, battery_file: Option<PathBuf>, log_file: Option<PathBuf>) -> Self {
        let (tx, rx) = unbounded::<Job>();
        let state = Arc::new(RwLock::new(DeviceState::default()));
        let log = Arc::new(Mutex::new(BatteryLog::load(log_file)));
        let (shared, shared_log) = (state.clone(), log.clone());
        thread::Builder::new()
            .name("hrtz-device".into())
            .spawn(move || {
                let mut w = Worker::new(shared, notify, battery_file, shared_log);
                let mut next_probe = Instant::now();
                loop {
                    match rx.recv_timeout(Duration::from_millis(25)) {
                        Ok(job) => job(&mut w),
                        Err(RecvTimeoutError::Timeout) => w.drain_unsolicited(),
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    if Instant::now() >= next_probe {
                        next_probe = Instant::now() + w.housekeeping();
                    }
                }
            })
            .expect("spawn device thread");
        Self { jobs: tx, state, log }
    }

    /// Run `f` on the device thread and wait for its result.
    pub fn call<T: Send + 'static>(&self, f: impl FnOnce(&mut Worker) -> Result<T> + Send + 'static) -> Result<T> {
        let (tx, rx) = bounded(1);
        self.jobs
            .send(Box::new(move |w| {
                let _ = tx.send(f(w));
            }))
            .map_err(|_| anyhow!("device thread stopped"))?;
        rx.recv_timeout(Duration::from_secs(60)).map_err(|_| anyhow!("device did not respond in time"))?
    }

    pub fn snapshot(&self) -> DeviceState {
        self.state.read().clone()
    }
}

pub struct Worker {
    api: Option<HidApi>,
    dev: Option<HidDevice>,
    image: Vec<u8>,
    loaded: bool,
    pending_status: Vec<u8>,
    last_battery_poll: Instant,
    battery: Estimator,
    battery_file: Option<PathBuf>,
    log: Arc<Mutex<BatteryLog>>,
    state: Arc<RwLock<DeviceState>>,
    notify: Notify,
    /// Why the last connect attempt failed, so each reason is logged once.
    connect_error: Option<String>,
}

impl Worker {
    fn new(state: Arc<RwLock<DeviceState>>, notify: Notify, battery_file: Option<PathBuf>, log: Arc<Mutex<BatteryLog>>) -> Self {
        let saved = battery_file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| serde_json::from_slice::<Saved>(&b).ok());
        Self {
            api: HidApi::new().ok(),
            dev: None,
            image: vec![0xFF; p::FLASH_SIZE],
            loaded: false,
            pending_status: Vec::new(),
            last_battery_poll: Instant::now(),
            battery: Estimator::new(saved),
            battery_file,
            log,
            state,
            notify,
            connect_error: None,
        }
    }

    /// Add a row to the battery log if something changed since the last one (see `battery_log`).
    fn log_battery(&self) {
        let entry = {
            let s = self.state.read();
            let Some(b) = s.battery.as_ref().filter(|_| s.connected) else { return };
            Entry {
                t: SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                level: b.level,
                mv: b.millivolts,
                charging: b.charging,
                hz: s.settings.as_ref().map_or(0, |x| x.polling_hz),
                awake: s.online,
            }
        };
        self.log.lock().record(entry);
    }

    fn publish(&self, event: Option<DeviceEvent>) {
        let s = self.state.read().clone();
        (self.notify)(&s, event);
    }

    fn set(&self, f: impl FnOnce(&mut DeviceState)) {
        let mut s = self.state.write();
        let rev = s.rev;
        f(&mut s);
        s.rev = rev + 1;
    }

    // ---- connection lifecycle ------------------------------------------------------------

    /// Periodic work; returns how long until it should run again.
    fn housekeeping(&mut self) -> Duration {
        if self.dev.is_none() {
            match self.connect() {
                Ok(()) => self.connect_error = None,
                Err(e) => {
                    // Each new reason once, so a receiver that won't answer can be told from one
                    // that isn't plugged in.
                    let why = format!("{e:#}");
                    if self.connect_error.as_deref() != Some(why.as_str()) {
                        eprintln!("connect failed: {why}");
                        self.connect_error = Some(why);
                    }
                    self.disconnect();
                }
            }
            return Duration::from_millis(1500);
        }
        let was_online = self.state.read().online;
        match self.online() {
            Ok(true) => {
                if !was_online {
                    // Show the mouse as awake right away; its settings follow once they're read
                    // (about a second for the 768 bytes), in case they changed while it slept.
                    self.set(|s| s.online = true);
                    self.publish(None);
                    self.loaded = false;
                }
                if !self.loaded {
                    if let Err(e) = self.load() {
                        eprintln!("load failed: {e:#}");
                    }
                } else {
                    // Poll faster while charging so the estimate climbs smoothly.
                    let charging = self.state.read().battery.as_ref().is_some_and(|b| b.charging);
                    let every = if charging { Duration::from_secs(10) } else { Duration::from_secs(60) };
                    if self.last_battery_poll.elapsed() >= every {
                        let _ = self.refresh_battery();
                    }
                }
            }
            Ok(false) if was_online => {
                self.set(|s| s.online = false);
                self.publish(None);
                self.log_battery();
            }
            // Asleep: the log still notes it now and then, so time asleep isn't taken for a gap.
            Ok(false) => self.log_battery(),
            Err(_) => self.disconnect(),
        }
        if self.state.read().online { PROBE_AWAKE } else { PROBE_ASLEEP }
    }

    fn connect(&mut self) -> Result<()> {
        let api = match self.api.as_mut() {
            Some(api) => api,
            None => {
                self.api = Some(HidApi::new()?);
                self.api.as_mut().unwrap()
            }
        };
        api.refresh_devices()?;
        let info = api
            .device_list()
            .find(|d| d.vendor_id() == p::VENDOR_ID && d.usage_page() == p::CONFIG_USAGE_PAGE && d.usage() == p::CONFIG_USAGE)
            .ok_or_else(|| anyhow!("no Zaopin receiver found"))?
            .clone();
        let dev = info.open_device(api).context("open HID device")?;
        #[cfg(windows)]
        let port = crate::usb::port_of(&info.path().to_string_lossy());
        self.dev = Some(dev);
        let nonce: [u8; 4] = rand_bytes();
        let r = self.transact(&p::packet(cmd::HANDSHAKE, 0, &nonce, Some(8)), REPLY_TIMEOUT)?;
        let d = p::payload(&r);
        let (cid, mid, device_type) = (d[4], d[5], d[6]);
        self.set(|s| {
            *s = DeviceState {
                connected: true,
                model: Some(settings::model_name(mid).to_string()),
                cid,
                mid,
                device_type,
                capabilities: Some(settings::capabilities(mid, device_type)),
                #[cfg(windows)]
                port,
                ..Default::default()
            }
        });
        // Tells the mouse a configuration app is listening, which enables status notifications.
        let _ = self.transact(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[1], None), REPLY_TIMEOUT);
        self.publish(None);
        if self.online()? {
            self.load()?;
        }
        Ok(())
    }

    fn disconnect(&mut self) {
        let was = self.state.read().connected;
        self.dev = None;
        self.loaded = false;
        self.image.fill(0xFF);
        self.set(|s| *s = DeviceState::default());
        if was {
            self.publish(None);
        }
    }

    /// Read everything the UI shows. Called on connect, on wake, and after a profile switch.
    fn load(&mut self) -> Result<()> {
        let fw = self.transact_simple(cmd::VERSION)?;
        let profile = self.transact_simple(cmd::GET_PROFILE)?;
        let long_range = self.transact_simple(cmd::GET_LONG_RANGE).ok();
        let region = self.read_flash(0, LOADED_REGION)?;
        self.image[..LOADED_REGION].copy_from_slice(&region);
        self.loaded = true;
        self.set(|s| {
            s.online = true;
            // Same rendering as the vendor app: hex major.minor, e.g. 0x0217 -> "2.17".
            s.firmware = Some(format!("{:x}.{:02x}", fw[6], fw[7]));
            s.profile = (profile[5] > 0).then_some(profile[6]);
            s.long_range = long_range.map(|r| r[6] != 0);
        });
        self.refresh_settings();
        self.refresh_battery()?;
        Ok(())
    }

    fn refresh_settings(&mut self) {
        let decoded = settings::decode(&self.image);
        self.set(|s| s.settings = Some(decoded));
        self.log_battery();
    }

    fn refresh_battery(&mut self) -> Result<()> {
        let r = self.transact_simple(cmd::BATTERY)?;
        let d = p::payload(&r);
        let (raw_level, charging, millivolts) = (d[0].min(100), d[1] != 0, u16::from_be_bytes([d[2], d[3]]));
        let before = self.battery.saved().map(|s| s.level);
        let level = self.battery.update(raw_level, charging, millivolts, SystemTime::now());
        if before != Some(level) {
            if let (Some(f), Some(saved)) = (&self.battery_file, self.battery.saved()) {
                let _ = std::fs::write(f, serde_json::to_vec(&saved).unwrap_or_default());
            }
        }
        let b = Battery { level, raw_level, charging, millivolts };
        self.last_battery_poll = Instant::now();
        self.set(|s| s.battery = Some(b));
        self.publish(None);
        self.log_battery();
        Ok(())
    }

    pub fn online(&mut self) -> Result<bool> {
        Ok(self.transact_simple(cmd::ONLINE)?[6] != 0)
    }

    fn require_online(&mut self) -> Result<()> {
        if self.dev.is_none() {
            bail!("No receiver connected. Plug in the receiver or connect the mouse with its cable.");
        }
        if !self.online()? {
            bail!("The mouse is asleep. Move it to wake it, then try again.");
        }
        if !self.loaded {
            self.load()?;
        }
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
        let dev = self.dev.as_ref().ok_or_else(|| anyhow!("not connected"))?;
        if let Err(e) = dev.write(pkt) {
            self.disconnect();
            bail!("Lost the receiver: {e}");
        }
        // The vendor driver keeps at least 8 ms between writes; the receiver drops packets otherwise.
        thread::sleep(Duration::from_millis(8));
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Packet>> {
        let dev = self.dev.as_ref().ok_or_else(|| anyhow!("not connected"))?;
        let mut buf = [0u8; p::PACKET_LEN + 1];
        let n = match dev.read_timeout(&mut buf, timeout.as_millis().max(1) as i32) {
            Ok(n) => n,
            Err(e) => {
                self.disconnect();
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

    pub fn read_flash(&mut self, addr: usize, len: usize) -> Result<Vec<u8>> {
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

    // ---- unsolicited status ---------------------------------------------------------------

    fn drain_unsolicited(&mut self) {
        if self.dev.is_none() {
            return;
        }
        while let Ok(Some(r)) = self.read(Duration::from_millis(1)) {
            if r[1] == cmd::STATUS_CHANGED {
                self.pending_status.push(r[6]);
            }
        }
        let bits = self.pending_status.drain(..).fold(0u8, |a, b| a | b);
        if bits != 0 {
            if let Err(e) = self.handle_status(bits) {
                eprintln!("status handling failed: {e:#}");
            }
        }
    }

    fn handle_status(&mut self, bits: u8) -> Result<()> {
        if !self.loaded {
            return self.load();
        }
        if bits & status::PROFILE != 0 {
            self.load()?;
            let index = self.state.read().profile.unwrap_or(0);
            self.publish(Some(DeviceEvent::Profile { index }));
            return Ok(());
        }
        if bits & status::DPI != 0 {
            let r = self.read_flash(0x02, 4)?;
            self.image[0x02..0x06].copy_from_slice(&r);
            self.refresh_settings();
            let s = self.state.read().settings.clone();
            if let Some(s) = s {
                let dpi = s.stages.get(s.dpi_stage as usize).map_or(0, |st| st.dpi);
                self.publish(Some(DeviceEvent::Dpi { stage: s.dpi_stage, dpi }));
            }
        }
        if bits & status::POLLING != 0 {
            let r = self.read_flash(0x00, 2)?;
            self.image[0x00..0x02].copy_from_slice(&r);
            self.refresh_settings();
            let hz = self.state.read().settings.as_ref().map_or(0, |s| s.polling_hz);
            self.publish(Some(DeviceEvent::Polling { hz }));
        }
        if bits & status::BATTERY != 0 {
            self.refresh_battery()?;
        }
        Ok(())
    }

    // ---- operations used by commands ------------------------------------------------------

    pub fn apply_settings(&mut self, new: &Settings) -> Result<Settings> {
        self.require_online()?;
        let caps = self.state.read().capabilities.clone().ok_or_else(|| anyhow!("unknown device"))?;
        settings::validate(new, &settings::decode(&self.image), &caps).map_err(|e| anyhow!(e))?;
        let target = settings::encode(new, &self.image);
        for (addr, bytes) in settings::diff(&self.image, &target, p::MAX_CHUNK) {
            self.write_flash(addr, &bytes)?;
        }
        self.refresh_settings();
        self.publish(None);
        Ok(self.state.read().settings.clone().unwrap())
    }

    pub fn set_profile(&mut self, index: u8) -> Result<()> {
        self.require_online()?;
        if index > 3 {
            bail!("profile must be 1-4");
        }
        self.transact(&p::packet(cmd::SET_PROFILE, 0, &[index], None), REPLY_TIMEOUT)?;
        thread::sleep(Duration::from_millis(150));
        self.pending_status.clear();
        self.load()?;
        self.publish(Some(DeviceEvent::Profile { index }));
        Ok(())
    }

    pub fn set_long_range(&mut self, on: bool) -> Result<()> {
        self.require_online()?;
        let mut data = [0u8; 10];
        data[0] = on as u8;
        self.transact(&p::packet(cmd::SET_LONG_RANGE, 0, &data, None), REPLY_TIMEOUT)?;
        let r = self.transact_simple(cmd::GET_LONG_RANGE)?;
        self.set(|s| s.long_range = Some(r[6] != 0));
        self.publish(None);
        Ok(())
    }

    /// Put the receiver into pairing for any mouse of this brand, as the vendor app does, and wait
    /// up to 30 s for one, asking how it's going once a second (`DeviceEvent::Pairing`). This holds
    /// the device thread, so housekeeping waits meanwhile, as the vendor pauses its own timers.
    /// Returns whether a mouse paired; the receiver is then met again, in case it's another model.
    pub fn pair(&mut self) -> Result<bool> {
        if self.dev.is_none() {
            bail!("No receiver connected. Plug in the receiver.");
        }
        if self.state.read().capabilities.as_ref().is_some_and(|c| c.wired) {
            bail!("Pairing works over the receiver only. Unplug the mouse's cable first.");
        }
        if self.receivers() > 1 {
            bail!("More than one receiver is connected. Unplug the others and try again.");
        }
        let cid = match self.state.read().cid {
            0 => 7,
            c => c,
        };
        self.write(&p::pair_packet(cid))?;
        let start = Instant::now();
        while let Some(left) = PAIR_FOR.checked_sub(start.elapsed()) {
            self.publish(Some(DeviceEvent::Pairing { seconds_left: left.as_secs() as u32 }));
            thread::sleep(Duration::from_secs(1));
            match self.transact_simple(cmd::PAIR_STATE) {
                Ok(r) if r[6] == p::pair_state::PAIRED => {
                    self.disconnect();
                    return Ok(true);
                }
                Ok(r) if r[6] == p::pair_state::FAILED => return Ok(false),
                Ok(_) => {}
                Err(_) if self.dev.is_none() => bail!("The receiver disconnected while pairing. Plug it back in and try again."),
                // A dropped answer is asked again next second.
                Err(_) => {}
            }
        }
        Ok(false)
    }

    /// Zaopin receivers, and mice on their cables, plugged in now.
    fn receivers(&mut self) -> usize {
        let Some(api) = self.api.as_mut() else { return 1 };
        if api.refresh_devices().is_err() {
            return 1;
        }
        api.device_list().filter(|d| d.vendor_id() == p::VENDOR_ID && d.usage_page() == p::CONFIG_USAGE_PAGE && d.usage() == p::CONFIG_USAGE).count()
    }

    pub fn factory_reset(&mut self) -> Result<()> {
        self.require_online()?;
        self.transact(&p::packet(cmd::FACTORY_RESET, 0, &[], None), RESET_TIMEOUT)?;
        thread::sleep(Duration::from_millis(500));
        self.load()?;
        self.publish(None);
        Ok(())
    }

    /// Full 6912-byte image of the active profile, reporting progress.
    pub fn read_full_image(&mut self) -> Result<Vec<u8>> {
        self.read_image(true)
    }

    /// The whole memory, with no progress bar: for a backup hrtz saves by itself in the
    /// background, which the user didn't start and shouldn't see as something loading.
    pub fn read_full_image_quietly(&mut self) -> Result<Vec<u8>> {
        self.read_image(false)
    }

    fn read_image(&mut self, progress: bool) -> Result<Vec<u8>> {
        self.require_online()?;
        let mut out = Vec::with_capacity(p::FLASH_SIZE);
        let step = 200;
        for a in (0..p::FLASH_SIZE).step_by(step) {
            let n = step.min(p::FLASH_SIZE - a);
            out.extend(self.read_flash(a, n)?);
            if progress {
                self.publish(Some(DeviceEvent::Progress { done: out.len(), total: p::FLASH_SIZE }));
            }
        }
        self.image.copy_from_slice(&out);
        self.loaded = true;
        self.refresh_settings();
        if !progress {
            self.publish(None);
        }
        Ok(out)
    }

    /// Write only the bytes that differ from `target`, then read them back.
    pub fn restore_image(&mut self, target: &[u8]) -> Result<usize> {
        let current = self.read_full_image()?;
        self.write_image(&current, target)
    }

    /// Writes `target` over the mouse's memory, which held `current` when just read: only the
    /// chunks that differ, each checked after. Returns how many chunks were written.
    pub fn write_image(&mut self, current: &[u8], target: &[u8]) -> Result<usize> {
        if target.len() != p::FLASH_SIZE {
            bail!("backup has {} bytes, expected {}", target.len(), p::FLASH_SIZE);
        }
        let plan = settings::diff(current, target, p::MAX_CHUNK);
        let total = plan.len();
        for (i, (addr, bytes)) in plan.iter().enumerate() {
            self.write_flash(*addr, bytes)?;
            self.publish(Some(DeviceEvent::Progress { done: i + 1, total }));
        }
        for (addr, bytes) in &plan {
            if self.read_flash(*addr, bytes.len())? != *bytes {
                bail!("Verification failed at 0x{addr:04X}; the mouse did not keep the restored bytes.");
            }
        }
        self.refresh_settings();
        self.publish(None);
        Ok(total)
    }

    pub fn snapshot(&self) -> DeviceState {
        self.state.read().clone()
    }

    /// Tell the mouse no configuration app is listening any more.
    pub fn goodbye(&mut self) {
        if self.dev.is_some() {
            let _ = self.transact(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[0], None), Duration::from_millis(300));
        }
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
