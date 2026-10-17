//! Mousepad usage: where the mouse sits on a calibrated pad, estimated from raw sensor counts.
//!
//! The mouse reports movement, never position, and sends nothing while lifted, so the estimate is
//! a cloud of guesses (`filter`) moved by every report:
//! - **Edges.** The pad's edges are the evidence: a guess that runs through one loses weight.
//! - **Lifts.** At each pause the lift model (`lift`) gives the chance it was a lift. That share of
//!   the guesses comes down somewhere new, near the user's usual put-down spot or back against the
//!   swipe. The rest stay put, and later movement and edges decide between them.
//! - **The map.** Swipes are held back for a while before they reach the heatmap, so each one is
//!   drawn from where the evidence finally says it started.
//! - **Learning.** The lift model, the put-down spot and the accuracy figure are all learned from
//!   use; nothing is asked of the user after calibration.
//!
//! Swipe reach comes straight from the counts and doesn't depend on any of this.

mod filter;
mod lift;
mod segment;
#[cfg(test)]
mod sim;

use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use filter::{Filter, Landing};
use lift::{After, FEATURES, Inputs};
use segment::{BLIP_MAX, BLIP_WINDOW, PAUSE_MIN, Pause, Segmenter};

/// Swipes are binned by width and height in 1 cm steps up to this.
pub const REACH_BINS: usize = 100;
/// Swipes held back before they're drawn on the map, so later evidence can still move them.
pub(crate) const HELD: usize = 24;
/// Movement is batched this long before the guesses move.
const BIN: Duration = Duration::from_millis(1);
/// After this long without movement, everything held back is drawn as it stands.
const IDLE_FLUSH: Duration = Duration::from_secs(3);
/// Stored path points are at least this far apart (cm).
const PATH_STEP: f64 = 0.2;
const MAX_PATH_POINTS: usize = 4000;
/// The start of the next swipe is read for this long, or this far, before the lift score is final.
const AFTER_TIME: Duration = Duration::from_millis(60);
const AFTER_DISTANCE: f64 = 1.0;
/// Each swipe is drawn on the map from this many weighted guesses at where it started.
const MAP_SAMPLES: usize = 8;
/// A swipe counts as reaching an edge when it comes this close (cm).
const EDGE_NEAR: f64 = 1.0;
/// Room left along a swipe's direction at which the room input reads 0 (cm).
const ROOM_SCALE: f64 = 12.0;
/// How quickly the learned put-down spot follows new landings.
const HOME_RATE: f64 = 0.1;
const KEEP_PAUSES: usize = 1000;
const REFIT_EVERY: u32 = 150;
const KEEP_DRIFT: usize = 50;
/// An edge correction is over once the edges have been quiet this long.
const DRIFT_SETTLE: Duration = Duration::from_millis(300);
const DATA_VERSION: u32 = 3;
/// Apps kept on their own, besides everything together.
const MAX_APPS: usize = 8;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    /// Rotation from the sensor's axes to the pad's, radians.
    pub angle: f64,
    /// Usable distance from the origin to each edge, cm.
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    /// 2: the origin is the user's own resting spot. Older calibrations measured from the centre.
    #[serde(default)]
    pub version: u32,
}

impl Calibration {
    fn cols(&self) -> usize {
        (self.left + self.right).ceil().max(1.0) as usize
    }
    fn rows(&self) -> usize {
        (self.top + self.bottom).ceil().max(1.0) as usize
    }
}

/// What was recorded in one app, or in all of them.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    /// cm of movement in each 1 cm cell, row by row from the top-left corner.
    pub cells: Vec<f32>,
    /// Expected edge hits per 1 cm along each edge: top and bottom by column, left and right by row.
    pub edge_top: Vec<f32>,
    pub edge_bottom: Vec<f32>,
    pub edge_left: Vec<f32>,
    pub edge_right: Vec<f32>,
    /// Pauses judged more likely lifts than not, once settled.
    pub lifts: f64,
    /// Swipes counted by their width and height, 1 cm bins.
    pub reach_x: Vec<u32>,
    pub reach_y: Vec<u32>,
    pub moving_ms: f64,
    pub distance_cm: f64,
}

impl Record {
    fn new(cols: usize, rows: usize) -> Self {
        Self {
            cells: vec![0.0; cols * rows],
            edge_top: vec![0.0; cols],
            edge_bottom: vec![0.0; cols],
            edge_left: vec![0.0; rows],
            edge_right: vec![0.0; rows],
            reach_x: vec![0; REACH_BINS],
            reach_y: vec![0; REACH_BINS],
            ..Default::default()
        }
    }

    fn fits(&self, cols: usize, rows: usize) -> bool {
        self.cells.len() == cols * rows
            && self.edge_top.len() == cols
            && self.edge_bottom.len() == cols
            && self.edge_left.len() == rows
            && self.edge_right.len() == rows
            && self.reach_x.len() == REACH_BINS
            && self.reach_y.len() == REACH_BINS
    }
}

/// An app the mouse was used in, by its exe name in lower case.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppRecord {
    pub exe: String,
    pub name: String,
    /// Full path, for the icon.
    pub path: String,
    /// Unix seconds.
    pub last_used: u64,
    #[serde(flatten)]
    pub record: Record,
}

/// The app in focus, as the foreground watcher found it.
#[derive(Clone, Debug)]
pub struct AppTag {
    /// Exe name in lower case.
    pub exe: Arc<str>,
    pub name: String,
    pub path: String,
}

/// Everything recorded for one calibration. Positions are cm from the origin, x right and y down
/// along the pad.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PadData {
    pub version: u32,
    pub calibration: Option<Calibration>,
    /// Learned put-down spot, and its spread as covariance xx, xy, yy (cm²).
    pub home: (f64, f64),
    pub home_spread: (f64, f64, f64),
    pub cols: usize,
    pub rows: usize,
    /// Everything, in every game recorded.
    #[serde(flatten)]
    pub all: Record,
    /// The apps used most, each on its own.
    pub apps: Vec<AppRecord>,
    /// The lift model, and how many edge-settled pauses it has learned from.
    pub weights: Vec<f64>,
    pub learned: u32,
    /// Recent pauses' inputs and final lift probabilities, for the refit.
    pub pauses: Vec<Vec<f32>>,
    pub pause_lifts: Vec<f32>,
    pub since_refit: u32,
    /// Recent corrections the edges made to the estimate (cm).
    pub drift: Vec<f32>,
}

