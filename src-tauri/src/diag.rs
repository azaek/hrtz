//! Diagnostics on the mouse's own reports, for the Lab page's connection and sensor tests.
//!
//! - [`Conn`] follows a connection test run: polls that came without a report while the movement
//!   was fast enough to fill every one, the gaps by size, and whether they come at a steady rhythm
//!   or in bursts.
//! - [`Strokes`] splits the movement into strokes, from a stop or a turn to the next, and says what
//!   the sensor did along each: how far and how fast it went, reports pointing the wrong way at
//!   speed, gaps, and stalls.
//!
//! Times are when the app read each report, not when it arrived. While the app is busy, reports
//! queue and are then read all at once, so a gap followed by a burst is the app's own delay. Both
//! parts tell that apart from a real gap.

use std::collections::VecDeque;

use serde::Serialize;

// ---- Connection --------------------------------------------------------------------------------

/// Movement fills every poll in practice at this many counts a poll, on average either side of a
/// gap. Slower, a poll can come up empty because there was nothing to report.
const FILL_COUNTS: f64 = 2.0;
/// Gaps this long (and at least 4 polls) are stutters, as on the polling rate test's chart.
const STUTTER_S: f64 = 0.002;
/// The pattern is read from this many of the latest gaps.
const PATTERN_GAPS: usize = 4096;
/// Gaps of this many polls or more also get a pattern of their own: the gaps of one or two polls
/// that a radio carrying fewer reports than the rate leaves are many, steady, and would hide how
/// the long ones fall.
const LONG_GAP: u32 = 4;

/// Gaps are binned by polls missed: 1, 2, 3, 4–7, 8–15, 16 or more.
fn bin(missed: u32) -> usize {
    match missed {
        0 | 1 => 0,
        2 => 1,
        3 => 2,
        4..=7 => 3,
        8..=15 => 4,
        _ => 5,
    }
}

#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    /// Under 8 gaps: too few to say.
    #[default]
    Few,
    /// At a steady rhythm: the mouse's own timing.
    Regular,
    /// In bunches, with clean stretches between: interference on the radio.
    Bursts,
    /// Scattered at random.
    Scattered,
}

/// A connection test run so far. Only movement fast enough to fill every poll counts.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnStats {
    /// Milliseconds of movement that counted.
    pub ms: f64,
    /// Reports that came in that time, and polls that came without one.
    pub reports: u32,
    pub missed: u32,
    /// Gaps by polls missed: 1, 2, 3, 4–7, 8–15, 16 or more.
    pub gaps: [u32; 6],
    /// Gaps of 2 ms or more.
    pub stutters: u32,
    /// The longest gap in microseconds, or 0 without one.
    pub longest_us: u32,
    pub pattern: Pattern,
    /// For a steady rhythm: how often a gap comes, in milliseconds.
    pub every_ms: Option<f64>,
    /// The same for gaps of 4 polls or more.
    pub long_pattern: Pattern,
    pub long_every_ms: Option<f64>,
}

/// A report still waiting on the ones after it.
struct Pending {
    i: u64,
    t: f64,
    counts: f64,
    /// Polls gone by since the stretch began, less reports read: it rises by one for each poll
    /// that came without a report, and for as long as the app reads late.
    d: f64,
}

#[derive(Default)]
struct Run {
    /// Polls that counted, each with a report or without.
    polls: f64,
    reports: u32,
    missed: u32,
    gaps: [u32; 6],
    stutters: u32,
    /// Stutters since the last tick took them.
    new_stutters: u32,
    longest: f64,
    /// Each gap: its stretch, the poll it ended on, and the polls it missed.
    log: VecDeque<(u32, f64, u32)>,
    /// The same for the gaps of 4 polls or more.
    long_log: VecDeque<(u32, f64, u32)>,
}

/// A connection test run, fed one movement report at a time.
pub struct Conn {
    period: f64,
    /// The stretch of movement in progress: when it began, and its next report's number.
    t0: f64,
    next: u64,
    last: Option<f64>,
    /// Reports waiting on what follows them.
    line: VecDeque<Pending>,
    /// Rising lows of `d` over `line`, for the lowest point ahead of each report.
    lows: VecDeque<(u64, f64)>,
    /// The report decided last: its time, number and level.
    prev: Option<(f64, u64, f64)>,
    /// Reports decided within the last speed window: time and counts.
    recent: VecDeque<(f64, f64)>,
    stretch: u32,
    run: Run,
}

/// Pattern input, copied out so the reading can happen without holding up the reports.
pub struct ConnSnapshot {
    stats: ConnStats,
    log: Vec<(u32, f64, u32)>,
    long_log: Vec<(u32, f64, u32)>,
    period: f64,
}

