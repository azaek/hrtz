//! The engine: one thread owns a mouse's connection and runs its driver, one request at a time.
//! It finds the mouse, follows it as it sleeps and wakes, keeps the battery estimate and log, and
//! publishes a `DeviceState` snapshot after every change. What's particular to a mouse's protocol
//! is in its driver (`mice::compx` for Zaopin).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Result, anyhow, bail};
use crossbeam_channel::{RecvTimeoutError, Sender, bounded, unbounded};
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};

use super::model::{Capabilities, Settings};
use super::{Driver, Finder, Follow};
use crate::battery::{Estimator, Saved};
use crate::battery_log::{BatteryLog, Entry};

/// How often to ask the receiver whether the mouse is linked. Nothing announces sleep or wake, so
/// this is what the UI's awake/asleep status waits on. While the mouse sleeps the receiver answers
/// on its own, so the fast probe costs the mouse nothing.
const PROBE_AWAKE: Duration = Duration::from_millis(1000);
const PROBE_ASLEEP: Duration = Duration::from_millis(250);
/// How often to look for the mouse while there's none.
const FIND_EVERY: Duration = Duration::from_millis(1500);

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    /// Smoothed estimate (see `battery.rs`); this is what the UI shows.
    pub level: u8,
    /// What the mouse itself reports, which jumps while charging.
    pub raw_level: u8,
    pub charging: bool,
    pub millivolts: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceState {
    /// Goes up with every change, so the UI can tell a stale copy from a fresh one: a reply and an
    /// event can arrive in either order.
    pub rev: u64,
    /// Who the mouse is: the same across connections and restarts, so what hrtz keeps for it
    /// (prefs, battery history, backups) is filed under it. Set once it's met.
    pub key: Option<String>,
    /// A receiver or wired mouse answered the handshake.
    pub connected: bool,
    /// The mouse itself is awake and linked (always true when wired).
    pub online: bool,
    pub model: Option<String>,
    pub cid: u8,
    pub mid: u8,
    pub device_type: u8,
    /// Which of its model's looks it is, where the mouse says: a Logitech's colour (its extended
    /// model ID), for its picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<u8>,
    pub firmware: Option<String>,
    pub battery: Option<Battery>,
    pub profile: Option<u8>,
    pub long_range: Option<bool>,
    /// Software drives the mouse live instead of its own memory: a Logitech in host mode, G HUB's
    /// doing with its On-board memory mode off. What's set here applies once it lets go.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub host_mode: bool,
    pub settings: Option<Settings>,
    pub capabilities: Option<Capabilities>,
    /// Where the receiver (or the mouse, on its cable) is plugged in.
    #[cfg(windows)]
    pub port: Option<crate::usb::UsbPort>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeviceEvent {
    Dpi {
        stage: u8,
        dpi: u32,
    },
    Polling {
        hz: u32,
    },
    Profile {
        index: u8,
    },
    Progress {
        done: usize,
        total: usize,
    },
    /// The receiver is looking for a mouse to pair with, for this many more seconds.
    Pairing {
        seconds_left: u32,
    },
}

pub type Notify = Arc<dyn Fn(&DeviceState, Option<DeviceEvent>) + Send + Sync>;
type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// The mouse as the UI sees it, and the way to tell the UI: drivers post here as they read.
pub struct Board {
    state: Arc<RwLock<DeviceState>>,
    notify: Notify,
}

impl Board {
    /// Change the state; its `rev` goes up, across every engine, so a mouse that moves to another
    /// connection (unplugged and plugged back in) doesn't look older than it was.
    pub fn set(&self, f: impl FnOnce(&mut DeviceState)) {
        static REV: AtomicU64 = AtomicU64::new(0);
        let mut s = self.state.write();
        f(&mut s);
        s.rev = REV.fetch_add(1, Ordering::Relaxed) + 1;
    }

    /// Tell the UI the state as it is now, and what just happened, if anything.
    pub fn publish(&self, event: Option<DeviceEvent>) {
        let s = self.state.read().clone();
        (self.notify)(&s, event);
    }

    pub fn read(&self) -> RwLockReadGuard<'_, DeviceState> {
        self.state.read()
    }
}