impl PadData {
    fn new(cal: Calibration) -> Self {
        let (cols, rows) = (cal.cols(), cal.rows());
        Self {
            version: DATA_VERSION,
            calibration: Some(cal),
            home: (0.0, 0.0),
            home_spread: (4.0, 0.0, 4.0),
            cols,
            rows,
            all: Record::new(cols, rows),
            weights: lift::START_WEIGHTS.to_vec(),
            ..Default::default()
        }
    }

    /// Apply `f` to the record for every game and the one for `app`. Nothing is recorded outside
    /// the games on the list (`app` is `None` there).
    fn each(&mut self, app: Option<&str>, mut f: impl FnMut(&mut Record)) {
        let Some(exe) = app else { return };
        f(&mut self.all);
        if let Some(r) = self.apps.iter_mut().find(|a| a.exe == exe) {
            f(&mut r.record);
        }
    }

    /// Start keeping `tag` on its own, making room by dropping the app that's been used least,
    /// and least lately.
    fn keep_app(&mut self, tag: &AppTag, now: u64) {
        if self.apps.iter().any(|a| a.exe == *tag.exe) {
            return;
        }
        if self.apps.len() >= MAX_APPS {
            let score = |a: &AppRecord| a.record.moving_ms / (1.0 + now.saturating_sub(a.last_used) as f64 / 86_400.0);
            if let Some(i) = (0..self.apps.len()).min_by(|&i, &j| score(&self.apps[i]).total_cmp(&score(&self.apps[j])))
            {
                self.apps.remove(i);
            }
        }
        self.apps.push(AppRecord {
            exe: tag.exe.to_string(),
            name: tag.name.clone(),
            path: tag.path.clone(),
            last_used: now,
            record: Record::new(self.cols, self.rows),
        });
    }

    /// Start a fresh record for the same calibration, keeping what was learned about the user.
    fn cleared(&self) -> Self {
        let Some(cal) = self.calibration else { return Self::default() };
        Self {
            home: self.home,
            home_spread: self.home_spread,
            weights: self.weights.clone(),
            learned: self.learned,
            pauses: self.pauses.clone(),
            pause_lifts: self.pause_lifts.clone(),
            since_refit: self.since_refit,
            ..Self::new(cal)
        }
    }

    pub fn load(path: &Path) -> Self {
        let data: Self = fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // An older format, or a shape that doesn't match its calibration: keep the calibration and
        // start the record over rather than misread it.
        let mut data = match data.calibration {
            Some(cal) if data.all.fits(cal.cols(), cal.rows()) => match data.version {
                DATA_VERSION if data.weights.len() == FEATURES => data,
                // Version 2 took hard-stopping flicks for lifts. Keep the map; start the lift
                // count, the lift model and the put-down spot it taught over.
                2 => Self {
                    version: DATA_VERSION,
                    all: Record { lifts: 0.0, ..data.all },
                    home: (0.0, 0.0),
                    home_spread: (4.0, 0.0, 4.0),
                    weights: lift::START_WEIGHTS.to_vec(),
                    learned: 0,
                    pauses: Vec::new(),
                    pause_lifts: Vec::new(),
                    since_refit: 0,
                    ..data
                },
                _ => Self::new(cal),
            },
            Some(cal) => Self::new(cal),
            None => Self::default(),
        };
        let (cols, rows) = (data.cols, data.rows);
        data.apps.retain(|a| a.record.fits(cols, rows));
        data
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self)?)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// What the UI shows.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PadView {
    pub calibration: Option<Calibration>,
    pub home: (f64, f64),
    pub home_spread: (f64, f64, f64),
    pub cols: usize,
    pub rows: usize,
    /// The app shown, or none for everything together.
    pub app: Option<String>,
    /// The record shown. Its lift count includes pauses still being decided that look like one.
    #[serde(flatten)]
    pub record: Record,
    /// Apps kept on their own, most used first.
    pub apps: Vec<AppSummary>,
    /// The live estimate, and how unsure it is (cm).
    pub pos: (f64, f64),
    pub spread: f64,
    /// Typical correction the edges have made (cm), once there are a few.
    pub accuracy: Option<f64>,
    /// Grip angle in use: calibration plus what the edges have taught (degrees).
    pub grip_deg: f64,
    pub learned: u32,
    pub tracking: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppSummary {
    pub exe: String,
    pub name: String,
    pub path: String,
    pub moving_ms: f64,
}

/// A swipe held back from the map, and the pause before it.
struct Held {
    #[cfg_attr(not(test), allow(dead_code))]
    started: Instant,
    slot: usize,
    bit: u32,
    /// The app in focus when it began.
    app: Option<Arc<str>>,
    /// Cumulative movement from the swipe's start, every `PATH_STEP`.
    points: Vec<(f32, f32)>,
    at: (f64, f64),
    last_point: (f64, f64),
    /// min x, max x, min y, max y of the cumulative movement.
    bbox: [f64; 4],
    /// The cross-axis coordinate where it reached each extreme: top, bottom, left, right.
    along: [f64; 4],
    closed: bool,
    /// The pause before this swipe, if there was one: its inputs, prior and final lift chance.
    inputs: Inputs,
    paused: bool,
    prior: f64,
    chance: f64,
}

impl Held {
    fn new(
        started: Instant,
        slot: usize,
        bit: u32,
        app: Option<Arc<str>>,
        inputs: Inputs,
        paused: bool,
        prior: f64,
    ) -> Self {
        Self {
            started,
            slot,
            bit,
            app,
            points: vec![(0.0, 0.0)],
            at: (0.0, 0.0),
            last_point: (0.0, 0.0),
            bbox: [0.0; 4],
            along: [0.0; 4],
            closed: false,
            inputs,
            paused,
            prior,
            chance: prior,
        }
    }

