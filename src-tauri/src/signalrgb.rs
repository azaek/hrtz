//! The SignalRGB link (hrtz-notes/SIGNALRGB.md): SignalRGB sets a mouse's DPI light with the rest
//! of the PC's lighting. hrtz's add-on for SignalRGB (`extras/signalrgb/hrtz.js`, copied into
//! SignalRGB's plugins folder from the Sensor page) shows each Zaopin with a light as a one-LED
//! device, and sends hrtz the colour SignalRGB paints on it, 30 times a second, over localhost UDP.
//! hrtz turns that stream into as few writes as it can:
//!
//! - a colour that holds for `SETTLE` becomes every stage's colour, on a steady light;
//! - one colour fading to near black and back (SignalRGB's Pulse) sets the light breathing in it,
//!   at the mouse's nearest speed;
//! - black turns the light off;
//! - anything else (an animated effect) leaves the light as it is.
//!
//! Each change is a write to the mouse's memory, whose endurance isn't known: at most one change
//! every `MIN_GAP` and `DAILY_CAP` a day, per mouse. The light as the user had it is kept in
//! `signalrgb.json` before the first change, and put back when SignalRGB quits or the link is
//! turned off. hrtz listens on localhost only, and only while the link is on.

use std::collections::{HashMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::mice::Hub;
use crate::mice::model::{DpiLight, LightEffect, Settings};

/// The add-on, as this hrtz has it.
pub const ADDON: &str = include_str!("../../extras/signalrgb/hrtz.js");
const ADDON_FILE: &str = "hrtz.js";
const ADDR: &str = "127.0.0.1:17421";
/// A colour held this long is meant: switching effects holds a stale frame for about 0.4 s.
const SETTLE: Duration = Duration::from_millis(1000);
/// What's kept of each stream: two pulses and more at the 2.1 s SignalRGB's Pulse takes.
const WINDOW: Duration = Duration::from_secs(6);
const MIN_GAP: Duration = Duration::from_secs(3);
const DAILY_CAP: usize = 500;
/// SignalRGB asks for mice every 2 s while it runs with the add-on.
const ASKED_WITHIN: Duration = Duration::from_secs(5);
/// A mouse SignalRGB sent a frame for this recently is being set by it.
const FRAMES_WITHIN: Duration = Duration::from_secs(2);
/// Seconds per breath at each of the mouse's breathing speeds, 1 to 5. Timed by eye on the Z1 PRO
/// MAX for 1 and 2 (2 and 3 breaths in 15 s, 2026-11-10); 3 to 5 assume one breath more per step,
/// which fits none of them being as fast as SignalRGB's Pulse (2.1 s).
const BREATH_S: [f32; 5] = [7.5, 5.0, 3.75, 3.0, 2.5];

/// How the link stands, for the Sensor page.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The link is turned on.
    pub on: bool,
    /// SignalRGB is installed: its folder in Documents is there.
    pub installed: bool,
    pub addon: Addon,
    /// SignalRGB asked for mice lately: it's running, with the add-on loaded.
    pub connected: bool,
    /// The mice whose light SignalRGB is setting now (keys).
    pub driving: Vec<String>,
    /// Why hrtz can't listen for SignalRGB.
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Addon {
    #[default]
    Missing,
    /// As this hrtz has it.
    Current,
    /// An older or newer copy.
    Outdated,
}

enum Msg {
    On(bool),
    /// Look at the add-on's file again.
    Look,
}

pub struct SignalRgb {
    control: Sender<Msg>,
    status: Arc<RwLock<Status>>,
}

impl SignalRgb {
    /// Start the link's thread, on or off; `changed` is told whenever the status changes. The light
    /// as each user had it is kept in `data_dir`.
    pub fn start(hub: Arc<Hub>, data_dir: &Path, on: bool, changed: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = unbounded();
        let status = Arc::new(RwLock::new(Status::default()));
        let file = data_dir.join("signalrgb.json");
        let kept = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let link = Link {
            hub,
            file,
            kept,
            on,
            ids: Vec::new(),
            streams: HashMap::new(),
            writes: HashMap::new(),
            asked_at: None,
            sockets: None,
            tried_at: None,
            error: None,
            addon: Addon::Missing,
            installed: false,
        };
        let shared = status.clone();
        thread::Builder::new()
            .name("hrtz-signalrgb".into())
            .spawn(move || link.run(&rx, &shared, &changed))
            .expect("spawn signalrgb");
        Self { control: tx, status }
    }

    pub fn set_on(&self, on: bool) {
        let _ = self.control.send(Msg::On(on));
    }

    pub fn status(&self) -> Status {
        self.status.read().clone()
    }

    /// Copy the add-on into SignalRGB's plugins folder, over an older one. SignalRGB loads it when
    /// it next starts.
    pub fn install(&self) -> Result<()> {
        let dir = signalrgb_dir().ok_or_else(|| anyhow!("SignalRGB isn't installed on this PC."))?.join("Plugins");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(ADDON_FILE), ADDON)?;
        // SignalRGB loads a plugin from the `.mjs` copy it makes of the `.js`: an old copy goes,
        // so the new add-on is the one it loads.
        let _ = std::fs::remove_file(dir.join(ADDON_FILE).with_extension("mjs"));
        let _ = self.control.send(Msg::Look);
        Ok(())
    }
}

