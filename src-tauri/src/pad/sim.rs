//! A simulated player, for testing the tracker against true positions and lifts.
//!
//! Plays on a 43 × 30 cm area (what a mouse's sensor can reach on a 49 × 42 cm pad) with its usual
//! spot 8 cm right of centre and 5 cm up. Swipes are mostly sideways. It lifts when it runs low on
//! room, and sometimes for no reason, coming down near its spot or back against the swipe.
//!
//! Some swipes can be flicks: fast, braking hard at the target, often with a small recoil back.

use std::time::{Duration, Instant};

use super::filter::Rng;
use super::{Calibration, PadData, Tracker};

pub struct Options {
    pub dpi: u32,
    /// Reports per second while moving: 8000 in front, about 125 merged in the background.
    pub rate: f64,
    /// Grip angle: the sensor's axes turned from the pad's (radians).
    pub grip: f64,
    /// Lift-off high enough (1 mm and up) to track a little while rising and landing.
    pub blips: bool,
    /// Lift-off low enough (0.7 mm) that fast swipes sometimes drop out mid-swipe.
    pub dropouts: bool,
    /// Share of swipes that are flicks.
    pub flicks: f64,
    /// Lifts at all. Without, a swipe that won't fit turns back instead.
    pub lifts: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            dpi: 1600,
            rate: 8000.0,
            grip: 7f64.to_radians(),
            blips: false,
            dropouts: true,
            flicks: 0.0,
            lifts: true,
        }
    }
}

pub struct Session {
    pub cal: Calibration,
    pub blips: bool,
    pub reports: Vec<(Duration, i32, i32)>,
    /// True position every 50 ms while moving.
    pub truth: Vec<(Duration, (f64, f64))>,
    /// Every pause between swipes: when movement resumed, and whether the mouse was lifted.
    pub pauses: Vec<(Duration, bool)>,
}

/// Where the pad ends, from the player's spot.
pub const CAL: Calibration = Calibration { angle: 0.0, left: 29.5, right: 13.5, top: 10.0, bottom: 20.0, version: 2 };

struct Out<'a> {
    o: &'a Options,
    s: Session,
    t: f64,
    carry: (f64, f64),
    last_truth: f64,
}

/// How speed rises and falls over a movement.
#[derive(Clone, Copy)]
enum Profile {
    Even,
    /// Smooth start and stop, the way a hand eases a swipe in.
    Hand,
    /// Up to speed over the first share of the time, braking over the last.
    Flick(f64, f64),
}

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl Profile {
    fn speed(self, u: f64) -> f64 {
        match self {
            Profile::Even => 1.0,
            Profile::Hand => 30.0 * u * u * (1.0 - u) * (1.0 - u),
            Profile::Flick(up, brake) => smooth(u / up) * (1.0 - smooth((u - (1.0 - brake)) / brake)),
        }
    }
}

impl Out<'_> {
    /// Move by (dx, dy) cm on the pad over `secs`, reporting at the player's rate. `reported`
    /// false moves the mouse without any reports (it's in the air).
    fn travel(&mut self, pos: &mut (f64, f64), dx: f64, dy: f64, secs: f64, reported: bool, profile: Profile) {
        self.travel_until(pos, dx, dy, secs, reported, profile, 1.0);
    }

    /// `travel`, stopped early at `cut` of the way through its timing.
    #[allow(clippy::too_many_arguments)]
    fn travel_until(
        &mut self,
        pos: &mut (f64, f64),
        dx: f64,
        dy: f64,
        secs: f64,
        reported: bool,
        profile: Profile,
        cut: f64,
    ) {
        let n = ((secs * self.o.rate).ceil() as usize).max(1);
        let counts_per_cm = self.o.dpi as f64 / 2.54;
        let (c, s) = (self.o.grip.cos(), self.o.grip.sin());
        let speeds: Vec<f64> = (0..n).map(|i| profile.speed((i as f64 + 0.5) / n as f64)).collect();
        let total = speeds.iter().sum::<f64>().max(1e-12);
        let steps = ((n as f64 * cut).ceil() as usize).clamp(1, n);
        let start = *pos;
        let mut f = 0.0;
        for &v in &speeds[..steps] {
            let df = v / total;
            f += df;
            let (mx, my) = (dx * df, dy * df);
            self.t += secs / n as f64;
            *pos = ((start.0 + dx * f).clamp(-CAL.left, CAL.right), (start.1 + dy * f).clamp(-CAL.top, CAL.bottom));
            if !reported {
                continue;
            }
            // Pad movement to sensor counts: the sensor is turned by the grip angle.
            let (sx, sy) = (mx * c + my * s, -mx * s + my * c);
            self.carry.0 += sx * counts_per_cm;
            self.carry.1 += sy * counts_per_cm;
            let (rx, ry) = (self.carry.0.trunc() as i32, self.carry.1.trunc() as i32);
            if rx != 0 || ry != 0 {
                self.carry.0 -= rx as f64;
                self.carry.1 -= ry as f64;
                self.s.reports.push((Duration::from_secs_f64(self.t), rx, ry));
            }
            if self.t - self.last_truth >= 0.05 {
                self.s.truth.push((Duration::from_secs_f64(self.t), *pos));
                self.last_truth = self.t;
            }
        }
    }
}

