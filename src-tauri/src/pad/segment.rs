//! Splits the report stream into swipes and describes how each one ended.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A gap this long between movement reports ends a swipe: the mouse stopped, or left the pad.
pub(super) const PAUSE_MIN: Duration = Duration::from_millis(40);
/// How quickly the speed estimate follows the reports (seconds).
const SPEED_TAU: f64 = 0.008;
/// Movement this close to a pause counts as a blip around it.
pub(super) const BLIP_WINDOW: Duration = Duration::from_millis(30);
/// The end speed is the average over this much time before reports stopped. Long enough to ride
/// over reports Windows delivers in a burst, short enough that a hand braking hard reads as slow.
const END_WINDOW: Duration = Duration::from_millis(6);
/// Movement against the swipe beyond this (cm) is the hand moving back, not the sensor tracking in
/// the air.
pub(super) const BLIP_MAX: f64 = 0.4;
/// Below this share of its path in one direction, a swipe went back and forth.
const STRAIGHT_MIN: f64 = 0.8;

/// How a swipe ended, measured when the next report shows there was a pause.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pause {
    pub gap: Duration,
    /// Unit direction of the swipe's net movement, or zero for a swipe too short to say.
    pub dir: (f64, f64),
    /// Net movement (cm).
    pub length: f64,
    /// Speed when reports stopped, as a share of the swipe's peak. Near 1: cut off while moving.
    /// A flick braking hard reads low: it slows in a few milliseconds, but it does slow.
    pub cut: f64,
    /// Speed when reports stopped (cm/s).
    pub end_speed: f64,
    /// Movement against the swipe's direction in its last moments (cm): the sensor still tracking
    /// as the mouse rises. Zero when that movement was too big to be a blip, or the swipe went
    /// back and forth.
    pub reverse: f64,
}

#[derive(Default)]
pub(super) struct Segmenter {
    last: Option<Instant>,
    speed: f64,
    peak: f64,
    moved: (f64, f64),
    path: f64,
    recent: VecDeque<(Instant, f64, f64)>,
}

impl Segmenter {
    pub fn last(&self) -> Option<Instant> {
        self.last
    }

    /// Feed one report, already in pad centimetres. Returns the pause it ends, if it follows one.
    pub fn push(&mut self, now: Instant, mx: f64, my: f64) -> Option<Pause> {
        let mut ended = None;
        match self.last {
            Some(prev) if now - prev < PAUSE_MIN => {
                let dt = (now - prev).as_secs_f64().max(1e-6);
                let a = 1.0 - (-dt / SPEED_TAU).exp();
                self.speed += a * (mx.hypot(my) / dt - self.speed);
                self.peak = self.peak.max(self.speed);
            }
            Some(prev) => {
                ended = Some(self.end(now - prev));
                *self = Self::default();
            }
            None => {}
        }
        self.moved.0 += mx;
        self.moved.1 += my;
        self.path += mx.hypot(my);
        self.recent.push_back((now, mx, my));
        while self.recent.front().is_some_and(|(t, _, _)| now - *t > BLIP_WINDOW) {
            self.recent.pop_front();
        }
        self.last = Some(now);
        ended
    }

    /// Close the swipe in progress as if a pause of `gap` followed it.
    pub fn end(&self, gap: Duration) -> Pause {
        let length = self.moved.0.hypot(self.moved.1);
        let dir = if length > 0.05 { (self.moved.0 / length, self.moved.1 / length) } else { (0.0, 0.0) };
        let back: f64 = self.recent.iter().map(|(_, x, y)| (-(x * dir.0 + y * dir.1)).max(0.0)).sum();
        let straight = length / self.path.max(1e-9);
        let reverse = if back <= BLIP_MAX && straight >= STRAIGHT_MIN { back } else { 0.0 };
        let end_speed = self.end_speed();
        let cut = if self.peak > 0.0 { (end_speed / self.peak).min(1.5) } else { 0.0 };
        Pause { gap, dir, length, cut, end_speed, reverse }
    }

    /// Average speed over the last `END_WINDOW` of reports, measured from the latest report at
    /// least that old so each report's movement is spread over the time it took.
    fn end_speed(&self) -> f64 {
        let Some(&(last, ..)) = self.recent.back() else { return 0.0 };
        let mut dist = 0.0;
        for (i, &(_, x, y)) in self.recent.iter().enumerate().rev() {
            dist += x.hypot(y);
            match i.checked_sub(1).map(|j| self.recent[j].0) {
                Some(t) if last - t >= END_WINDOW => return dist / (last - t).as_secs_f64(),
                Some(_) => {}
                None => break,
            }
        }
        let first = self.recent.front().map_or(last, |r| r.0);
        dist / (last - first).max(END_WINDOW).as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reports every 125 µs, `speed(ms)` in cm/s, for `ms` milliseconds.
    fn feed(s: &mut Segmenter, t0: Instant, ms: f64, speed: impl Fn(f64) -> f64) -> Instant {
        let steps = (ms * 8.0) as u64;
        for i in 1..=steps {
            let t = i as f64 / 8.0;
            s.push(t0 + Duration::from_micros(i * 125), speed(t) * 125e-6, 0.0);
        }
        t0 + Duration::from_micros(steps * 125)
    }

    #[test]
    fn a_hard_stop_reads_slow_and_a_cut_off_reads_fast() {
        // 200 cm/s, then braking to nothing in 12 ms.
        let mut s = Segmenter::default();
        let end = feed(&mut s, Instant::now(), 52.0, |t| if t < 40.0 { 200.0 } else { 200.0 * (52.0 - t) / 12.0 });
        let stop = s.push(end + Duration::from_millis(300), 0.01, 0.0).unwrap();
        assert!(stop.cut < 0.35, "hard stop {}", stop.cut);

        let mut s = Segmenter::default();
        let end = feed(&mut s, Instant::now(), 40.0, |_| 200.0);
        let lift = s.push(end + Duration::from_millis(300), 0.01, 0.0).unwrap();
        assert!(lift.cut > 0.8, "cut off {}", lift.cut);
    }

    #[test]
    fn a_swipe_back_is_not_a_blip() {
        // 6 cm right, then straight back 5 cm with no pause between.
        let mut s = Segmenter::default();
        let end = feed(&mut s, Instant::now(), 60.0, |_| 100.0);
        let end = feed(&mut s, end, 50.0, |_| -100.0);
        let p = s.push(end + Duration::from_millis(300), 0.01, 0.0).unwrap();
        assert_eq!(p.reverse, 0.0);
    }
}