/// SignalRGB's folder in Documents, which it makes when first run; `None` without it.
fn signalrgb_dir() -> Option<PathBuf> {
    Some(documents()?.join("WhirlwindFX")).filter(|d| d.is_dir())
}

/// The Documents folder, wherever it's been moved to (OneDrive moves it).
#[cfg(windows)]
fn documents() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: the path the shell returns is read, then freed once, as its documentation asks.
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let path = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const _));
        path
    }
}

#[cfg(not(windows))]
fn documents() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents"))
}

fn addon_state() -> Addon {
    let Some(file) = signalrgb_dir().map(|d| d.join("Plugins").join(ADDON_FILE)) else { return Addon::Missing };
    match std::fs::read_to_string(file) {
        Ok(s) if s == ADDON => Addon::Current,
        Ok(_) => Addon::Outdated,
        Err(_) => Addon::Missing,
    }
}

// ---- what SignalRGB paints --------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Steady([u8; 3]),
    /// The colour, and the mouse's breathing speed nearest SignalRGB's.
    Breathing([u8; 3], u8),
    Off,
}

#[derive(Default)]
struct Stream {
    frames: VecDeque<(Instant, [u8; 3])>,
    /// What the light was last set to, or found to be already.
    applied: Option<Target>,
    last_try: Option<Instant>,
}

impl Stream {
    fn push(&mut self, t: Instant, colour: [u8; 3]) {
        self.frames.push_back((t, colour));
        while self.frames.front().is_some_and(|(at, _)| t.duration_since(*at) > WINDOW) {
            self.frames.pop_front();
        }
    }
}

fn value(c: [u8; 3]) -> u8 {
    c[0].max(c[1]).max(c[2])
}

fn near(a: [u8; 3], b: [u8; 3], by: u8) -> bool {
    (0..3).all(|i| a[i].abs_diff(b[i]) <= by)
}

/// The same finding: a pulse's brightest frame differs a little from one pulse to the next.
fn same(a: Target, b: Target) -> bool {
    match (a, b) {
        (Target::Off, Target::Off) => true,
        (Target::Steady(x), Target::Steady(y)) => near(x, y, 2),
        (Target::Breathing(x, s), Target::Breathing(y, t)) => s == t && near(x, y, 16),
        _ => false,
    }
}

