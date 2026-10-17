//! Double clicks over days: presses, and presses that came too soon after a release (a switch
//! bouncing), per button per day, so a wearing switch shows before it gets bad.
//!
//! Counted from raw input in the background too. Windows hands background listeners merged reports
//! about 122 times a second, but keeps every press and release, in order (the user's probe,
//! 2026-10-17: 38 presses and 39 releases alternating, none merged), so a bounce of 8 ms or more
//! is seen with its gap read to about 8 ms. One shorter can come merged into one report, which
//! counts too; one Windows drops entirely can't. With hrtz in front, every report comes.

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A press this soon after a release of the same button is the switch bouncing, not a finger:
/// the fastest double click the user made was 76 ms from release to press.
pub const DOUBLE_MS: f64 = 30.0;
/// Days kept.
const KEEP_DAYS: usize = 30;
/// RI_MOUSE_* down and up flags for left, right, middle, back, forward.
const FLAGS: [(u16, u16); 5] =
    [(0x0001, 0x0002), (0x0004, 0x0008), (0x0010, 0x0020), (0x0040, 0x0080), (0x0100, 0x0200)];
/// Any of them: reports without these carry nothing for the log.
pub const BUTTON_BITS: u16 = 0x03FF;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ButtonDay {
    pub presses: u32,
    pub doubles: u32,
    /// The longest gap of a double, in milliseconds: what click debounce would have to cover.
    pub worst_ms: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    /// Local date, `2026-10-17`.
    pub date: String,
    /// Left, right, middle, back, forward.
    pub buttons: [ButtonDay; 5],
}

#[derive(Default)]
pub struct ClickLog {
    days: VecDeque<Day>,
    last_up: [Option<f64>; 5],
    down: [bool; 5],
    pub dirty: bool,
}

impl ClickLog {
    pub fn load(path: &Path) -> Self {
        let days: Vec<Day> =
            std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { days: days.into(), ..Self::default() }
    }

    pub fn save(&mut self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string(&self.days).unwrap_or_else(|_| "[]".into());
        std::fs::write(path, json)?;
        self.dirty = false;
        Ok(())
    }

    pub fn days(&self) -> Vec<Day> {
        self.days.iter().cloned().collect()
    }

    /// One report's button flags, read at `t` seconds, on the local `date`.
    pub fn report(&mut self, date: &str, t: f64, flags: u16) {
        for (i, (down, up)) in FLAGS.into_iter().enumerate() {
            match (flags & down != 0, flags & up != 0) {
                // Both in one merged report. Held before: it let go and pressed again at once, a
                // bounce. Not held: a press and a release too quick to tell apart.
                (true, true) if self.down[i] => {
                    self.release(i, t);
                    self.press(i, t, date);
                }
                (true, true) => {
                    self.press(i, t, date);
                    self.release(i, t);
                }
                (true, false) => self.press(i, t, date),
                (false, true) => self.release(i, t),
                (false, false) => {}
            }
        }
    }

    fn press(&mut self, i: usize, t: f64, date: &str) {
        let gap = self.last_up[i].map(|up| (t - up) * 1000.0);
        let day = self.day(date);
        let b = &mut day.buttons[i];
        b.presses += 1;
        if let Some(gap) = gap.filter(|g| *g < DOUBLE_MS) {
            b.doubles += 1;
            b.worst_ms = b.worst_ms.max(gap as f32);
        }
        self.down[i] = true;
        self.dirty = true;
    }

    fn release(&mut self, i: usize, t: f64) {
        self.last_up[i] = Some(t);
        self.down[i] = false;
    }

    fn day(&mut self, date: &str) -> &mut Day {
        if self.days.back().is_none_or(|d| d.date != date) {
            self.days.push_back(Day { date: date.to_string(), ..Day::default() });
            while self.days.len() > KEEP_DAYS {
                self.days.pop_front();
            }
        }
        self.days.back_mut().expect("just pushed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOWN: u16 = 0x0001;
    const UP: u16 = 0x0002;

    #[test]
    fn a_bounce_is_a_double_and_a_quick_double_click_is_not() {
        let mut log = ClickLog::default();
        // A click, a real double click (76 ms from release to press), then a bounce at 9 ms.
        for (t, f) in [(0.0, DOWN), (0.07, UP), (0.146, DOWN), (0.21, UP), (0.219, DOWN), (0.29, UP)] {
            log.report("2026-10-17", t, f);
        }
        let b = log.days()[0].buttons[0];
        assert_eq!((b.presses, b.doubles), (3, 1));
        assert!((b.worst_ms - 9.0).abs() < 0.01, "{b:?}");
    }

    #[test]
    fn a_merged_release_and_press_while_held_is_a_bounce() {
        let mut log = ClickLog::default();
        log.report("2026-10-17", 0.0, DOWN);
        log.report("2026-10-17", 0.05, DOWN | UP);
        log.report("2026-10-17", 0.12, UP);
        let b = log.days()[0].buttons[0];
        assert_eq!((b.presses, b.doubles), (2, 1));
        // Not held: a very short click, not a double.
        log.report("2026-10-17", 1.0, DOWN | UP);
        assert_eq!(log.days()[0].buttons[0].doubles, 1);
    }

    #[test]
    fn days_roll_over_and_the_oldest_go() {
        let mut log = ClickLog::default();
        for d in 0..40 {
            log.report(&format!("day {d:02}"), d as f64, DOWN);
            log.report(&format!("day {d:02}"), d as f64 + 0.1, UP);
        }
        let days = log.days();
        assert_eq!(days.len(), KEEP_DAYS);
        assert_eq!(days[0].date, "day 10");
        assert!(days.iter().all(|d| d.buttons[0].presses == 1));
    }
}