    fn extend(&mut self, mx: f64, my: f64) {
        self.at.0 += mx;
        self.at.1 += my;
        let (x, y) = self.at;
        if y < self.bbox[2] {
            self.bbox[2] = y;
            self.along[0] = x;
        }
        if y > self.bbox[3] {
            self.bbox[3] = y;
            self.along[1] = x;
        }
        if x < self.bbox[0] {
            self.bbox[0] = x;
            self.along[2] = y;
        }
        if x > self.bbox[1] {
            self.bbox[1] = x;
            self.along[3] = y;
        }
        if (x - self.last_point.0).hypot(y - self.last_point.1) >= PATH_STEP && self.points.len() < MAX_PATH_POINTS {
            self.points.push((x as f32, y as f32));
            self.last_point = (x, y);
        }
    }

    /// Moved at least 0.5 cm toward top, bottom, left, right from where it started.
    fn reached(&self) -> [bool; 4] {
        [self.bbox[2] < -0.5, self.bbox[3] > 0.5, self.bbox[0] < -0.5, self.bbox[1] > 0.5]
    }
}

/// The first moments of a swipe after a pause, read to finish that pause's lift score.
struct AfterWindow {
    start: Instant,
    last: Instant,
    dir: (f64, f64),
    gap: Duration,
    end_speed: f64,
    moved: (f64, f64),
    path: f64,
    reverse: f64,
}

pub struct Tracker {
    pub data: PadData,
    filter: Filter,
    seg: Segmenter,
    held: VecDeque<Held>,
    seq: u64,
    after: Option<AfterWindow>,
    bin: (f64, f64),
    bin_start: Option<Instant>,
    drift_run: f64,
    drift_last: Option<Instant>,
    /// Lift-off is 1 mm or more: the sensor tracks for a moment as the mouse rises and lands.
    pub blips: bool,
    /// The app in focus, and whether its record exists yet.
    app: Option<AppTag>,
    app_kept: bool,
    pub dirty: bool,
    /// Each settled pause: when the swipe after it began, and its final lift chance.
    #[cfg(test)]
    pause_log: Vec<(Instant, f64)>,
    #[cfg(test)]
    pause_detail: Vec<(Inputs, f64, f64, f64)>,
}

/// How close a swipe heading `dir` from `pos` was to running out of pad: 1 at the edge, 0 with
/// `ROOM_SCALE` cm left, -1 with twice that.
fn room_left(pos: (f64, f64), dir: (f64, f64), cal: &Calibration) -> f64 {
    let ray = |p: f64, u: f64, lo: f64, hi: f64| {
        if u > 1e-6 {
            (hi - p) / u
        } else if u < -1e-6 {
            (lo - p) / u
        } else {
            f64::INFINITY
        }
    };
    if dir == (0.0, 0.0) {
        return 0.0;
    }
    let t = ray(pos.0, dir.0, -cal.left, cal.right).min(ray(pos.1, dir.1, -cal.top, cal.bottom)).max(0.0);
    (1.0 - t / ROOM_SCALE).clamp(-1.0, 1.0)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn odds(p: f64) -> f64 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    p / (1.0 - p)
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64)
}

impl Tracker {
    pub fn new(data: PadData) -> Self {
        Self::with_seed(data, seed())
    }

    fn with_seed(data: PadData, seed: u64) -> Self {
        let mut filter = Filter::new(data.home, seed);
        if let Some(cal) = data.calibration {
            // After a restart the mouse could be anywhere it's usually put down.
            let land = Landing { home: data.home, spread: data.home_spread };
            filter.split(1.0, 0, (0.0, 0.0), &land, &cal);
        }
        Self {
            data,
            filter,
            seg: Segmenter::default(),
            held: VecDeque::new(),
            seq: 0,
            after: None,
            bin: (0.0, 0.0),
            bin_start: None,
            drift_run: 0.0,
            drift_last: None,
            blips: false,
            app: None,
            app_kept: false,
            dirty: false,
            #[cfg(test)]
            pause_log: Vec::new(),
            #[cfg(test)]
            pause_detail: Vec::new(),
        }
    }

    /// Start over with a new calibration; the mouse is at the origin, the user's resting spot.
    pub fn calibrate(&mut self, cal: Calibration) {
        let (blips, app) = (self.blips, self.app.take());
        *self = Self::new(PadData::new(cal));
        self.blips = blips;
        self.app = app;
        self.filter = Filter::new((0.0, 0.0), seed());
        self.dirty = true;
    }

    /// Clear the record and keep the calibration and what was learned about the user.
    pub fn clear(&mut self) {
        let data = self.data.cleared();
        let filter = std::mem::replace(&mut self.filter, Filter::new((0.0, 0.0), 1));
        let (blips, app) = (self.blips, self.app.take());
        *self = Self::new(data);
        self.filter = filter;
        self.blips = blips;
        self.app = app;
        self.dirty = true;
    }

    pub fn pos(&self) -> (f64, f64) {
        self.filter.mean()
    }

    /// The app in focus from now on; `None` for none worth keeping on its own (this one).
    pub fn set_app(&mut self, app: Option<AppTag>) {
        if self.app.as_ref().map(|a| &a.exe) != app.as_ref().map(|a| &a.exe) {
            self.app = app;
            self.app_kept = false;
        }
    }

    fn app_exe(&self) -> Option<Arc<str>> {
        self.app.as_ref().map(|a| a.exe.clone())
    }

    /// One report from the mouse: `dx`, `dy` in sensor counts at `dpi`.
    pub fn report(&mut self, now: Instant, dx: i32, dy: i32, dpi: u32) {
        let Some(cal) = self.data.calibration else { return };
        if dpi == 0 || (dx == 0 && dy == 0) {
            return;
        }
        let k = 2.54 / dpi as f64;
        let (sx, sy) = (dx as f64 * k, dy as f64 * k);
        let (c, s) = (cal.angle.cos(), cal.angle.sin());
        let (mx, my) = (sx * c - sy * s, sx * s + sy * c);

        if !self.app_kept {
            if let Some(tag) = &self.app {
                self.data.keep_app(tag, unix_now());
            }
            self.app_kept = true;
        }
        let app = self.app.as_ref().map(|a| a.exe.clone());
        let moving = self.seg.last().map(|prev| now - prev).filter(|d| *d < PAUSE_MIN);
        let step = mx.hypot(my);
        self.data.each(app.as_deref(), |r| {
            if let Some(d) = moving {
                r.moving_ms += d.as_secs_f64() * 1000.0;
            }
            r.distance_cm += step;
        });
        match self.seg.push(now, mx, my) {
            Some(pause) => {
                self.flush_bin(now, &cal);
                self.paused(now, pause, &cal);
            }
            None if self.held.is_empty() => self.open(now, None, &cal),
            None => {}
        }
        self.after_step(now, mx, my, &cal);
        if let Some(h) = self.held.back_mut() {
            h.extend(mx, my);
        }
        self.bin.0 += mx;
        self.bin.1 += my;
        let start = *self.bin_start.get_or_insert(now);
        if now - start >= BIN {
            self.flush_bin(now, &cal);
        }
        self.dirty = true;
    }

