//! Battery level estimate. The mouse's own percentage follows battery voltage, which jumps up as
//! soon as a charger is connected and sags under load, so the raw number swings by tens of percent.
//! This works as the vendor app's estimate does: voltage → level through the model's curve, then
//! rate-limited movement, slower near full while charging, and "100%" only when the mouse says so.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Voltage (mV) at 0, 5, 10 … 100 %: the curve the vendor app uses for all six models.
const CURVE: [u16; 21] = [
    3050, 3420, 3480, 3540, 3600, 3660, 3720, 3760, 3800, 3840, 3880, 3920, 3940, 3960, 3980, 4000, 4020, 4040,
    4060, 4080, 4110,
];
/// The vendor app's pace: 1 % per 10 s, or per 5 min above 85 % while charging.
const STEP: Duration = Duration::from_secs(10);
const STEP_NEAR_FULL: Duration = Duration::from_secs(300);
/// Charging voltage reads high; the estimate only climbs while the reading is this far ahead.
const CHARGE_LEAD: u8 = 10;
/// A difference this large means the old estimate is meaningless (battery swapped, long gap).
const SNAP: u8 = 30;
/// Consecutive "100 %" reports from the mouse before showing full.
const FULL_READS: u8 = 3;

/// Level the vendor curve gives for `mv`, read as the vendor app reads it.
pub fn voltage_level(mv: u16, charging: bool) -> u8 {
    if mv >= CURVE[CURVE.len() - 1] {
        return if charging { 99 } else { 100 };
    }
    let Some(i) = CURVE.iter().position(|&v| mv < v) else { return 100 };
    if i == 0 {
        return 0;
    }
    let span = ((CURVE[i] - CURVE[i - 1]) / 5).max(1);
    ((mv - CURVE[i - 1]) / span + (i as u16 - 1) * 5).min(100) as u8
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Saved {
    pub level: u8,
    /// Unix seconds.
    pub at: u64,
}

#[derive(Debug, Default)]
pub struct Estimator {
    level: Option<u8>,
    last: Option<SystemTime>,
    credit: Duration,
    full_reads: u8,
}

fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Estimator {
    /// Resume from the last session's estimate, as the vendor app does.
    pub fn new(saved: Option<Saved>) -> Self {
        let mut e = Self::default();
        if let Some(s) = saved {
            e.level = Some(s.level);
            e.last = Some(UNIX_EPOCH + Duration::from_secs(s.at));
        }
        e
    }

    pub fn saved(&self) -> Option<Saved> {
        Some(Saved { level: self.level?, at: unix(self.last?) })
    }

    /// Feed one reading; returns the level to show.
    pub fn update(&mut self, raw_level: u8, charging: bool, mv: u16, now: SystemTime) -> u8 {
        let fact = if mv > 0 { voltage_level(mv, charging) } else { raw_level.min(100) };
        let elapsed = self.last.and_then(|l| now.duration_since(l).ok()).unwrap_or_default();
        self.last = Some(now);
        self.full_reads = if charging && raw_level >= 100 { self.full_reads.saturating_add(1) } else { 0 };

        let Some(mut level) = self.level else {
            // No history: a charging voltage reads high, so start the same lead below it.
            let start = if charging { fact.saturating_sub(CHARGE_LEAD) } else { fact };
            self.level = Some(start);
            return start;
        };
        if elapsed > Duration::from_secs(30 * 60) {
            // Too long since the last reading to rate-limit against it.
            self.level = Some(fact);
            self.credit = Duration::ZERO;
            return fact;
        }

        if self.full_reads >= FULL_READS {
            level = 100;
        } else if charging {
            if level < 85 {
                self.credit += elapsed;
                let steps = (self.credit.as_secs() / STEP.as_secs()) as u8;
                self.credit -= STEP * steps as u32;
                if fact > level.saturating_add(CHARGE_LEAD) {
                    level = level.saturating_add(steps).min(fact - CHARGE_LEAD).min(85);
                }
            } else if fact >= 95 {
                self.credit += elapsed;
                let steps = (self.credit.as_secs() / STEP_NEAR_FULL.as_secs()) as u8;
                self.credit -= STEP_NEAR_FULL * steps as u32;
                level = level.saturating_add(steps).min(99);
            }
        } else {
            self.credit += elapsed;
            let steps = (self.credit.as_secs() / STEP.as_secs()) as u8;
            self.credit -= STEP * steps as u32;
            if fact.abs_diff(level) >= SNAP {
                level = fact;
            } else if fact < level {
                level = level.saturating_sub(steps).max(fact);
            }
            if raw_level == 0 {
                level = 0;
            }
        }
        self.level = Some(level);
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000 + s)
    }

    #[test]
    fn curve_matches_vendor() {
        assert_eq!(voltage_level(3970, false), 67);
        assert_eq!(voltage_level(4070, true), 92);
        assert_eq!(voltage_level(4200, true), 99);
        assert_eq!(voltage_level(4200, false), 100);
        assert_eq!(voltage_level(3000, false), 0);
    }

    #[test]
    fn plugging_in_does_not_jump() {
        // Resting at 3.97 V (67 %), then a charger pushes the reading to 4.07 V.
        let mut e = Estimator::default();
        assert_eq!(e.update(65, false, 3970, at(0)), 67);
        assert_eq!(e.update(95, true, 4070, at(60)), 73);
        // Over a few minutes it climbs, but never past the reading minus the charging lead.
        assert_eq!(e.update(95, true, 4070, at(600)), 82);
        assert_eq!(e.update(95, true, 4070, at(1200)), 82);
    }

    #[test]
    fn near_full_climbs_slowly_and_full_needs_the_mouse_to_say_so() {
        let mut e = Estimator::new(Some(Saved { level: 86, at: unix(at(0)) }));
        assert_eq!(e.update(98, true, 4100, at(10)), 86);
        assert_eq!(e.update(98, true, 4100, at(310)), 87);
        e.update(100, true, 4120, at(320));
        e.update(100, true, 4120, at(330));
        assert_eq!(e.update(100, true, 4120, at(340)), 100);
    }

    #[test]
    fn discharge_only_moves_down_gradually() {
        let mut e = Estimator::new(Some(Saved { level: 70, at: unix(at(0)) }));
        // Voltage sags under load: drop at most 1 % per 10 s, never rise while discharging.
        assert_eq!(e.update(60, false, 3900, at(20)), 68);
        assert_eq!(e.update(80, false, 4000, at(40)), 68);
    }

    #[test]
    fn first_reading_on_the_charger_starts_below_the_voltage() {
        let mut e = Estimator::default();
        assert_eq!(e.update(95, true, 4070, at(0)), 82);
    }

    #[test]
    fn long_gap_trusts_the_reading() {
        let mut e = Estimator::new(Some(Saved { level: 90, at: unix(at(0)) }));
        assert_eq!(e.update(60, false, 3880, at(3600)), 50);
    }
}