fn ray(pos: (f64, f64), dir: (f64, f64)) -> f64 {
    let along = |p: f64, u: f64, lo: f64, hi: f64| {
        if u > 1e-6 {
            (hi - p) / u
        } else if u < -1e-6 {
            (lo - p) / u
        } else {
            f64::INFINITY
        }
    };
    along(pos.0, dir.0, -CAL.left, CAL.right).min(along(pos.1, dir.1, -CAL.top, CAL.bottom)).max(0.0)
}

pub fn play(seconds: f64, seed: u64, o: &Options) -> Session {
    let mut rng = Rng::new(seed);
    let mut out = Out {
        o,
        s: Session {
            cal: Calibration { angle: o.grip, ..CAL },
            blips: o.blips,
            reports: Vec::new(),
            truth: Vec::new(),
            pauses: Vec::new(),
        },
        t: 0.0,
        carry: (0.0, 0.0),
        last_truth: 0.0,
    };
    let mut pos = (0.0, 0.0);
    let mut last_dir = (1.0, 0.0);
    let mut next = plan(&mut rng, last_dir);
    let mut lift = false;
    let mut flick = false;
    while out.t < seconds {
        let Plan { mut dir, len, speed } = next;
        let pause = if lift {
            0.25 + rng.unit() * 0.2
        } else if flick {
            // Aiming and shooting after a flick.
            0.05 + rng.unit() * 0.55
        } else {
            0.08 + rng.unit() * 1.4
        };
        if lift {
            if o.blips {
                // Still tracking for a moment as the mouse rises, moving back.
                out.travel(&mut pos, -last_dir.0 * 0.2, -last_dir.1 * 0.2, 0.02, true, Profile::Even);
            }
            out.t += pause;
            let (z1, z2) = (rng.normal(), rng.normal());
            pos = if rng.unit() < 0.7 {
                (z1 * 2.5, z2 * 2.5)
            } else {
                let d = (9.0 + 3.0 * z1).max(2.0);
                (pos.0 - last_dir.0 * d, pos.1 - last_dir.1 * d + z2)
            };
            pos = (pos.0.clamp(-CAL.left, CAL.right), pos.1.clamp(-CAL.top, CAL.bottom));
            out.s.pauses.push((Duration::from_secs_f64(out.t), true));
            if o.blips {
                out.travel(&mut pos, -last_dir.0 * 0.2, -last_dir.1 * 0.2, 0.02, true, Profile::Even);
            }
        } else {
            out.t += pause;
            out.s.pauses.push((Duration::from_secs_f64(out.t), false));
        }

        // Without lifting, a swipe that won't fit turns back.
        if !o.lifts && ray(pos, dir) < len {
            dir = (-dir.0, -dir.1);
        }
        // The swipe itself; it stops at the pad's edge if it runs out.
        flick = rng.unit() < o.flicks;
        let len = if flick { 2.0 + rng.unit() * 12.0 } else { len };
        let len = len.min(ray(pos, dir));
        // A flick still takes a hand at least 45 ms, and up to about 2.5 m/s on top.
        let secs = if flick { 0.045 + len / (120.0 + rng.unit() * 130.0) } else { len / speed };
        let end = (pos.0 + dir.0 * len, pos.1 + dir.1 * len);
        // Decide what comes after, since a lift next can cut this swipe off while it's moving.
        next = plan(&mut rng, dir);
        lift = o.lifts && ((ray(end, next.dir) < next.len + 1.5 && rng.unit() < 0.85) || rng.unit() < 0.12);
        let cut = if lift && rng.unit() < 0.3 { 0.75 } else { 1.0 };
        if o.dropouts && speed > 40.0 && rng.unit() < 0.02 {
            out.travel(&mut pos, dir.0 * len * 0.5, dir.1 * len * 0.5, secs * 0.5, true, Profile::Even);
            out.travel(&mut pos, dir.0 * len * 0.1, dir.1 * len * 0.1, 0.06, false, Profile::Even);
            out.travel(&mut pos, dir.0 * len * 0.4, dir.1 * len * 0.4, secs * 0.4, true, Profile::Even);
        } else if flick {
            let shape = Profile::Flick(0.2 + rng.unit() * 0.15, 0.25 + rng.unit() * 0.2);
            out.travel_until(&mut pos, dir.0 * len, dir.1 * len, secs, true, shape, cut);
            // Overshooting the target and pulling back onto it.
            if cut == 1.0 && rng.unit() < 0.5 {
                let back = 0.05 + rng.unit() * 0.25;
                out.travel(&mut pos, -dir.0 * back, -dir.1 * back, 0.02 + rng.unit() * 0.03, true, Profile::Hand);
            }
        } else {
            out.travel_until(&mut pos, dir.0 * len, dir.1 * len, secs, true, Profile::Hand, cut);
        }
        last_dir = dir;
    }
    out.s
}