#[derive(Clone)]
pub struct Engine {
    jobs: Sender<Job>,
    pub state: Arc<RwLock<DeviceState>>,
    pub log: Arc<Mutex<BatteryLog>>,
}

impl Engine {
    /// Start the thread, finding the mouse through `finder`. Each mouse's battery estimate and log
    /// are kept in `data_dir` (`battery/`). The thread ends once every handle is dropped, telling
    /// the UI its mouse went.
    pub fn start(finder: Box<dyn Finder>, notify: Notify, data_dir: Option<PathBuf>) -> Self {
        let (tx, rx) = unbounded::<Job>();
        let state = Arc::new(RwLock::new(DeviceState::default()));
        let log = Arc::new(Mutex::new(BatteryLog::load(None)));
        let (shared, shared_log) = (state.clone(), log.clone());
        thread::Builder::new()
            .name("hrtz-device".into())
            .spawn(move || {
                let mut w = Worker::new(finder, shared, notify, data_dir, shared_log);
                let mut next_probe = Instant::now();
                loop {
                    match rx.recv_timeout(Duration::from_millis(25)) {
                        Ok(job) => job(&mut w),
                        Err(RecvTimeoutError::Timeout) => w.drain_unsolicited(),
                        Err(RecvTimeoutError::Disconnected) => {
                            w.disconnect();
                            break;
                        }
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
    finder: Box<dyn Finder>,
    driver: Option<Box<dyn Driver>>,
    /// The settings are read since the mouse connected or last woke.
    loaded: bool,
    board: Board,
    last_battery_poll: Instant,
    battery: Estimator,
    battery_file: Option<PathBuf>,
    log: Arc<Mutex<BatteryLog>>,
    /// Where each mouse's battery files go, and whose are loaded.
    data_dir: Option<PathBuf>,
    battery_key: Option<String>,
    /// Why the last connect attempt failed, so each reason is logged once.
    connect_error: Option<String>,
}

impl Worker {
    pub(crate) fn new(
        finder: Box<dyn Finder>,
        state: Arc<RwLock<DeviceState>>,
        notify: Notify,
        data_dir: Option<PathBuf>,
        log: Arc<Mutex<BatteryLog>>,
    ) -> Self {
        Self {
            finder,
            driver: None,
            loaded: false,
            board: Board { state, notify },
            last_battery_poll: Instant::now(),
            battery: Estimator::new(None),
            battery_file: None,
            log,
            data_dir,
            battery_key: None,
            connect_error: None,
        }
    }

    /// Use mouse `key`'s battery estimate and log. The files from before several mice (one
    /// estimate, one log) are copied to the first Zaopin's: left in place, an older hrtz still
    /// finds them.
    pub(crate) fn battery_of(&mut self, key: &str) {
        let Some(dir) = self.data_dir.clone() else { return };
        if self.battery_key.as_deref() == Some(key) {
            return;
        }
        let (estimate, log) = battery_files(&dir, key);
        if key.starts_with("compx/") && !estimate.exists() && !log.exists() {
            let _ = std::fs::create_dir_all(dir.join("battery"));
            let _ = std::fs::copy(dir.join("battery.json"), &estimate);
            let _ = std::fs::copy(dir.join("battery_log.jsonl"), &log);
        }
        let saved = std::fs::read(&estimate).ok().and_then(|b| serde_json::from_slice::<Saved>(&b).ok());
        self.battery = Estimator::new(saved);
        self.battery_file = Some(estimate);
        *self.log.lock() = BatteryLog::load(Some(log));
        self.battery_key = Some(key.to_string());
    }

    /// Run `f` on the driver; if the connection was lost meanwhile, let go of it.
    fn drive<T>(&mut self, f: impl FnOnce(&mut dyn Driver, &Board) -> Result<T>) -> Result<T> {
        let d = self.driver.as_deref_mut().ok_or_else(|| anyhow!("not connected"))?;
        let r = f(d, &self.board);
        if self.driver.as_ref().is_some_and(|d| d.lost()) {
            self.disconnect();
        }
        r
    }

    /// Add a row to the battery log if something changed since the last one (see `battery_log`).
    fn log_battery(&self) {
        let entry = {
            let s = self.board.read();
            let Some(b) = s.battery.as_ref().filter(|_| s.connected) else { return };
            Entry {
                t: SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
                level: b.level,
                mv: b.millivolts,
                charging: b.charging,
                hz: s.settings.as_ref().and_then(|x| x.polling_hz).unwrap_or(0),
                awake: s.online,
            }
        };
        self.log.lock().record(entry);
    }

    // ---- connection lifecycle ------------------------------------------------------------

    /// Periodic work; returns how long until it should run again.
    pub(crate) fn housekeeping(&mut self) -> Duration {
        if self.driver.is_none() {
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
            return FIND_EVERY;
        }
        let was_online = self.board.read().online;
        match self.drive(|d, _| d.online()) {
            Ok(true) => {
                if !was_online {
                    // Show the mouse as awake right away; its settings follow once they're read
                    // (about a second), in case they changed while it slept.
                    self.board.set(|s| s.online = true);
                    self.board.publish(None);
                    self.loaded = false;
                }
                if !self.loaded {
                    if let Err(e) = self.load() {
                        eprintln!("load failed: {e:#}");
                    }
                } else {
                    // Poll faster while charging so the estimate climbs smoothly.
                    let charging = self.board.read().battery.as_ref().is_some_and(|b| b.charging);
                    let every = if charging { Duration::from_secs(10) } else { Duration::from_secs(60) };
                    if self.last_battery_poll.elapsed() >= every {
                        let _ = self.refresh_battery();
                    }
                }
            }
            Ok(false) if was_online => {
                self.board.set(|s| s.online = false);
                self.board.publish(None);
                self.log_battery();
            }
            // Asleep: the log still notes it now and then, so time asleep isn't taken for a gap.
            Ok(false) => self.log_battery(),
            Err(_) => self.disconnect(),
        }
        if self.board.read().online { PROBE_AWAKE } else { PROBE_ASLEEP }
    }

    fn connect(&mut self) -> Result<()> {
        let driver = self.finder.find(&self.board)?;
        self.driver = Some(driver);
        self.loaded = false;
        let key = self.board.read().key.clone();
        if let Some(key) = key {
            self.battery_of(&key);
        }
        if self.drive(|d, _| d.online())? {
            self.load()?;
        }
        Ok(())
    }

    /// Let go of the connection. The state keeps who the mouse was, so the UI knows which went.
    fn disconnect(&mut self) {
        let (was, key) = {
            let s = self.board.read();
            (s.connected, s.key.clone())
        };
        self.driver = None;
        self.loaded = false;
        self.board.set(|s| *s = DeviceState { key, ..DeviceState::default() });
        if was {
            self.board.publish(None);
        }
    }

    /// Read everything the UI shows. Called on connect, on wake, and after a profile switch.
    fn load(&mut self) -> Result<()> {
        self.drive(|d, b| d.load(b))?;
        self.loaded = true;
        self.log_battery();
        self.refresh_battery()
    }

    fn refresh_battery(&mut self) -> Result<()> {
        if self.board.read().capabilities.as_ref().is_some_and(|c| !c.battery) {
            return Ok(());
        }
        let r = self.drive(|d, _| d.battery())?;
        let before = self.battery.saved().map(|s| s.level);
        let mv = if r.by_voltage { r.millivolts } else { 0 };
        let level = self.battery.update(r.level, r.charging, mv, SystemTime::now());
        if before != Some(level)
            && let (Some(f), Some(saved)) = (&self.battery_file, self.battery.saved())
        {
            let _ = std::fs::write(f, serde_json::to_vec(&saved).unwrap_or_default());
        }
        let b = Battery { level, raw_level: r.level, charging: r.charging, millivolts: r.millivolts };
        self.last_battery_poll = Instant::now();
        self.board.set(|s| s.battery = Some(b));
        self.board.publish(None);
        self.log_battery();
        Ok(())
    }

    fn require_online(&mut self) -> Result<()> {
        if self.driver.is_none() {
            bail!("No receiver connected. Plug in the receiver or connect the mouse with its cable.");
        }
        if !self.drive(|d, _| d.online())? {
            bail!("The mouse is asleep. Move it to wake it, then try again.");
        }
        if !self.loaded {
            self.load()?;
        }
        Ok(())
    }

    /// Take in what the mouse reported by itself: a button pressed on it, a battery change.
    pub(crate) fn drain_unsolicited(&mut self) {
        if self.driver.is_none() {
            return;
        }
        let loaded = self.loaded;
        let done = self.drive(|d, b| d.changes(b, loaded)).and_then(|follow: Follow| {
            if follow.load {
                return self.load();
            }
            if follow.settings {
                self.log_battery();
            }
            if follow.battery { self.refresh_battery() } else { Ok(()) }
        });
        if let Err(e) = done {
            eprintln!("status handling failed: {e:#}");
        }
    }

    // ---- operations used by commands ------------------------------------------------------

    pub fn apply_settings(&mut self, new: &Settings) -> Result<Settings> {
        self.require_online()?;
        self.drive(|d, b| d.apply(b, new))?;
        self.log_battery();
        self.board.publish(None);
        Ok(self.board.read().settings.clone().unwrap())
    }

    pub fn set_profile(&mut self, index: u8) -> Result<()> {
        self.require_online()?;
        self.drive(|d, b| d.set_profile(b, index))?;
        self.log_battery();
        Ok(())
    }

    pub fn set_long_range(&mut self, on: bool) -> Result<()> {
        self.require_online()?;
        self.drive(|d, b| d.set_long_range(b, on))
    }

    /// Pair a mouse with the receiver: up to 30 s, with `DeviceEvent::Pairing` counting down.
    /// Returns whether one paired; the receiver is then met again, in case it's another model.
    pub fn pair(&mut self) -> Result<bool> {
        if self.driver.is_none() {
            bail!("No receiver connected. Plug in the receiver.");
        }
        let connections = self.finder.count();
        self.drive(|d, b| d.pair(b, connections))
    }

    pub fn factory_reset(&mut self) -> Result<()> {
        self.require_online()?;
        self.drive(|d, b| d.factory_reset(b))?;
        self.loaded = true;
        self.log_battery();
        self.refresh_battery()?;
        self.board.publish(None);
        Ok(())
    }

    /// The mouse's whole memory, with a progress bar.
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
        let image = self.drive(|d, b| d.read_image(b, progress))?;
        self.loaded = true;
        self.log_battery();
        Ok(image)
    }

    /// Write only the bytes that differ from `target`, then read them back.
    pub fn restore_image(&mut self, target: &[u8]) -> Result<usize> {
        let current = self.read_full_image()?;
        self.write_image(&current, target)
    }

    /// Writes `target` over the mouse's memory, which held `current` when just read: only the
    /// chunks that differ, each checked after. Returns how many chunks were written.
    pub fn write_image(&mut self, current: &[u8], target: &[u8]) -> Result<usize> {
        let written = self.drive(|d, b| d.write_image(b, current, target))?;
        self.log_battery();
        Ok(written)
    }

    pub fn snapshot(&self) -> DeviceState {
        self.board.read().clone()
    }

    /// Tell the mouse no configuration app is listening any more.
    pub fn goodbye(&mut self) {
        if let Some(d) = self.driver.as_mut() {
            d.goodbye();
        }
    }
}

/// Mouse `key`'s battery estimate and log, in the data folder: `battery/compx-7-4.json` and `.jsonl`.
pub fn battery_files(data_dir: &Path, key: &str) -> (PathBuf, PathBuf) {
    let name: String = key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let dir = data_dir.join("battery");
    (dir.join(format!("{name}.json")), dir.join(format!("{name}.jsonl")))
}