impl ConnSnapshot {
    pub fn stats(mut self) -> ConnStats {
        let ms = |polls: f64| polls * self.period * 1000.0;
        let (all, every) = pattern(&self.log);
        let (long, long_every) = pattern(&self.long_log);
        self.stats.pattern = all;
        self.stats.every_ms = every.map(ms);
        self.stats.long_pattern = long;
        self.stats.long_every_ms = long_every.map(ms);
        self.stats
    }
}

impl Conn {
    pub fn new(hz: u32) -> Self {
        Self {
            period: 1.0 / hz.max(1) as f64,
            t0: 0.0,
            next: 0,
            last: None,
            line: VecDeque::new(),
            lows: VecDeque::new(),
            prev: None,
            recent: VecDeque::new(),
            stretch: 0,
            run: Run::default(),
        }
    }

    /// Speeds either side of a gap are measured over this long.
    fn window(&self) -> f64 {
        (8.0 * self.period).max(0.004)
    }

    /// A report is decided once this much has come after it: reports the app read late arrive
    /// together well within it.
    fn lookahead(&self) -> f64 {
        (16.0 * self.period).max(0.030)
    }

    /// A stop this long ends a stretch of movement.
    fn stretch_end(&self) -> f64 {
        (10.0 * self.period).max(0.050)
    }

    /// The polling rate gaps are measured against. A new one starts a new run: gaps counted at one
    /// rate don't add up with another's, and reports that came before the change go with it.
    pub fn set_rate(&mut self, hz: u32) {
        let period = 1.0 / hz.max(1) as f64;
        if (period - self.period).abs() > 1e-12 {
            self.restart();
            self.run = Run::default();
            self.period = period;
        }
    }

    /// A new run; the stretch in progress carries on.
    pub fn reset(&mut self) {
        self.run = Run::default();
    }

    /// One movement report, read at `t` seconds, with `counts` of movement (|dx| + |dy|).
    pub fn push(&mut self, t: f64, counts: f64) {
        if self.last.is_some_and(|last| t - last >= self.stretch_end()) {
            self.flush();
            self.restart();
        }
        if self.last.is_none() {
            self.t0 = t;
        }
        let d = (t - self.t0) / self.period - self.next as f64;
        while self.lows.back().is_some_and(|&(_, low)| low >= d) {
            self.lows.pop_back();
        }
        self.lows.push_back((self.next, d));
        self.line.push_back(Pending { i: self.next, t, counts, d });
        self.next += 1;
        self.last = Some(t);
        let ahead = self.lookahead();
        while self.line.front().is_some_and(|p| t - p.t >= ahead) {
            self.decide();
        }
    }

    /// Nothing has come since `now`: once the mouse has been still long enough, decide what's
    /// waiting.
    pub fn idle(&mut self, now: f64) {
        if self.last.is_some_and(|last| now - last >= self.stretch_end()) {
            self.flush();
            self.restart();
        }
    }

    /// The app went behind another window, where Windows hands it reports about 125 times a second,
    /// merged: decide what's waiting, and start afresh once it's back in front.
    pub fn pause(&mut self) {
        self.flush();
        self.restart();
    }

    /// Stutters decided since the last call.
    pub fn take_stutters(&mut self) -> u32 {
        std::mem::take(&mut self.run.new_stutters)
    }

    /// The run so far; its pattern is read from the snapshot.
    pub fn snapshot(&self) -> ConnSnapshot {
        let r = &self.run;
        ConnSnapshot {
            stats: ConnStats {
                ms: r.polls * self.period * 1000.0,
                reports: r.reports,
                missed: r.missed,
                gaps: r.gaps,
                stutters: r.stutters,
                longest_us: (r.longest * 1e6).round() as u32,
                pattern: Pattern::Few,
                every_ms: None,
                long_pattern: Pattern::Few,
                long_every_ms: None,
            },
            log: r.log.iter().copied().collect(),
            long_log: r.long_log.iter().copied().collect(),
            period: self.period,
        }
    }

    fn flush(&mut self) {
        while !self.line.is_empty() {
            self.decide();
        }
    }

    fn restart(&mut self) {
        self.next = 0;
        self.last = None;
        self.prev = None;
        self.line.clear();
        self.lows.clear();
        self.recent.clear();
        self.stretch = self.stretch.wrapping_add(1);
    }