/// What SignalRGB is painting, from the frames kept: `None` while it's changing in a way the light
/// can't follow, or has stopped sending.
fn classify(frames: &VecDeque<(Instant, [u8; 3])>, now: Instant) -> Option<Target> {
    let &(first, _) = frames.front()?;
    let &(last_at, last) = frames.back()?;
    if now.duration_since(first) < SETTLE || now.duration_since(last_at) > Duration::from_millis(250) {
        return None;
    }
    if frames.iter().filter(|(t, _)| now.duration_since(*t) <= SETTLE).all(|(_, c)| near(*c, last, 2)) {
        return Some(if value(last) < 8 { Target::Off } else { Target::Steady(last) });
    }
    pulse(frames)
}

/// One colour fading to near black and back, twice or more: the light breathing in its brightest,
/// at the speed nearest its period.
fn pulse(frames: &VecDeque<(Instant, [u8; 3])>) -> Option<Target> {
    let &(_, peak) = frames.iter().max_by_key(|(_, c)| value(*c))?;
    let top = value(peak);
    if top < 40 {
        return None;
    }
    // The same colour throughout, only dimmer.
    let one_colour = frames.iter().filter(|(_, c)| value(*c) >= top / 3).all(|(_, c)| {
        let v = value(*c) as f32;
        (0..3).all(|i| (c[i] as f32 / v - peak[i] as f32 / top as f32).abs() <= 0.12)
    });
    if !one_colour {
        return None;
    }
    let mut low = false;
    let mut dips: Vec<Instant> = Vec::new();
    for &(t, c) in frames {
        let v = value(c);
        if !low && v <= top / 8 {
            low = true;
            dips.push(t);
        } else if low && v >= top / 2 {
            low = false;
        }
    }
    let (&first, &last) = (dips.first()?, dips.last()?);
    if dips.len() < 2 {
        return None;
    }
    let period = last.duration_since(first).as_secs_f32() / (dips.len() - 1) as f32;
    Some(Target::Breathing(peak, speed_for(period)))
}

/// The breathing speed (1-5) whose breath is nearest `period` seconds.
fn speed_for(period: f32) -> u8 {
    (1..=5u8)
        .min_by(|a, b| {
            (BREATH_S[*a as usize - 1] - period).abs().total_cmp(&(BREATH_S[*b as usize - 1] - period).abs())
        })
        .unwrap_or(3)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

// ---- the light ------------------------------------------------------------------------------

/// A mouse's light as the user had it, before SignalRGB set it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Before {
    /// Every stage slot's colour.
    colours: Vec<String>,
    effect: LightEffect,
    speed: u8,
}

impl Before {
    fn of(s: &Settings) -> Option<Self> {
        let light = s.dpi_light?;
        Some(Self {
            colours: s.stages.as_ref()?.iter().map(|x| x.color.clone()).collect(),
            effect: light.effect,
            speed: light.speed,
        })
    }
}

/// What to write so the light shows `target`: every stage the DPI button cycles through takes the
/// colour. `None` when it already does.
fn change(s: &Settings, target: Target) -> Option<Settings> {
    let (stages, light, count) = (s.stages.as_ref()?, s.dpi_light?, s.dpi_stage_count?);
    let (colour, effect, speed) = match target {
        Target::Steady(c) => (Some(c), LightEffect::Steady, light.speed),
        Target::Breathing(c, speed) => (Some(c), LightEffect::Breathing, speed),
        Target::Off => (None, LightEffect::Off, light.speed),
    };
    let mut next = Settings::default();
    if let Some(c) = colour {
        let mut changed = stages.clone();
        for st in changed.iter_mut().take(count as usize) {
            st.color = hex(c);
        }
        if changed != *stages {
            next.stages = Some(changed);
        }
    }
    if light.effect != effect || light.speed != speed {
        next.dpi_light = Some(DpiLight { effect, speed, ..light });
    }
    (next != Settings::default()).then_some(next)
}

/// What to write to put the light back as `before` had it. The stages' DPI, and the brightness,
/// stay as they are now.
fn put_back(s: &Settings, before: &Before) -> Option<Settings> {
    let (mut stages, light) = (s.stages.clone()?, s.dpi_light?);
    for (st, was) in stages.iter_mut().zip(&before.colours) {
        st.color.clone_from(was);
    }
    Some(Settings {
        stages: Some(stages),
        dpi_light: Some(DpiLight { effect: before.effect, speed: before.speed, ..light }),
        ..Default::default()
    })
}