#[derive(Clone, Copy)]
struct Plan {
    dir: (f64, f64),
    len: f64,
    speed: f64,
}

/// The next swipe: usually carrying on the way the last one went (turning further, tracking),
/// otherwise mostly sideways.
fn plan(rng: &mut Rng, last: (f64, f64)) -> Plan {
    let r = rng.unit();
    let angle = if r < 0.6 {
        last.1.atan2(last.0) + rng.normal() * 0.12
    } else if r < 0.85 {
        (if rng.unit() < 0.5 { 0.0 } else { std::f64::consts::PI }) + rng.normal() * 0.17
    } else {
        rng.unit() * std::f64::consts::TAU
    };
    Plan {
        dir: (angle.cos(), angle.sin()),
        len: (6.0 * (0.7 * rng.normal()).exp()).clamp(1.0, 35.0),
        speed: 25.0 + rng.unit() * 65.0,
    }
}

/// A 2 cm grid over the pad, for comparing movement maps.
fn block(x: f64, y: f64) -> usize {
    let cols = ((CAL.left + CAL.right) / 2.0).ceil() as usize;
    let rows = ((CAL.top + CAL.bottom) / 2.0).ceil() as usize;
    let c = (((x + CAL.left) / 2.0).floor().max(0.0) as usize).min(cols - 1);
    let r = (((y + CAL.top) / 2.0).floor().max(0.0) as usize).min(rows - 1);
    r * cols + c
}

fn blocks() -> usize {
    ((CAL.left + CAL.right) / 2.0).ceil() as usize * ((CAL.top + CAL.bottom) / 2.0).ceil() as usize
}

/// Where the mouse really moved, or where `est` (one position per truth sample) says it did.
pub fn path_map(s: &Session, est: Option<&[(f64, f64)]>) -> Vec<f64> {
    let mut m = vec![0.0; blocks()];
    for i in 1..s.truth.len() {
        let ((t0, a), (t1, b)) = (s.truth[i - 1], s.truth[i]);
        if t1 - t0 > Duration::from_millis(60) {
            continue;
        }
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let at = match est {
            Some(e) => ((e[i - 1].0 + e[i].0) / 2.0, (e[i - 1].1 + e[i].1) / 2.0),
            None => ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
        };
        m[block(at.0, at.1)] += len;
    }
    m
}

/// The tracker's committed heatmap on the same grid.
pub fn tracker_map(t: &Tracker) -> Vec<f64> {
    let mut m = vec![0.0; blocks()];
    for (i, v) in t.data.all.cells.iter().enumerate() {
        let (c, r) = (i % t.data.cols, i / t.data.cols);
        m[block(c as f64 + 0.5 - CAL.left, r as f64 + 0.5 - CAL.top)] += *v as f64;
    }
    m
}

/// How much two maps agree once each is scaled to the same total: 1 identical, 0 nothing shared.
pub fn overlap(a: &[f64], b: &[f64]) -> f64 {
    let (sa, sb) = (a.iter().sum::<f64>().max(1e-9), b.iter().sum::<f64>().max(1e-9));
    a.iter().zip(b).map(|(x, y)| (x / sa).min(y / sb)).sum()
}