    /// Housekeeping between reports: after a while without movement, draw everything held back.
    pub fn tick(&mut self, now: Instant) {
        let Some(cal) = self.data.calibration else { return };
        self.settle_drift(now);
        if self.held.is_empty() || self.seg.last().is_none_or(|t| now - t < IDLE_FLUSH) {
            return;
        }
        self.flush_bin(now, &cal);
        self.finish_after(&cal);
        while !self.held.is_empty() {
            self.commit_front(&cal);
        }
        self.dirty = true;
    }

    fn flush_bin(&mut self, now: Instant, cal: &Calibration) {
        let (mx, my) = std::mem::take(&mut self.bin);
        self.bin_start = None;
        if mx == 0.0 && my == 0.0 {
            return;
        }
        if let Some(shift) = self.filter.motion(mx, my, cal) {
            self.drift_run += shift;
            self.drift_last = Some(now);
        } else {
            self.settle_drift(now);
        }
    }

    /// Record an edge correction once it's over.
    fn settle_drift(&mut self, now: Instant) {
        if self.drift_last.is_some_and(|t| now - t >= DRIFT_SETTLE) {
            if self.drift_run >= 0.2 {
                self.data.drift.push(self.drift_run as f32);
                if self.data.drift.len() > KEEP_DRIFT {
                    self.data.drift.remove(0);
                }
            }
            self.drift_run = 0.0;
            self.drift_last = None;
        }
    }

    fn landing(&self) -> Landing {
        Landing { home: self.data.home, spread: self.data.home_spread }
    }

    fn paused(&mut self, now: Instant, mut pause: Pause, cal: &Calibration) {
        self.finish_after(cal);
        self.close_current();
        // Below 1 mm lift-off the sensor stops the moment the mouse rises, so movement back just
        // before a pause is the hand: a flick's recoil.
        if !self.blips {
            pause.reverse = 0.0;
        }
        let room = room_left(self.filter.mean(), pause.dir, cal);
        let inputs = lift::pre_inputs(&pause, room);
        let prior = lift::prior(&self.data.weights, &inputs);
        self.open(now, Some((pause, inputs, prior)), cal);
    }

    /// Record the reach of the swipe in progress; it stays held back.
    fn close_current(&mut self) {
        let Some(h) = self.held.back_mut() else { return };
        if h.closed {
            return;
        }
        h.closed = true;
        let (w, ht) = (h.bbox[1] - h.bbox[0], h.bbox[3] - h.bbox[2]);
        if w >= 0.5 || ht >= 0.5 {
            let bin = |span: f64| (span.round() as usize).min(REACH_BINS - 1);
            let app = h.app.clone();
            self.data.each(app.as_deref(), |r| {
                r.reach_x[bin(w)] += 1;
                r.reach_y[bin(ht)] += 1;
            });
        }
    }

    /// Start a new swipe, after a pause or not.
    fn open(&mut self, now: Instant, before: Option<(Pause, Inputs, f64)>, cal: &Calibration) {
        while self.held.len() >= HELD {
            self.commit_front(cal);
        }
        self.seq += 1;
        let slot = (self.seq % HELD as u64) as usize;
        let bit = (self.seq % 32) as u32;
        let land = self.landing();
        let held = match before {
            Some((pause, inputs, prior)) => {
                self.filter.resample();
                // Keep enough guesses on each side to represent both outcomes, then weight them
                // back to the model's odds.
                let drawn = prior.clamp(0.15, 0.85);
                self.filter.split(drawn, bit, pause.dir, &land, cal);
                self.filter.reweight(bit, odds(prior) / odds(drawn));
                self.after = Some(AfterWindow {
                    start: now,
                    last: now,
                    dir: pause.dir,
                    gap: pause.gap,
                    end_speed: pause.end_speed,
                    moved: (0.0, 0.0),
                    path: 0.0,
                    reverse: 0.0,
                });
                Held::new(now, slot, bit, self.app_exe(), inputs, true, prior)
            }
            None => {
                self.filter.split(0.0, bit, (0.0, 0.0), &land, cal);
                Held::new(now, slot, bit, self.app_exe(), [0.0; FEATURES], false, 0.0)
            }
        };
        self.filter.record_start(slot);
        self.held.push_back(held);
    }

    fn after_step(&mut self, now: Instant, mx: f64, my: f64, cal: &Calibration) {
        let Some(a) = self.after.as_mut() else { return };
        a.last = now;
        a.moved.0 += mx;
        a.moved.1 += my;
        a.path += mx.hypot(my);
        if now - a.start <= BLIP_WINDOW {
            a.reverse += (-(mx * a.dir.0 + my * a.dir.1)).max(0.0);
        }
        if a.path >= AFTER_DISTANCE || now - a.start >= AFTER_TIME {
            self.finish_after(cal);
        }
    }

    /// Finish the lift score of the latest pause from how the next swipe began, and move weight
    /// between the guesses that lifted and those that stayed to match.
    fn finish_after(&mut self, _cal: &Calibration) {
        let Some(a) = self.after.take() else { return };
        let Some(h) = self.held.back_mut() else { return };
        let len = a.moved.0.hypot(a.moved.1);
        let same_way = if len > 0.05 { (a.moved.0 * a.dir.0 + a.moved.1 * a.dir.1) / len } else { 0.0 };
        let secs = (a.last - a.start).as_secs_f64().max(0.005);
        let resume = a.path / secs;
        let dropout =
            a.gap < Duration::from_millis(150) && same_way > 0.8 && a.end_speed > 10.0 && resume >= 0.5 * a.end_speed;
        // A landing blip is a few millimetres back before the swipe gets going. A swipe that just
        // heads back the other way isn't one, and below 1 mm lift-off there are none.
        let reverse = if self.blips && same_way > -0.3 { a.reverse.min(BLIP_MAX) } else { 0.0 };
        lift::add_after(&mut h.inputs, &After { reverse, dropout, same_way });
        let chance = lift::probability(&self.data.weights, &h.inputs);
        if h.prior > 0.0 {
            self.filter.reweight(h.bit, odds(chance) / odds(h.prior));
        }
        h.chance = chance;
    }