struct Link {
    hub: Arc<Hub>,
    file: PathBuf,
    /// Each mouse's light before SignalRGB set it, until it's put back.
    kept: HashMap<String, Before>,
    on: bool,
    /// The keys of the mice told to SignalRGB, by their id: one stays the same while hrtz runs.
    ids: Vec<String>,
    streams: HashMap<u8, Stream>,
    /// Each mouse's changes in the last day.
    writes: HashMap<String, VecDeque<Instant>>,
    asked_at: Option<Instant>,
    sockets: Option<(TcpListener, UdpSocket)>,
    tried_at: Option<Instant>,
    error: Option<String>,
    addon: Addon,
    installed: bool,
}

impl Link {
    fn run(mut self, rx: &Receiver<Msg>, status: &RwLock<Status>, changed: &dyn Fn()) {
        let mut looked: Option<Instant> = None;
        let mut ticked = Instant::now();
        loop {
            // Listening, a turn every 20 ms takes what arrived meanwhile (a frame comes every 33 ms);
            // idle, a turn a second.
            let wait = if self.sockets.is_some() { Duration::from_millis(20) } else { Duration::from_secs(1) };
            let mut msgs = Vec::new();
            match rx.recv_timeout(wait) {
                Ok(m) => msgs.push(m),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            msgs.extend(rx.try_iter());
            for m in msgs {
                match m {
                    Msg::On(on) => {
                        self.on = on;
                        looked = None;
                    }
                    Msg::Look => looked = None,
                }
            }

            if self.on && self.sockets.is_none() && self.tried_at.is_none_or(|t| t.elapsed() >= Duration::from_secs(5))
            {
                self.listen();
            }
            if !self.on && self.sockets.is_some() {
                self.sockets = None;
                self.streams.clear();
                self.asked_at = None;
            }
            if self.sockets.is_some() {
                self.serve();
            }
            if ticked.elapsed() >= Duration::from_millis(100) {
                ticked = Instant::now();
                self.tick();
            }
            if looked.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
                looked = Some(Instant::now());
                self.installed = signalrgb_dir().is_some();
                self.addon = addon_state();
                // Off, a light SignalRGB set goes back once its mouse is there to take it.
                if !self.on {
                    let keys: Vec<String> = self.kept.keys().cloned().collect();
                    for key in keys {
                        self.restore(&key);
                    }
                }
            }
            let now = self.status();
            if *status.read() != now {
                *status.write() = now;
                changed();
            }
        }
    }

    fn status(&self) -> Status {
        let mut driving: Vec<String> = self
            .streams
            .iter()
            .filter(|(_, s)| s.frames.back().is_some_and(|(t, _)| t.elapsed() <= FRAMES_WITHIN))
            .filter_map(|(id, _)| self.ids.get(*id as usize).cloned())
            .collect();
        driving.sort();
        Status {
            on: self.on,
            installed: self.installed,
            addon: self.addon,
            connected: self.sockets.is_some() && self.asked_at.is_some_and(|t| t.elapsed() <= ASKED_WITHIN),
            driving,
            error: if self.on { self.error.clone() } else { None },
        }
    }

    fn listen(&mut self) {
        self.tried_at = Some(Instant::now());
        let bound = (|| -> std::io::Result<(TcpListener, UdpSocket)> {
            let tcp = TcpListener::bind(ADDR)?;
            tcp.set_nonblocking(true)?;
            let udp = UdpSocket::bind(ADDR)?;
            udp.set_nonblocking(true)?;
            Ok((tcp, udp))
        })();
        match bound {
            Ok(s) => {
                self.sockets = Some(s);
                self.error = None;
            }
            Err(e) => {
                eprintln!("signalrgb: can't listen on {ADDR}: {e}");
                self.error =
                    Some(format!("Another program is using port 17421, which hrtz needs for SignalRGB ({e})."));
            }
        }
    }