/// Feed a session to a tracker, merging reports into `merge`-long groups like Windows does for
/// background listeners. Returns the tracker, its estimate at each truth sample, and its clock's
/// start.
pub fn run(s: &Session, merge: Option<Duration>, seed: u64) -> (Tracker, Vec<(f64, f64)>, Instant) {
    let mut t = Tracker::with_seed(PadData::new(s.cal), seed);
    t.filter = super::filter::Filter::new((0.0, 0.0), seed);
    t.blips = s.blips;
    t.set_app(Some(super::AppTag { exe: "game.exe".into(), name: "Game".into(), path: String::new() }));
    let t0 = Instant::now();
    let mut est = Vec::with_capacity(s.truth.len());
    let mut ti = 0;
    let mut group: Option<(Duration, i32, i32)> = None;
    for &(at, dx, dy) in &s.reports {
        while ti < s.truth.len() && s.truth[ti].0 <= at {
            est.push(t.pos());
            ti += 1;
        }
        match (merge, group) {
            (None, _) => t.report(t0 + at, dx, dy, 1600),
            (Some(m), Some((first, gx, gy))) if at - first < m => group = Some((first, gx + dx, gy + dy)),
            (Some(_), Some((first, gx, gy))) => {
                t.report(t0 + first, gx, gy, 1600);
                group = Some((at, dx, dy));
            }
            (Some(_), None) => group = Some((at, dx, dy)),
        }
        t.tick(t0 + at);
    }
    if let Some((first, gx, gy)) = group {
        t.report(t0 + first, gx, gy, 1600);
    }
    while est.len() < s.truth.len() {
        est.push(t.pos());
    }
    t.tick(t0 + s.reports.last().map_or(Duration::ZERO, |r| r.0) + Duration::from_secs(10));
    (t, est, t0)
}

/// Root-mean-square distance between truth and estimate after `warmup` seconds.
pub fn rms(s: &Session, est: &[(f64, f64)], warmup: f64) -> f64 {
    let pairs: Vec<f64> = s
        .truth
        .iter()
        .zip(est)
        .filter(|((t, _), _)| t.as_secs_f64() > warmup)
        .map(|((_, a), b)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2))
        .collect();
    (pairs.iter().sum::<f64>() / pairs.len().max(1) as f64).sqrt()
}

/// Precision and recall of the tracker's lift calls (chance above one half) against the truth,
/// matching each true pause to the settled pause whose swipe began within 40 ms of it.
pub fn lift_scores(s: &Session, t: &Tracker, t0: Instant) -> (f64, f64) {
    let (mut tp, mut fp, mut missed) = (0.0f64, 0.0f64, 0.0f64);
    for &(at, lifted) in &s.pauses {
        let called = t
            .pause_log
            .iter()
            .find(|(when, _)| when.duration_since(t0).abs_diff(at) < Duration::from_millis(40))
            .is_some_and(|(_, share)| *share > 0.5);
        match (lifted, called) {
            (true, true) => tp += 1.0,
            (false, true) => fp += 1.0,
            (true, false) => missed += 1.0,
            _ => {}
        }
    }
    (tp / (tp + fp).max(1.0), tp / (tp + missed).max(1.0))
}

/// Plain adding-up with the edges as hard stops: what the tracker would do without lift detection.
pub fn naive(s: &Session) -> Vec<(f64, f64)> {
    let (c, sn) = (s.cal.angle.cos(), s.cal.angle.sin());
    let k = 2.54 / 1600.0;
    let mut pos = (0.0, 0.0);
    let mut est = Vec::with_capacity(s.truth.len());
    let mut ti = 0;
    for &(at, dx, dy) in &s.reports {
        while ti < s.truth.len() && s.truth[ti].0 <= at {
            est.push(pos);
            ti += 1;
        }
        let (sx, sy) = (dx as f64 * k, dy as f64 * k);
        pos = (
            (pos.0 + sx * c - sy * sn).clamp(-CAL.left, CAL.right),
            (pos.1 + sx * sn + sy * c).clamp(-CAL.top, CAL.bottom),
        );
    }
    while est.len() < s.truth.len() {
        est.push(pos);
    }
    est
}

/// `lift_scores` for the pauses that resumed between `from` and `to`.
pub fn lift_scores_between(s: &Session, t: &Tracker, t0: Instant, from: Duration, to: Duration) -> (f64, f64) {
    let part = Session {
        cal: s.cal,
        blips: s.blips,
        reports: Vec::new(),
        truth: Vec::new(),
        pauses: s.pauses.iter().copied().filter(|p| p.0 >= from && p.0 < to).collect(),
    };
    lift_scores(&part, t, t0)
}

/// `rms` over truth samples between `from` and `to`.
pub fn rms_between(s: &Session, est: &[(f64, f64)], from: Duration, to: Duration) -> f64 {
    let pairs: Vec<f64> = s
        .truth
        .iter()
        .zip(est)
        .filter(|((t, _), _)| *t >= from && *t < to)
        .map(|((_, a), b)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2))
        .collect();
    (pairs.iter().sum::<f64>() / pairs.len().max(1) as f64).sqrt()
}