    /// Draw the oldest held swipe on the map from where the guesses now say it started, and
    /// settle the pause before it.
    fn commit_front(&mut self, cal: &Calibration) {
        if self.held.len() == 1 {
            self.close_current();
        }
        let Some(h) = self.held.pop_front() else { return };
        // Outside the games on the list the mouse is followed, so the estimate is right on the way
        // back in, but nothing is drawn, counted or learned: pointing isn't aiming.
        if h.app.is_none() {
            return;
        }
        let ((sx, sy), var) = self.filter.start(h.slot);
        let (cols, rows) = (self.data.cols, self.data.rows);
        let col = |x: f64| ((x + cal.left).floor().max(0.0) as usize).min(cols - 1);
        let row = |y: f64| ((y + cal.top).floor().max(0.0) as usize).min(rows - 1);
        // Draw the swipe from a weighted sample of where the guesses say it started, not their
        // average: when they disagree, the average can sit where the mouse never was.
        let starts = self.filter.sample_starts(h.slot, MAP_SAMPLES);
        let share = 1.0 / starts.len() as f64;
        let tail = h.points.last().map_or((0.0, 0.0), |p| (p.0 as f64, p.1 as f64));
        let rest = (h.at.0 - tail.0).hypot(h.at.1 - tail.1);
        let mut cells = Vec::with_capacity(starts.len() * h.points.len());
        for (ox, oy) in starts {
            for pair in h.points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let len = ((b.0 - a.0) as f64).hypot((b.1 - a.1) as f64);
                let (mx, my) = (ox + (a.0 + b.0) as f64 / 2.0, oy + (a.1 + b.1) as f64 / 2.0);
                cells.push((row(my) * cols + col(mx), (len * share) as f32));
            }
            if rest > 0.0 {
                cells.push((row(oy + h.at.1) * cols + col(ox + h.at.0), (rest * share) as f32));
            }
        }
        let chances = self.filter.edge_chances(h.slot, h.bbox, h.reached(), EDGE_NEAR, cal);
        let at = [col(sx + h.along[0]), col(sx + h.along[1]), row(sy + h.along[2]), row(sy + h.along[3])];
        let lift_share = h.paused.then(|| self.filter.lift_share(h.bit));
        self.data.each(h.app.as_deref(), |r| {
            for &(i, v) in &cells {
                r.cells[i] += v;
            }
            let edges = [&mut r.edge_top, &mut r.edge_bottom, &mut r.edge_left, &mut r.edge_right];
            for ((list, at), chance) in edges.into_iter().zip(at).zip(chances) {
                if chance > 0.01 {
                    list[at] += chance as f32;
                }
            }
            // Counted only when it's more likely a lift than not: a run of pauses that might each
            // be one shouldn't add up to lifts that never happened.
            if lift_share.is_some_and(|s| s > 0.5) {
                r.lifts += 1.0;
            }
        });
        if let Some(a) = h.app.as_deref().and_then(|exe| self.data.apps.iter_mut().find(|a| a.exe == exe)) {
            a.last_used = unix_now();
        }