    /// Answer SignalRGB's asks, and take in the frames that arrived, without waiting for more: a
    /// stream never pauses long enough for a read with a timeout to end, and SignalRGB gives an
    /// ask half a second.
    fn serve(&mut self) {
        let Some((tcp, udp)) = &self.sockets else { return };
        let mut asks = Vec::new();
        loop {
            match tcp.accept() {
                Ok((stream, _)) => asks.push(stream),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        let mut packets = Vec::new();
        let mut buf = [0u8; 64];
        while let Ok((n, _)) = udp.recv_from(&mut buf) {
            if n >= 12 && buf.starts_with(b"HZ") {
                packets.push(buf);
            }
            if packets.len() >= 256 {
                break;
            }
        }
        if !asks.is_empty() {
            let list = self.listed();
            for stream in asks {
                let _ = answer(stream, &list);
            }
            self.asked_at = Some(Instant::now());
        }
        let now = Instant::now();
        for p in packets {
            // "HZ", kind (1 hello, 2 frame, 3 goodbye), the mouse's id, frame number (4), r, g, b,
            // and on a goodbye whether SignalRGB is suspending.
            let id = p[3];
            match p[2] {
                2 => self.streams.entry(id).or_default().push(now, [p[8], p[9], p[10]]),
                3 => {
                    self.streams.remove(&id);
                    // Quitting, the light goes back; suspending, the PC is going to sleep and
                    // SignalRGB will pick up where it was.
                    if p[11] == 0
                        && let Some(key) = self.ids.get(id as usize).cloned()
                    {
                        self.restore(&key);
                    }
                }
                _ => {}
            }
        }
    }

    /// The connected mice with a DPI light, with their ids and names, as the add-on asks. Zaopins
    /// only: the breathing speeds were timed on one, and an ATK's light has three of the five.
    fn listed(&mut self) -> Vec<(u8, String)> {
        let mut list = Vec::new();
        for s in self.hub.states() {
            let lit = s.capabilities.as_ref().and_then(|c| c.dpi.as_ref()).is_some_and(|d| d.light)
                && crate::mice::compx::models::is_zaopin(s.cid, s.mid);
            let Some(key) = s.key.filter(|_| s.connected && lit) else { continue };
            let id = match self.ids.iter().position(|k| *k == key) {
                Some(i) => i,
                None => {
                    self.ids.push(key);
                    self.ids.len() - 1
                }
            };
            if let Ok(id) = u8::try_from(id) {
                list.push((id, s.model.unwrap_or_else(|| "Mouse".into())));
            }
        }
        list
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let ids: Vec<u8> = self.streams.keys().copied().collect();
        for id in ids {
            let Some(key) = self.ids.get(id as usize).cloned() else { continue };
            let Some(s) = self.streams.get(&id) else { continue };
            let Some(target) = classify(&s.frames, now) else { continue };
            // Asleep, it's tried once it wakes.
            if !self.hub.engine(&key).is_some_and(|e| e.snapshot().online) {
                continue;
            }
            if s.applied.is_some_and(|a| same(a, target)) || s.last_try.is_some_and(|t| now.duration_since(t) < MIN_GAP)
            {
                continue;
            }
            let day = self.writes.entry(key.clone()).or_default();
            while day.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(24 * 3600)) {
                day.pop_front();
            }
            if day.len() >= DAILY_CAP {
                continue;
            }
            let result = self.set(&key, target);
            let s = self.streams.entry(id).or_default();
            s.last_try = Some(now);
            match result {
                Ok(wrote) => {
                    s.applied = Some(target);
                    if wrote {
                        self.writes.entry(key).or_default().push_back(now);
                    }
                }
                // Asleep or gone: tried again after the gap.
                Err(e) => eprintln!("signalrgb: {key}: {e:#}"),
            }
        }
    }

    /// Set mouse `key`'s light to `target`, keeping the user's light first. Returns whether
    /// anything was written.
    fn set(&mut self, key: &str, target: Target) -> Result<bool> {
        let engine = self.hub.engine(key).ok_or_else(|| anyhow!("not connected"))?;
        let s = engine.snapshot();
        if !s.online {
            bail!("asleep");
        }
        let settings = s.settings.ok_or_else(|| anyhow!("its settings aren't read yet"))?;
        let Some(next) = change(&settings, target) else { return Ok(false) };
        if !self.kept.contains_key(key) {
            let before = Before::of(&settings).ok_or_else(|| anyhow!("it has no DPI light"))?;
            self.kept.insert(key.to_string(), before);
            self.save();
        }
        engine.call(move |w| w.apply_settings(&next))?;
        Ok(true)
    }

    /// Put mouse `key`'s light back as the user had it, if it's there to take it.
    fn restore(&mut self, key: &str) {
        let Some(before) = self.kept.get(key) else { return };
        let Some(engine) = self.hub.engine(key) else { return };
        let s = engine.snapshot();
        let Some(next) = s.settings.as_ref().filter(|_| s.online).and_then(|x| put_back(x, before)) else { return };
        match engine.call(move |w| w.apply_settings(&next)) {
            Ok(_) => {
                self.kept.remove(key);
                self.save();
            }
            Err(e) => eprintln!("signalrgb: {key}: couldn't put the light back: {e:#}"),
        }
    }

    fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(&self.kept) {
            let _ = std::fs::write(&self.file, b);
        }
    }
}