    fn decide(&mut self) {
        let Some(p) = self.line.pop_front() else { return };
        while self.lows.front().is_some_and(|&(i, _)| i < p.i) {
            self.lows.pop_front();
        }
        // The lowest level from here on. A report the app read late sits above it, and the burst
        // that follows brings it back down; only polls that really came without a report raise it.
        let level = self.lows.front().map_or(p.d, |&(_, low)| low.min(p.d));
        let w = self.window();
        if let Some((pt, _, prev_level)) = self.prev {
            let need = FILL_COUNTS / self.period;
            let before = self.recent.iter().filter(|(t, _)| *t > pt - w).map(|(_, c)| c).sum::<f64>() / w;
            let after = (p.counts + self.line.iter().take_while(|q| q.t < p.t + w).map(|q| q.counts).sum::<f64>()) / w;
            if before >= need && after >= need {
                let rise = level - prev_level;
                let missed = if rise >= 0.5 { rise.round() as u32 } else { 0 };
                self.count(missed, p.i as f64 + level);
            }
        }
        self.prev = Some((p.t, p.i, level));
        self.recent.push_back((p.t, p.counts));
        while self.recent.front().is_some_and(|(t, _)| *t <= p.t - w) {
            self.recent.pop_front();
        }
    }

    fn count(&mut self, missed: u32, poll: f64) {
        let period = self.period;
        let stretch = self.stretch;
        let r = &mut self.run;
        r.reports += 1;
        r.polls += 1.0 + missed as f64;
        if missed == 0 {
            return;
        }
        r.missed += missed;
        r.gaps[bin(missed)] += 1;
        let gap = (missed + 1) as f64 * period;
        r.longest = r.longest.max(gap);
        if gap >= STUTTER_S.max(4.0 * period) {
            r.stutters += 1;
            r.new_stutters += 1;
        }
        for (log, keep) in [(&mut r.log, true), (&mut r.long_log, missed >= LONG_GAP)] {
            if keep {
                log.push_back((stretch, poll, missed));
                if log.len() > PATTERN_GAPS {
                    log.pop_front();
                }
            }
        }
    }
}

/// How the gaps fall, from the polls between one and the next in the same stretch of movement.
/// For a steady rhythm, also how many polls apart they come.
fn pattern(log: &[(u32, f64, u32)]) -> (Pattern, Option<f64>) {
    let apart: Vec<f64> =
        log.windows(2).filter(|w| w[0].0 == w[1].0).map(|w| w[1].1 - w[0].1).filter(|x| *x > 0.0).collect();
    if log.len() < 8 || apart.len() < 6 {
        return (Pattern::Few, None);
    }
    let mut sorted = apart.clone();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted[sorted.len() / 2];
    // Within 15% of the usual spacing, or a poll either way when they come close together.
    let steady = apart.iter().filter(|x| (*x - mid).abs() <= (mid * 0.15).max(1.0)).count();
    if steady * 10 >= apart.len() * 7 {
        return (Pattern::Regular, Some(mid));
    }
    let mean = apart.iter().sum::<f64>() / apart.len() as f64;
    let sd = (apart.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / apart.len() as f64).sqrt();
    if sd > 1.5 * mean { (Pattern::Bursts, None) } else { (Pattern::Scattered, None) }
}

// ---- Strokes -----------------------------------------------------------------------------------

/// A stop this long ends a stroke: the mouse was still, or in the air.
const STROKE_PAUSE: f64 = 0.150;
/// Coming back this far from the farthest point is a turn, and starts the next stroke (mm).
const TURN_MM: f64 = 3.0;
/// Speeds are averaged over this long, so reports the app read in a burst don't read as a burst of
/// speed.
const SPEED_S: f64 = 0.010;
/// Above this speed (m/s) a report pointing back is the sensor losing track.
const FLIP_MPS: f64 = 1.0;
/// Reports this close to a stroke's far end can point back for real: the mouse bouncing off a
/// stop (mm).
const FLIP_END_MM: f64 = 5.0;
/// A gap this long while moving fast enough to fill every poll is a dropout.
const DROPOUT_S: f64 = 0.002;
/// A stall is a stop mid-stroke of at least this long, and of this many reports' worth at the
/// speed it was going.
const STALL_S: f64 = 0.008;
const STALL_REPORTS: f64 = 6.0;
/// Speed and direction either side of a stall are measured over this long.
const STALL_SIDE_S: f64 = 0.030;
/// Slower than this (cm/s), the hand may simply be holding still.
const STALL_MIN_CMS: f64 = 0.5;
/// The most reports kept in one stroke: 10 s at 8,000 Hz.
const STROKE_MAX: usize = 80_000;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum StrokeEnd {
    /// It stopped, or left the surface, for 150 ms or more.
    Pause,
    /// It turned back.
    Turn,
    /// It went on too long, or the test stopped listening.
    Cut,
}