        if let Some(share) = lift_share {
            #[cfg(test)]
            self.pause_log.push((h.started, share));
            #[cfg(test)]
            self.pause_detail.push((h.inputs, h.prior, h.chance, share));
            // Learn where later evidence, the edges above all, moved it from what the score expected.
            if (share - h.chance).abs() > 0.2 {
                lift::learn(&mut self.data.weights, &h.inputs, share, h.chance);
                self.data.learned += 1;
            }
            self.data.pauses.push(h.inputs.iter().map(|v| *v as f32).collect());
            self.data.pause_lifts.push(share as f32);
            if self.data.pauses.len() > KEEP_PAUSES {
                self.data.pauses.remove(0);
                self.data.pause_lifts.remove(0);
            }
            self.data.since_refit += 1;
            if self.data.since_refit >= REFIT_EVERY {
                lift::refit(&mut self.data.weights, &self.data.pauses, &self.data.pause_lifts);
                self.data.since_refit = 0;
            }
            // A clear landing teaches where the user puts the mouse down.
            if share > 0.9 && var < 4.0 {
                let (hx, hy) = self.data.home;
                let (dx, dy) = (sx - hx, sy - hy);
                let r = HOME_RATE;
                self.data.home = (hx + r * dx, hy + r * dy);
                let (xx, xy, yy) = self.data.home_spread;
                self.data.home_spread = (
                    ((1.0 - r) * xx + r * dx * dx).clamp(1.0, 25.0),
                    ((1.0 - r) * xy + r * dx * dy).clamp(-20.0, 20.0),
                    ((1.0 - r) * yy + r * dy * dy).clamp(1.0, 25.0),
                );
            }
        }
        self.dirty = true;
    }

    /// What the UI shows, for `app` (an exe name in lower case) or everything together.
    pub fn view(&self, tracking: bool, app: Option<&str>) -> PadView {
        let d = &self.data;
        let kept = app.and_then(|exe| d.apps.iter().find(|a| a.exe == exe));
        let mut record = kept.map_or(&d.all, |a| &a.record).clone();
        let pending = self
            .held
            .iter()
            .filter(|h| {
                h.paused
                    && h.app.is_some()
                    && (kept.is_none() || h.app.as_deref() == app)
                    && self.filter.lift_share(h.bit) > 0.5
            })
            .count();
        record.lifts += pending as f64;
        let mut apps: Vec<AppSummary> = d
            .apps
            .iter()
            .map(|a| AppSummary {
                exe: a.exe.clone(),
                name: a.name.clone(),
                path: a.path.clone(),
                moving_ms: a.record.moving_ms,
            })
            .collect();
        apps.sort_by(|a, b| b.moving_ms.total_cmp(&a.moving_ms));
        let accuracy = (d.drift.len() >= 5).then(|| {
            let mut v = d.drift.clone();
            v.sort_by(|a, b| a.total_cmp(b));
            v[v.len() / 2] as f64
        });
        let grip = d.calibration.map_or(0.0, |c| c.angle) + self.filter.grip();
        PadView {
            calibration: d.calibration,
            home: d.home,
            home_spread: d.home_spread,
            cols: d.cols,
            rows: d.rows,
            app: kept.map(|a| a.exe.clone()),
            record,
            apps,
            pos: self.filter.mean(),
            spread: self.filter.spread(),
            accuracy,
            grip_deg: grip.to_degrees(),
            learned: d.learned,
            tracking,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sim::{self, CAL, Options};
    use super::*;

    const DPI: u32 = 1600;
    fn counts(cm: f64) -> i32 {
        (cm / 2.54 * DPI as f64).round() as i32
    }

    fn calibrated() -> Tracker {
        let mut t = Tracker::with_seed(PadData::new(CAL), 3);
        t.filter = Filter::new((0.0, 0.0), 3);
        t.set_app(Some(tag("game.exe")));
        t
    }

    /// Move `cm` along x in 1 ms reports, evenly, from `t0`.
    fn swipe(t: &mut Tracker, t0: Instant, cm: f64, ms: u64) -> Instant {
        let step = counts(cm) / ms as i32;
        for i in 1..=ms {
            t.report(t0 + Duration::from_millis(i), step, 0, DPI);
        }
        t0 + Duration::from_millis(ms)
    }

    /// Ease to a stop the way a hand does, moving `sign` along x.
    fn settle(t: &mut Tracker, mut now: Instant, sign: i32) -> Instant {
        for (gap, cm) in [(1, 0.05), (2, 0.04), (5, 0.03), (10, 0.02), (20, 0.01), (35, 0.005)] {
            now += Duration::from_millis(gap);
            t.report(now, sign * counts(cm).max(1), 0, DPI);
        }
        now
    }

    #[test]
    fn follows_movement_and_stops_at_the_edge() {
        let mut t = calibrated();
        let now = swipe(&mut t, Instant::now(), 10.0, 200);
        assert!((t.pos().0 - 10.0).abs() < 0.6, "{:?}", t.pos());
        swipe(&mut t, now, 10.0, 200);
        assert!((t.pos().0 - CAL.right).abs() < 1.0, "held at the right edge, {:?}", t.pos());
    }

    #[test]
    fn fast_cut_off_near_the_edge_reads_as_a_lift() {
        let mut t = calibrated();
        let end = swipe(&mut t, Instant::now(), 12.0, 60);
        swipe(&mut t, end + Duration::from_millis(300), 1.5, 30);
        let h = t.held.back().unwrap();
        assert!(h.chance > 0.8, "{}", h.chance);
        assert!(t.pos().0 < 9.0, "came down nearer the usual spot, {:?}", t.pos());
    }

    #[test]
    fn easing_to_a_stop_mid_pad_is_not_a_lift() {
        let mut t = calibrated();
        let end = swipe(&mut t, Instant::now(), 3.0, 150);
        let end = settle(&mut t, end, 1);
        swipe(&mut t, end + Duration::from_millis(300), -2.0, 100);
        let h = t.held.back().unwrap();
        assert!(h.chance < 0.5, "{}", h.chance);
        assert!(t.filter.lift_share(h.bit) < 0.5, "most guesses kept adding up");
    }

    /// Move `cm` along x over `ms`: up to speed in 15 ms, braking hard over the last `brake` ms.
    fn flick(t: &mut Tracker, t0: Instant, cm: f64, ms: u64, brake: u64) -> Instant {
        let speed = |i: u64| {
            let up = (i as f64 / 15.0).min(1.0);
            let down = ((ms - i) as f64 / brake as f64).min(1.0);
            up.min(down)
        };
        let total: f64 = (1..=ms).map(speed).sum();
        let mut carry = 0.0;
        for i in 1..=ms {
            carry += counts(cm) as f64 * speed(i) / total;
            let step = carry.trunc();
            carry -= step;
            if step != 0.0 {
                t.report(t0 + Duration::from_millis(i), step as i32, 0, DPI);
            }
        }
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn a_flick_that_brakes_hard_is_not_a_lift() {
        let mut t = calibrated();
        let mut now = Instant::now();
        // Flick 8 cm right toward the near edge, recoil 2 mm, aim; flick back and aim. Twenty times.
        for _ in 0..20 {
            now = flick(&mut t, now, 8.0, 60, 12);
            now = swipe(&mut t, now, -0.2, 20);
            now = flick(&mut t, now + Duration::from_millis(250), -8.0, 60, 12);
            now += Duration::from_millis(250);
            if let Some(h) = t.held.back() {
                assert!(h.chance < 0.3, "{}", h.chance);
            }
        }
        t.tick(now + Duration::from_secs(5));
        assert!(t.data.all.lifts < 1.0, "counted {} lifts", t.data.all.lifts);
    }

    fn tag(exe: &str) -> AppTag {
        AppTag { exe: exe.into(), name: exe.to_string(), path: format!(r"C:\Games\{exe}") }
    }

    #[test]
    fn keeps_each_game_on_its_own() {
        let mut t = calibrated();
        t.set_app(Some(tag("valorant.exe")));
        let now = swipe(&mut t, Instant::now(), 6.0, 100);
        let now = settle(&mut t, now, 1);
        // Not a game on the list: followed, not recorded.
        t.set_app(None);
        let now = swipe(&mut t, now + Duration::from_millis(300), -4.0, 100);
        assert!((t.pos().0 - 2.0).abs() < 0.6, "still followed, {:?}", t.pos());
        t.set_app(Some(tag("apex.exe")));
        let now = swipe(&mut t, now + Duration::from_millis(300), 2.0, 50);
        t.tick(now + Duration::from_secs(5));

        let all = t.view(false, None);
        assert!((all.record.distance_cm - 8.0).abs() < 0.2, "{}", all.record.distance_cm);
        let drawn: f32 = all.record.cells.iter().sum();
        assert!((drawn - 8.0).abs() < 0.5, "only the games' swipes are drawn, {drawn}");
        assert_eq!(all.record.reach_x.iter().sum::<u32>(), 2);
        let valorant = t.view(false, Some("valorant.exe"));
        assert_eq!(valorant.app.as_deref(), Some("valorant.exe"));
        assert!((valorant.record.distance_cm - 6.0).abs() < 0.2, "{}", valorant.record.distance_cm);
        let drawn: f32 = valorant.record.cells.iter().sum();
        assert!((drawn - 6.0).abs() < 0.4, "the map has its own swipe, {drawn}");
        assert_eq!(valorant.record.reach_x.iter().sum::<u32>(), 1);
        assert_eq!(all.apps.iter().map(|a| a.exe.as_str()).collect::<Vec<_>>(), ["valorant.exe", "apex.exe"]);
        // An app with nothing kept shows everything.
        assert_eq!(t.view(false, Some("notepad.exe")).app, None);
    }

    #[test]
    fn makes_room_by_dropping_the_least_used_app() {
        let mut t = calibrated();
        let mut now = Instant::now();
        for i in 0..=MAX_APPS {
            t.set_app(Some(tag(&format!("app{i}.exe"))));
            // app0 gets the least movement.
            now = swipe(
                &mut t,
                now + Duration::from_millis(300),
                if i % 2 == 0 { 3.0 } else { -3.0 },
                if i == 0 { 20 } else { 100 },
            );
        }
        let apps: Vec<String> = t.data.apps.iter().map(|a| a.exe.clone()).collect();
        assert_eq!(apps.len(), MAX_APPS);
        assert!(!apps.contains(&"app0.exe".to_string()), "{apps:?}");
        assert!(apps.contains(&format!("app{MAX_APPS}.exe")), "{apps:?}");
    }

    #[test]
    fn older_saved_data_loads_with_its_map() {
        let mut d = PadData::new(CAL);
        d.all.cells[5] = 2.0;
        d.all.distance_cm = 40.0;
        let json = serde_json::to_value(&d).unwrap();
        // The map's fields sit at the top level, as they did before apps were kept apart.
        assert_eq!(json["distanceCm"], 40.0);
        let dir = std::env::temp_dir().join(format!("hrtz-pad-{}", std::process::id()));
        let file = dir.join("pad.json");
        d.save(&file).unwrap();
        let back = PadData::load(&file);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(back.all.cells[5], 2.0);
        assert!(back.apps.is_empty());
    }

    #[test]
    fn the_edge_settles_a_repeated_swipe_as_a_lift() {
        let mut t = calibrated();
        // Ease into the right edge slowly, so nothing about the stop says "lift"...
        let end = swipe(&mut t, Instant::now(), 12.5, 600);
        let end = settle(&mut t, end, 1);
        // ...then carry on right, further than the pad allows without having moved back.
        let end = swipe(&mut t, end + Duration::from_millis(300), 6.0, 200);
        t.tick(end + Duration::from_secs(5));
        let share = t.pause_log.last().unwrap().1;
        assert!(share > 0.9, "edges say it was lifted, {share}");
        assert!(t.pos().0 < CAL.right - 1.0, "{:?}", t.pos());
    }

    struct Scores {
        live: f64,
        plain: f64,
        map: f64,
        plain_map: f64,
        precision: f64,
        recall: f64,
    }

    fn session(name: &str, o: &Options, merge: Option<Duration>) -> Scores {
        let s = sim::play(300.0, 11, o);
        let (t, est, t0) = sim::run(&s, merge, 5);
        let plain_est = sim::naive(&s);
        let truth = sim::path_map(&s, None);
        let sc = Scores {
            live: sim::rms(&s, &est, 30.0),
            plain: sim::rms(&s, &plain_est, 30.0),
            map: sim::overlap(&truth, &sim::tracker_map(&t)),
            plain_map: sim::overlap(&truth, &sim::path_map(&s, Some(&plain_est))),
            precision: 0.0,
            recall: 0.0,
        };
        let (precision, recall) = sim::lift_scores(&s, &t, t0);
        let sc = Scores { precision, recall, ..sc };
        let true_lifts = s.pauses.iter().filter(|p| p.1).count();
        eprintln!(
            "{name}: heatmap {:.0}% right (plain adding-up {:.0}%); live dot off by {:.1} cm (plain {:.1} cm); lifts {:.0} of {true_lifts}, precision {:.0}%, recall {:.0}%; learned from {}",
            sc.map * 100.0,
            sc.plain_map * 100.0,
            sc.live,
            sc.plain,
            t.data.all.lifts,
            sc.precision * 100.0,
            sc.recall * 100.0,
            t.data.learned
        );
        sc
    }

    #[test]
    fn session_in_front_at_8k() {
        let s = session("8K, 0.7 mm", &Options::default(), None);
        assert!(s.map > s.plain_map, "{} vs {}", s.map, s.plain_map);
        assert!(s.live < s.plain, "{} vs {}", s.live, s.plain);
    }

    #[test]
    fn session_with_lift_off_blips() {
        let o = Options { blips: true, dropouts: false, ..Options::default() };
        let s = session("8K, 1 mm", &o, None);
        assert!(s.map > s.plain_map, "{} vs {}", s.map, s.plain_map);
        assert!(s.live < s.plain, "{} vs {}", s.live, s.plain);
    }

    #[test]
    fn session_in_the_background() {
        let s = session("background, merged", &Options::default(), Some(Duration::from_millis(8)));
        assert!(s.map > s.plain_map, "{} vs {}", s.map, s.plain_map);
    }

    #[test]
    fn session_with_flicks() {
        let s = session("8K, flicks", &Options { flicks: 0.6, ..Options::default() }, None);
        assert!(s.map > s.plain_map, "{} vs {}", s.map, s.plain_map);
        assert!(s.precision > 0.75, "{}", s.precision);
    }

    #[test]
    fn session_with_flicks_in_the_background() {
        let s = session(
            "background, flicks",
            &Options { flicks: 0.6, ..Options::default() },
            Some(Duration::from_millis(8)),
        );
        assert!(s.precision > 0.75, "{}", s.precision);
    }

    /// Flicking around without ever lifting: what the user saw count up.
    #[test]
    fn flicking_without_lifting() {
        for (name, merge) in [("8K", None), ("background", Some(Duration::from_millis(8)))] {
            let o = Options { flicks: 1.0, lifts: false, ..Options::default() };
            let s = sim::play(300.0, 5, &o);
            let (t, _, _) = sim::run(&s, merge, 5);
            let rate = t.data.all.lifts / s.pauses.len() as f64;
            eprintln!("{name}, flicking without lifting: {} lifts in {} pauses", t.data.all.lifts, s.pauses.len());
            assert!(rate < 0.03, "{name}: {} lifts in {} pauses", t.data.all.lifts, s.pauses.len());
        }
    }
}

#[cfg(test)]
mod learning {
    use super::sim::{self, Options};
    use std::time::Duration;

    /// Does it get better with use? Scores each 5-minute stretch of a 20-minute session.
    /// `cargo test --lib pad::learning -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn improves_over_a_long_session() {
        for (name, o) in
            [("0.7 mm", Options::default()), ("1 mm", Options { blips: true, dropouts: false, ..Options::default() })]
        {
            let s = sim::play(1200.0, 21, &o);
            let (t, est, t0) = sim::run(&s, None, 9);
            for w in 0..4 {
                let (a, b) = (Duration::from_secs(w * 300), Duration::from_secs((w + 1) * 300));
                let (p, r) = sim::lift_scores_between(&s, &t, t0, a, b);
                let e = sim::rms_between(&s, &est, a, b);
                eprintln!(
                    "{name} minutes {}-{}: live dot off by {e:.1} cm, lift precision {:.0}%, recall {:.0}%",
                    w * 5,
                    w * 5 + 5,
                    p * 100.0,
                    r * 100.0
                );
            }
            eprintln!(
                "{name}: weights {:?}, learned from {}",
                t.data.weights.iter().map(|w| (w * 100.0).round() / 100.0).collect::<Vec<_>>(),
                t.data.learned
            );
        }
    }
}

#[cfg(test)]
mod diagnose {
    use super::sim::{self, Options};
    use std::time::Duration;

    /// Averages of the inputs over pauses called lifts and over the rest.
    /// `cargo test --lib pad::diagnose -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn false_lifts_while_flicking() {
        for (name, merge) in [("8K", None), ("background", Some(Duration::from_millis(8)))] {
            let o = Options { flicks: 1.0, lifts: false, ..Options::default() };
            let s = sim::play(300.0, 5, &o);
            let (t, _, _) = sim::run(&s, merge, 5);
            let (mut called, mut rest) = (vec![0.0; 16], vec![0.0; 16]);
            let (mut nc, mut nr) = (0.0, 0.0);
            let mut how = [0; 4];
            for (x, prior, chance, share) in &t.pause_detail {
                let (acc, n) = if *share > 0.5 { (&mut called, &mut nc) } else { (&mut rest, &mut nr) };
                *n += 1.0;
                for (a, v) in acc.iter_mut().zip(x.iter().chain([prior, chance, share])) {
                    *a += v;
                }
                if *share > 0.5 {
                    how[(*prior > 0.5) as usize * 2 + (*chance > 0.5) as usize] += 1;
                }
            }
            let avg =
                |v: &[f64], n: f64| v.iter().map(|a| (a / n.max(1.0) * 100.0).round() / 100.0).collect::<Vec<_>>();
            eprintln!("{name}: called {nc}, rest {nr}");
            eprintln!("  inputs + prior, chance, share when called: {:?}", avg(&called, nc));
            eprintln!("  the rest:                                  {:?}", avg(&rest, nr));
            eprintln!(
                "  called with prior/chance low-low {}, low-high {}, high-low {}, high-high {}",
                how[0], how[1], how[2], how[3]
            );
        }
    }
}