fn answer(mut stream: TcpStream, mice: &[(u8, String)]) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf)?;
    let (status, body) = if buf[..n].starts_with(b"GET /mice ") {
        let list: Vec<_> = mice.iter().map(|(id, name)| serde_json::json!({ "id": id, "name": name })).collect();
        ("200 OK", serde_json::to_string(&list).unwrap_or_default())
    } else {
        ("404 Not Found", String::new())
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mice::model::DpiStage;

    const FRAME: Duration = Duration::from_millis(33);

    /// `n` frames, one every 33 ms, coloured by `f` (from the frame's index); and the moment after.
    fn frames(n: usize, f: impl Fn(usize) -> [u8; 3]) -> (VecDeque<(Instant, [u8; 3])>, Instant) {
        let t0 = Instant::now();
        let mut s = Stream::default();
        for i in 0..n {
            s.push(t0 + FRAME * i as u32, f(i));
        }
        (s.frames, t0 + FRAME * n as u32)
    }

    /// SignalRGB's Pulse: `rgb` to black and back every `period` frames.
    fn pulsing(rgb: [u8; 3], period: usize) -> impl Fn(usize) -> [u8; 3] {
        move |i| {
            let phase = (i % period) as f32 / period as f32;
            let k = 1.0 - (2.0 * phase - 1.0).abs();
            rgb.map(|c| (c as f32 * k) as u8)
        }
    }

    #[test]
    fn a_colour_held_a_second_is_meant() {
        let (f, now) = frames(40, |_| [0xEB, 0, 0x10]);
        assert_eq!(classify(&f, now), Some(Target::Steady([0xEB, 0, 0x10])));
        let (f, now) = frames(40, |_| [0, 0, 0]);
        assert_eq!(classify(&f, now), Some(Target::Off));
        // Not yet: a switch holds a stale frame for 0.4 s.
        let (f, now) = frames(15, |_| [0x29, 0x55, 0x9F]);
        assert_eq!(classify(&f, now), None);
        // Nor when SignalRGB has stopped sending.
        let (f, now) = frames(40, |_| [0xEB, 0, 0x10]);
        assert_eq!(classify(&f, now + Duration::from_millis(500)), None);
    }

    #[test]
    fn a_pulse_breathes_in_its_brightest_colour_at_the_nearest_speed() {
        // #00BBFF every 2.1 s (64 frames), as SignalRGB's Pulse sent it.
        let (f, now) = frames(180, pulsing([0, 0xBB, 0xFF], 64));
        let Some(Target::Breathing(c, speed)) = classify(&f, now) else { panic!("not a pulse") };
        assert!(near(c, [0, 0xBB, 0xFF], 16), "{}", hex(c));
        assert_eq!(speed, speed_for(2.1));
        // Faster, a faster speed (or the same, at the end of the scale).
        let (f, now) = frames(180, pulsing([0xEB, 0, 0x10], 30));
        let Some(Target::Breathing(_, faster)) = classify(&f, now) else { panic!("not a pulse") };
        assert!(faster >= speed);
    }

    #[test]
    fn an_animated_effect_is_left_alone() {
        // The hue going round, as Neon Shift does.
        let (f, now) = frames(180, |i| {
            let h = (i * 6 % 256) as u8;
            [h, 255 - h, (h / 2).wrapping_add(64)]
        });
        assert_eq!(classify(&f, now), None);
    }

    #[test]
    fn speeds_go_by_the_nearest_breath() {
        assert_eq!(speed_for(100.0), 1);
        assert_eq!(speed_for(0.1), 5);
        for (i, s) in BREATH_S.iter().enumerate() {
            assert_eq!(speed_for(*s) as usize, i + 1);
        }
    }

    fn mouse(colours: &[&str], count: u8, effect: LightEffect) -> Settings {
        Settings {
            dpi_stage_count: Some(count),
            stages: Some(colours.iter().map(|c| DpiStage { dpi: 800, color: c.to_string() }).collect()),
            dpi_light: Some(DpiLight { effect, brightness: 7, speed: 2 }),
            ..Default::default()
        }
    }

    #[test]
    fn every_stage_in_the_cycle_takes_the_colour_and_only_whats_new_is_written() {
        let s = mouse(&["#FFFFFF", "#00FF00", "#0000FF"], 2, LightEffect::Off);
        let next = change(&s, Target::Steady([0xFF, 0, 0])).unwrap();
        let colours: Vec<_> = next.stages.unwrap().iter().map(|x| x.color.clone()).collect();
        assert_eq!(colours, ["#FF0000", "#FF0000", "#0000FF"], "the third isn't in the cycle");
        assert_eq!(next.dpi_light, Some(DpiLight { effect: LightEffect::Steady, brightness: 7, speed: 2 }));

        // Already showing it: nothing to write.
        let s = mouse(&["#FF0000", "#FF0000"], 2, LightEffect::Steady);
        assert_eq!(change(&s, Target::Steady([0xFF, 0, 0])), None);
        // Off leaves the colours.
        let off = change(&s, Target::Off).unwrap();
        assert_eq!((off.stages, off.dpi_light.map(|l| l.effect)), (None, Some(LightEffect::Off)));
        // A pulse sets the speed too.
        let b = change(&s, Target::Breathing([0xFF, 0, 0], 5)).unwrap();
        assert_eq!(b.stages, None);
        assert_eq!(b.dpi_light, Some(DpiLight { effect: LightEffect::Breathing, brightness: 7, speed: 5 }));
    }

    #[test]
    fn the_light_goes_back_as_the_user_had_it() {
        let mine = mouse(&["#FFFFFF", "#00FF00"], 2, LightEffect::Off);
        let before = Before::of(&mine).unwrap();
        let mut set = mouse(&["#FF0000", "#FF0000"], 2, LightEffect::Breathing);
        set.dpi_light = Some(DpiLight { effect: LightEffect::Breathing, brightness: 3, speed: 5 });
        let back = put_back(&set, &before).unwrap();
        let colours: Vec<_> = back.stages.unwrap().iter().map(|x| x.color.clone()).collect();
        assert_eq!(colours, ["#FFFFFF", "#00FF00"]);
        // The brightness is the user's own setting, changed meanwhile: it stays.
        assert_eq!(back.dpi_light, Some(DpiLight { effect: LightEffect::Off, brightness: 3, speed: 2 }));
    }
}