/// One stroke, from a stop or a turn to the next. Distances are in sensor counts.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stroke {
    /// Microseconds since the listener started.
    pub start_us: u64,
    pub end_us: u64,
    pub dx: f64,
    pub dy: f64,
    /// The farthest it got from where it started.
    pub reach: f64,
    /// All its movement, whatever the direction.
    pub path: f64,
    /// Movement against the stroke's own direction: the hand settling, or the sensor tracking
    /// for a moment as the mouse rises or lands.
    pub back: f64,
    /// The fastest it went, in counts a second.
    pub peak: f64,
    /// Reports pointing back while going over 1 m/s: the sensor losing track.
    pub flips: u32,
    /// Gaps of 2 ms or more while moving fast enough to fill every poll.
    pub dropouts: u32,
    /// The longest of them, in microseconds.
    pub longest_dropout_us: u32,
    /// Stops mid-stroke after which it carried on the same way at much the same speed: the sensor
    /// losing the surface for a moment.
    pub stalls: u32,
    pub end: StrokeEnd,
}

/// Splits movement reports into strokes.
pub struct Strokes {
    dpi: f64,
    period: f64,
    /// The stroke in progress: each report's time and movement.
    cur: Vec<(f64, f64, f64)>,
    net: (f64, f64),
    far: f64,
    far_at: usize,
    done: Vec<Stroke>,
}

impl Strokes {
    pub fn new(dpi: u32, hz: u32) -> Self {
        Self {
            dpi: dpi.max(1) as f64,
            period: 1.0 / hz.max(1) as f64,
            cur: Vec::new(),
            net: (0.0, 0.0),
            far: 0.0,
            far_at: 0,
            done: Vec::new(),
        }
    }

    /// The DPI in use, to turn distances and speeds into counts, and the polling rate.
    pub fn set(&mut self, dpi: u32, hz: u32) {
        self.dpi = dpi.max(1) as f64;
        self.period = 1.0 / hz.max(1) as f64;
    }

    fn counts(&self, mm: f64) -> f64 {
        mm * self.dpi / 25.4
    }

    /// One movement report, read at `t` seconds.
    pub fn push(&mut self, t: f64, dx: f64, dy: f64) {
        if self.cur.last().is_some_and(|&(last, ..)| t - last >= STROKE_PAUSE) {
            self.finish(StrokeEnd::Pause);
        }
        let mut queue = vec![(t, dx, dy)];
        while let Some(r) = queue.pop() {
            self.cur.push(r);
            self.net.0 += r.1;
            self.net.1 += r.2;
            let dist = self.net.0.hypot(self.net.1);
            if dist > self.far {
                self.far = dist;
                self.far_at = self.cur.len() - 1;
            }
            if self.far - dist > self.counts(TURN_MM) {
                // Split at the farthest point: the way back starts the next stroke.
                let rest = self.cur.split_off(self.far_at + 1);
                self.finish(StrokeEnd::Turn);
                queue.extend(rest.into_iter().rev());
            } else if self.cur.len() >= STROKE_MAX {
                self.finish(StrokeEnd::Cut);
            }
        }
    }

    /// Nothing has come since `now`: a stroke still open once the mouse has been still long
    /// enough has ended.
    pub fn idle(&mut self, now: f64) {
        if self.cur.last().is_some_and(|&(last, ..)| now - last >= STROKE_PAUSE) {
            self.finish(StrokeEnd::Pause);
        }
    }

    /// Strokes ended since the last call.
    pub fn take(&mut self) -> Vec<Stroke> {
        std::mem::take(&mut self.done)
    }

    /// Forget the stroke in progress, keeping those that ended: the app went behind another window,
    /// where report times no longer mean anything.
    pub fn drop_current(&mut self) {
        self.cur.clear();
        self.net = (0.0, 0.0);
        self.far = 0.0;
        self.far_at = 0;
    }

    /// Forget the stroke in progress and any not taken yet.
    pub fn clear(&mut self) {
        self.cur.clear();
        self.net = (0.0, 0.0);
        self.far = 0.0;
        self.far_at = 0;
        self.done.clear();
    }

    fn finish(&mut self, end: StrokeEnd) {
        let reports = std::mem::take(&mut self.cur);
        self.net = (0.0, 0.0);
        self.far = 0.0;
        self.far_at = 0;
        if !reports.is_empty() {
            let stroke = self.measure(&reports, end);
            self.done.push(stroke);
        }
    }