#[cfg(test)]
mod lab {
    use super::sim::{self, Options};
    use super::{AppTag, PadData, Tracker};
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// Pad views for the Lab page's lab (`lab-labpage.html`): yours as saved, and a simulated
    /// 20 minutes of Valorant plus 8 of Apex from the app's own simulator and tracker. Writes
    /// `src/lab/data/pad-*.json`. Goes with the labs.
    /// `cargo test --lib pad::lab -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_lab_pad() {
        let out = PathBuf::from("../src/lab/data");
        std::fs::create_dir_all(&out).unwrap();
        let write = |name: &str, v: serde_json::Value| {
            std::fs::write(out.join(name), format!("{}\n", serde_json::to_string_pretty(&v).unwrap())).unwrap()
        };

        let saved = PathBuf::from(std::env::var("APPDATA").unwrap()).join(r"com.azaek.hrtz\pad.json");
        let yours = Tracker::new(PadData::load(&saved));
        write("pad-yours.json", serde_json::to_value(yours.view(false, None)).unwrap());

        let first = sim::play(1200.0, 21, &Options::default());
        let mut t = Tracker::with_seed(PadData::new(first.cal), 9);
        t.filter = super::filter::Filter::new((0.0, 0.0), 9);
        let t0 = Instant::now();
        let mut offset = Duration::ZERO;
        let games = [
            ("valorant-win64-shipping.exe", "VALORANT", first),
            ("r5apex_dx12.exe", "Apex Legends", sim::play(480.0, 7, &Options { flicks: 0.3, ..Options::default() })),
        ];
        for (exe, name, s) in &games {
            t.set_app(Some(AppTag { exe: (*exe).into(), name: (*name).into(), path: String::new() }));
            for &(at, dx, dy) in &s.reports {
                t.report(t0 + offset + at, dx, dy, 1600);
                t.tick(t0 + offset + at);
            }
            offset += s.reports.last().map_or(Duration::ZERO, |r| r.0) + Duration::from_secs(60);
            t.tick(t0 + offset);
        }
        let mut views = serde_json::Map::new();
        views.insert("all".into(), serde_json::to_value(t.view(false, None)).unwrap());
        views.insert("tracking".into(), serde_json::to_value(t.view(true, None)).unwrap());
        for (exe, _, _) in &games {
            views.insert((*exe).into(), serde_json::to_value(t.view(false, Some(exe))).unwrap());
        }
        write("pad-sim.json", serde_json::Value::Object(views));
        eprintln!(
            "lifts {:.0}, accuracy {:?}, learned {}",
            t.data.all.lifts,
            t.view(false, None).accuracy,
            t.data.learned
        );
    }
}