    fn measure(&self, r: &[(f64, f64, f64)], end: StrokeEnd) -> Stroke {
        let n = r.len();
        let hyp: Vec<f64> = r.iter().map(|&(_, x, y)| x.hypot(y)).collect();
        // Where it was after each report, and its farthest point.
        let (mut x, mut y, mut far, mut far_at) = (0.0, 0.0, 0.0, 0);
        let mut dist = Vec::with_capacity(n);
        for (k, &(_, dx, dy)) in r.iter().enumerate() {
            x += dx;
            y += dy;
            let d = f64::hypot(x, y);
            dist.push(d);
            if d > far {
                far = d;
                far_at = k;
            }
        }
        // The stroke's direction: toward its farthest point.
        let (fx, fy) = r[..=far_at].iter().fold((0.0, 0.0), |(a, b), &(_, dx, dy)| (a + dx, b + dy));
        let dir = if far > 0.0 { (fx / far, fy / far) } else { (1.0, 0.0) };
        let back: f64 = r.iter().map(|&(_, dx, dy)| (-(dx * dir.0 + dy * dir.1)).max(0.0)).sum();

        // Speed over the window ending at each report, and the movement in it.
        let mut speed = vec![0.0; n];
        let mut heading = vec![(0.0, 0.0); n];
        let (mut j, mut sum, mut hx, mut hy) = (0, 0.0, 0.0, 0.0);
        for k in 0..n {
            sum += hyp[k];
            hx += r[k].1;
            hy += r[k].2;
            while r[j].0 <= r[k].0 - SPEED_S {
                sum -= hyp[j];
                hx -= r[j].1;
                hy -= r[j].2;
                j += 1;
            }
            speed[k] = sum / SPEED_S;
            heading[k] = (hx, hy);
        }
        let peak = speed.iter().copied().fold(0.0, f64::max);

        // Pointing back at speed, against the way it was heading just before, while still on the
        // way out and short of the far end.
        let fast = FLIP_MPS * 1000.0 / 25.4 * self.dpi;
        let near_end = far - self.counts(FLIP_END_MM);
        let mut flips = 0;
        for k in 1..far_at {
            let (hx, hy) = heading[k - 1];
            let (dx, dy) = (r[k].1, r[k].2);
            if speed[k - 1] > fast && dist[k] < near_end && dx * hx + dy * hy < 0.0 {
                flips += 1;
            }
        }

        // Movement in [from, to) seconds, and its total length.
        let span = |from: f64, to: f64| {
            let (mut sx, mut sy, mut len) = (0.0, 0.0, 0.0);
            for k in r.partition_point(|q| q.0 < from)..r.partition_point(|q| q.0 < to) {
                sx += r[k].1;
                sy += r[k].2;
                len += hyp[k];
            }
            (sx, sy, len)
        };
        // Reports read in a burst right after a gap: the app was busy, not the mouse.
        let burst_after =
            |k: usize| (k + 1..(k + 4).min(n)).filter(|&q| r[q].0 - r[q - 1].0 < 0.3 * self.period).count() >= 2;

        let need = FILL_COUNTS / self.period;
        let slow_min = STALL_MIN_CMS * 10.0 / 25.4 * self.dpi;
        let (mut dropouts, mut longest, mut stalls) = (0, 0.0_f64, 0);
        for k in 1..n {
            let gap = r[k].0 - r[k - 1].0;
            if gap < DROPOUT_S.min(STALL_S) || burst_after(k) {
                continue;
            }
            let (_, _, ahead) = span(r[k].0, r[k].0 + SPEED_S);
            if gap >= DROPOUT_S && speed[k - 1] >= need && ahead / SPEED_S >= need {
                dropouts += 1;
                longest = longest.max(gap);
                continue;
            }
            let (bx, by, blen) = span(r[k - 1].0 - STALL_SIDE_S + 1e-9, r[k - 1].0 + 1e-9);
            let (ax, ay, alen) = span(r[k].0, r[k].0 + STALL_SIDE_S);
            let (vb, va) = (blen / STALL_SIDE_S, alen / STALL_SIDE_S);
            if vb < slow_min || gap < STALL_S.max(STALL_REPORTS / vb) {
                continue;
            }
            let same_speed = (0.4..=2.5).contains(&(va / vb));
            let same_way = bx * ax + by * ay > 0.7 * f64::hypot(bx, by) * f64::hypot(ax, ay);
            if same_speed && same_way {
                stalls += 1;
            }
        }

        Stroke {
            start_us: (r[0].0 * 1e6) as u64,
            end_us: (r[n - 1].0 * 1e6) as u64,
            dx: x,
            dy: y,
            reach: far,
            path: hyp.iter().sum(),
            back,
            peak,
            flips,
            dropouts,
            longest_dropout_us: (longest * 1e6).round() as u32,
            stalls,
            end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny deterministic generator, so the tests don't depend on a crate.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    const P8K: f64 = 1.0 / 8000.0;

    /// `secs` of fast movement at 8,000 Hz, 6 counts a poll. `skip(poll)` says how many polls
    /// from this one come without a report; `late(poll)` how long the app takes to read it.
    /// Returns the reports as the app reads them: (time read, counts).
    fn stream(
        secs: f64,
        period: f64,
        mut skip: impl FnMut(u64) -> u64,
        mut late: impl FnMut(u64) -> f64,
    ) -> Vec<(f64, f64)> {
        let polls = (secs / period) as u64;
        let mut out = Vec::new();
        let mut poll = 0;
        let mut owed = 0.0;
        let mut ready = 0.0_f64;
        while poll < polls {
            let s = skip(poll);
            owed += 6.0 * s as f64;
            poll += s;
            if poll >= polls {
                break;
            }
            // Read in order: a report can't be read before the one ahead of it.
            ready = ready.max(poll as f64 * period + late(poll));
            out.push((ready, 6.0 + owed));
            owed = 0.0;
            poll += 1;
        }
        out
    }

    fn run(reports: &[(f64, f64)], hz: u32) -> ConnStats {
        let mut c = Conn::new(hz);
        for &(t, n) in reports {
            c.push(t, n);
        }
        c.idle(reports.last().unwrap().0 + 1.0);
        c.snapshot().stats()
    }

    /// Intervals between reads of at least `polls` polls: what counting gaps naively would find.
    fn long_intervals(s: &[(f64, f64)], polls: f64) -> usize {
        s.windows(2).filter(|w| w[1].0 - w[0].0 >= polls * P8K).count()
    }

    #[test]
    fn a_new_rate_starts_a_new_run() {
        // A second at 8,000 Hz with a gap every 100 polls, then the mouse goes to 4,000 Hz: reports
        // still read as 8,000 Hz ones until the meter hears of it.
        let fast = stream(1.0, P8K, |poll| u64::from(poll % 100 == 99), |_| 0.0);
        let slow: Vec<(f64, f64)> =
            stream(1.0, 2.0 * P8K, |_| 0, |_| 0.0).into_iter().map(|(t, n)| (t + 1.0, n)).collect();
        let mut c = Conn::new(8000);
        for &(t, n) in fast.iter().chain(&slow[..400]) {
            c.push(t, n);
        }
        c.set_rate(4000);
        for &(t, n) in &slow[400..] {
            c.push(t, n);
        }
        c.idle(3.0);
        let st = c.snapshot().stats();
        assert!((800.0..=905.0).contains(&st.ms), "{st:?}");
        assert_eq!((st.missed, st.stutters), (0, 0), "{st:?}");
    }

    #[test]
    fn the_apps_own_delays_are_not_gaps() {
        // Reads up to 200 µs late, and now and then the app stalls for 1 to 4 ms and catches up.
        let mut rng = Rng(7);
        let mut stall_until = 0.0;
        let s = stream(
            5.0,
            P8K,
            |_| 0,
            |poll| {
                let t = poll as f64 * P8K;
                if t > stall_until && rng.next() < 0.0005 {
                    stall_until = t + 0.001 + rng.next() * 0.003;
                }
                if t < stall_until { stall_until - t } else { rng.next() * 200e-6 }
            },
        );
        // Read naively, the stream is full of gaps and stutters.
        assert!(
            long_intervals(&s, 1.5) > 1000 && long_intervals(&s, 8.0) > 5,
            "{} {}",
            long_intervals(&s, 1.5),
            long_intervals(&s, 8.0)
        );
        let st = run(&s, 8000);
        assert!(st.ms > 4900.0, "{st:?}");
        assert_eq!(st.missed, 0, "{st:?}");
        assert_eq!(st.stutters, 0);
    }

    #[test]
    fn fewer_reports_than_polls() {
        // The radio carries about 3,500 of 8,000 a second, the rest merged into the next: a gap of
        // one poll or two every other report or so.
        let (mut rng, mut jitter) = (Rng(9), Rng(10));
        let s = stream(
            5.0,
            P8K,
            |_| match rng.next() {
                r if r < 0.1 => 0,
                r if r < 0.7 => 1,
                _ => 2,
            },
            |_| jitter.next() * 40e-6,
        );
        let st = run(&s, 8000);
        let per_s = st.reports as f64 / (st.ms / 1000.0);
        assert!((per_s - 3500.0).abs() < 300.0, "{per_s} a second, {st:?}");
        assert_eq!(st.gaps[2..], [0, 0, 0, 0]);
        assert_eq!(st.pattern, Pattern::Regular);
    }

    #[test]
    fn a_slow_clock_is_not_a_gap() {
        // The USB clock runs 400 ppm slow against this PC's.
        let s = stream(5.0, P8K * 1.0004, |_| 0, |_| 20e-6);
        assert_eq!(run(&s, 8000).missed, 0);
    }

    #[test]
    fn a_steady_rhythm_of_gaps() {
        // Every 40th poll comes without a report, with the app reading up to 50 µs late.
        let mut rng = Rng(3);
        let s = stream(5.0, P8K, |poll| u64::from(poll % 40 == 39), |_| rng.next() * 50e-6);
        let st = run(&s, 8000);
        let gaps = st.gaps[0];
        assert!((990..=1000).contains(&gaps), "{st:?}");
        assert_eq!(st.missed, gaps);
        assert_eq!(st.pattern, Pattern::Regular);
        let every = st.every_ms.unwrap();
        assert!((every - 5.0).abs() < 0.2, "every {every}");
        // 1 missed in 40.
        let fill = st.reports as f64 / (st.reports + st.missed) as f64;
        assert!((fill - 0.975).abs() < 0.002, "fill {fill}");
    }

    #[test]
    fn gaps_in_bursts() {
        // Clean, except for 30 ms of interference every half second, missing 1 to 3 polls in 5.
        let mut rng = Rng(11);
        let s = stream(
            5.0,
            P8K,
            |poll| {
                let t = poll as f64 * P8K;
                if t % 0.5 < 0.03 && rng.next() < 0.2 { 1 + (rng.next() * 3.0) as u64 } else { 0 }
            },
            |_| 20e-6,
        );
        let st = run(&s, 8000);
        assert!(st.missed > 100, "{st:?}");
        assert_eq!(st.pattern, Pattern::Bursts);
    }

    #[test]
    fn gaps_at_random() {
        let mut rng = Rng(5);
        let s = stream(5.0, P8K, |_| u64::from(rng.next() < 0.004), |_| 20e-6);
        let st = run(&s, 8000);
        assert_eq!(st.pattern, Pattern::Scattered, "{st:?}");
    }

    #[test]
    fn long_gaps_have_their_own_pattern() {
        // The radio merges every other report or so, steadily, and interference comes for 30 ms
        // every half second with gaps of 5 to 20 polls.
        let (mut rng, mut jitter) = (Rng(21), Rng(22));
        let s = stream(
            5.0,
            P8K,
            |poll| {
                let t = poll as f64 * P8K;
                if t % 0.5 < 0.03 && rng.next() < 0.05 {
                    5 + (rng.next() * 15.0) as u64
                } else {
                    u64::from(rng.next() < 0.5)
                }
            },
            |_| jitter.next() * 40e-6,
        );
        let st = run(&s, 8000);
        assert_eq!(st.pattern, Pattern::Regular, "{st:?}");
        assert_eq!(st.long_pattern, Pattern::Bursts, "{st:?}");
    }

    #[test]
    fn a_dropout_is_a_stutter() {
        // 24 polls (3 ms) without a report, once.
        let s = stream(2.0, P8K, |poll| if poll == 8000 { 24 } else { 0 }, |_| 20e-6);
        let st = run(&s, 8000);
        assert_eq!(st.missed, 24, "{st:?}");
        assert_eq!(st.gaps[5], 1);
        assert_eq!(st.stutters, 1);
        assert_eq!(st.longest_us, 3125);
    }

    #[test]
    fn slow_movement_does_not_count() {
        // One count every third poll: polls come up empty because there's nothing to report.
        let s: Vec<(f64, f64)> = (0..10_000).map(|i| (i as f64 * 3.0 * P8K, 1.0)).collect();
        let st = run(&s, 8000);
        assert_eq!(st.ms, 0.0);
        assert_eq!(st.missed, 0);
    }

    // ---- Strokes

    /// Reports at 8,000 Hz from `t0`, following `speed(t)` (cm/s, signed, along x) for `secs`, at
    /// 1,600 DPI. Fractions of a count carry over, as on a sensor.
    fn slide(out: &mut Vec<(f64, f64, f64)>, t0: f64, secs: f64, speed: impl Fn(f64) -> f64) -> f64 {
        let mut owed = 0.0;
        let polls = (secs / P8K) as u64;
        for i in 0..polls {
            let t = i as f64 * P8K;
            owed += speed(t) * P8K / 2.54 * 1600.0;
            let whole = owed.trunc();
            if whole != 0.0 {
                out.push((t0 + t, whole, 0.0));
                owed -= whole;
            }
        }
        t0 + polls as f64 * P8K
    }

    fn strokes(reports: &[(f64, f64, f64)]) -> Vec<Stroke> {
        let mut s = Strokes::new(1600, 8000);
        for &(t, dx, dy) in reports {
            s.push(t, dx, dy);
        }
        s.idle(reports.last().unwrap().0 + 1.0);
        s.take()
    }

    /// A smooth swipe of `cm` taking `secs`, peaking at twice its average speed.
    fn swipe(cm: f64, secs: f64) -> impl Fn(f64) -> f64 {
        move |t| cm / secs * (1.0 - (2.0 * std::f64::consts::PI * t / secs).cos())
    }

    #[test]
    fn back_and_forth_between_two_stops() {
        // 10 cm one way and back, faster each time.
        let mut r = Vec::new();
        let mut t = 0.0;
        for (i, secs) in [0.8, 0.6, 0.4, 0.25, 0.15].into_iter().enumerate() {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let f = swipe(10.0, secs);
            t = slide(&mut r, t, secs, move |x| sign * f(x));
        }
        let s = strokes(&r);
        assert_eq!(s.len(), 5, "{s:?}");
        let cm = |x: f64| x / 1600.0 * 2.54;
        for st in &s {
            assert!((cm(st.reach) - 10.0).abs() < 0.05, "reach {}", cm(st.reach));
            assert_eq!((st.flips, st.dropouts, st.stalls), (0, 0, 0));
        }
        assert!(s[..4].iter().all(|x| x.end == StrokeEnd::Turn));
        assert_eq!(s[4].end, StrokeEnd::Pause);
        // Faster each time: the last peaks near 2 × 10 cm / 0.15 s.
        assert!(s.windows(2).all(|w| w[1].peak > w[0].peak));
        let mps = s[4].peak / 1600.0 * 0.0254;
        assert!((mps - 1.33).abs() < 0.1, "peak {mps} m/s");
    }

    #[test]
    fn spinning_out_mid_swipe() {
        // 10 cm in 0.1 s (up to 2 m/s), with reports pointing back for 2 ms in the middle.
        let mut r = Vec::new();
        let f = swipe(10.0, 0.1);
        slide(&mut r, 0.0, 0.1, |t| if (0.049..0.051).contains(&t) { -0.5 * f(t) } else { f(t) });
        let s = strokes(&r);
        assert_eq!(s.len(), 1, "{s:?}");
        assert!(s[0].flips >= 5, "{:?}", s[0]);
    }

    #[test]
    fn a_dropout_at_speed() {
        // 3 ms without reports at 1 m/s, the movement arriving in the next report.
        let mut all = Vec::new();
        slide(&mut all, 0.0, 0.2, |_| 100.0);
        let (mut r, mut owed) = (Vec::new(), 0.0);
        for (t, dx, dy) in all {
            if (0.1..0.103).contains(&t) {
                owed += dx;
            } else {
                r.push((t, dx + owed, dy));
                owed = 0.0;
            }
        }
        let s = strokes(&r);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].dropouts, 1, "{:?}", s[0]);
        assert!((s[0].longest_dropout_us as f64 - 3000.0).abs() < 200.0);
    }

    #[test]
    fn a_stall_on_a_slow_slide() {
        // 2 cm/s, losing the surface for 40 ms halfway, then carrying on.
        let mut r = Vec::new();
        slide(&mut r, 0.0, 1.0, |t| if (0.5..0.54).contains(&t) { 0.0 } else { 2.0 });
        let s = strokes(&r);
        assert_eq!(s.len(), 1, "{s:?}");
        assert_eq!(s[0].stalls, 1, "{:?}", s[0]);
        // A hand that stops for longer, or speeds off afterwards, isn't a stall.
        let mut r = Vec::new();
        slide(&mut r, 0.0, 1.0, |t| {
            if (0.5..0.54).contains(&t) {
                0.0
            } else if t > 0.54 {
                9.0
            } else {
                2.0
            }
        });
        assert_eq!(strokes(&r)[0].stalls, 0);
    }

    #[test]
    fn lift_and_carry_back() {
        // A 6 cm slide, the sensor tracking 0.8 mm back as the mouse rises, 400 ms in the air,
        // 0.5 mm back as it lands, then the next slide.
        let mut r = Vec::new();
        let mut t = slide(&mut r, 0.0, 0.5, swipe(6.0, 0.5));
        t = slide(&mut r, t, 0.02, |_| -4.0);
        t = slide(&mut r, t + 0.4, 0.02, |_| -2.5);
        slide(&mut r, t, 0.5, swipe(6.0, 0.5));
        let s = strokes(&r);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(s[0].end, StrokeEnd::Pause);
        let mm = |x: f64| x / 1600.0 * 25.4;
        assert!((mm(s[0].back) - 0.8).abs() < 0.1, "back {}", mm(s[0].back));
        assert!((mm(s[1].back) - 0.5).abs() < 0.1, "back {}", mm(s[1].back));
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    /// What 10 s of fast movement at 8,000 Hz costs: run with `--ignored --nocapture`, in the
    /// build you care about.
    #[test]
    #[ignore]
    fn ten_seconds_at_8k() {
        let (mut c, mut s) = (Conn::new(8000), Strokes::new(1600, 8000));
        let t0 = Instant::now();
        for i in 0..80_000u32 {
            let t = i as f64 / 8000.0;
            let a = t * 6.0;
            let (dx, dy) = ((a.cos() * 12.0).round(), (a.sin() * 12.0).round());
            c.push(t, dx.abs() + dy.abs());
            s.push(t, dx, dy);
        }
        let took = t0.elapsed();
        println!(
            "{:.2} µs a report, {:.1}% of a core at 8,000 Hz",
            took.as_secs_f64() * 1e6 / 80_000.0,
            took.as_secs_f64() / 10.0 * 100.0
        );
        let _ = (c.snapshot().stats(), s.take());
    }
}
