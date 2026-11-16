//! The mouse's days, for the week in review: how far it travelled, how long it moved, its fastest
//! moment, and in each game hrtz knows, the same plus how many full turns (where the game's
//! sensitivity is read). Totals per day and per game only, kept on this PC: no positions, no times
//! of day, no other app named.
//!
//! Counted from raw input in the background too. Windows hands a background listener merged
//! reports about 8 ms apart: every count arrives, so distances are whole, and speeds are measured
//! over 50 ms, which merged reports fill.

use std::collections::VecDeque;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Days kept: a week, the four before it to compare with, and room.
const KEEP_DAYS: usize = 60;
/// A speed is measured over at least this long (s).
const WINDOW_S: f64 = 0.05;
/// A window this long has a pause in it, not a speed (s).
const STALE_S: f64 = 0.5;
/// Reports this close together are one movement, for the time spent moving (s).
const MOVING_GAP_S: f64 = 0.25;
/// Faster than this (m/s) is a misread count or a wrong DPI, not a hand: sensors are rated to
/// about 19.
const FASTEST_MPS: f64 = 25.0;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GameDay {
    /// The game's exe, in lower case.
    pub exe: String,
    pub cm: f64,
    pub moving_s: f64,
    pub top_mps: f64,
    /// Full turns, counted only while the game's sensitivity is known.
    pub turns: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Day {
    /// Local date, `2026-11-16`.
    pub date: String,
    pub cm: f64,
    pub moving_s: f64,
    pub top_mps: f64,
    pub games: Vec<GameDay>,
}

/// The game in front, as the log counts it.
#[derive(Clone, Debug, PartialEq)]
pub struct InGame {
    /// Its exe, in lower case.
    pub exe: String,
    /// Degrees turned per count: its yaw times its sensitivity, where its settings file says.
    pub deg_per_count: Option<f64>,
}

#[derive(Default)]
pub struct Activity {
    days: VecDeque<Day>,
    game: Option<InGame>,
    /// Where the speed being measured started, and the counts since.
    window: Option<(f64, f64)>,
    last: Option<f64>,
    /// Today's date, and when it was last asked for.
    date: (String, f64),
    pub dirty: bool,
}

impl Activity {
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

    /// The days counted, oldest first.
    pub fn days(&self) -> Vec<Day> {
        self.days.iter().cloned().collect()
    }

    /// The game in front now, or none.
    pub fn set_game(&mut self, game: Option<InGame>) {
        self.game = game;
    }

    /// A report at `t` seconds, from a mouse at `dpi`. Today's date is asked for once a second.
    pub fn report(&mut self, t: f64, dx: i32, dy: i32, dpi: u32) {
        if self.date.0.is_empty() || t - self.date.1 >= 1.0 || t < self.date.1 {
            self.date = (chrono::Local::now().format("%Y-%m-%d").to_string(), t);
        }
        let date = std::mem::take(&mut self.date.0);
        self.report_on(&date, t, dx, dy, dpi);
        self.date.0 = date;
    }

    /// A report on `date`, at `t` seconds, from a mouse at `dpi`.
    pub fn report_on(&mut self, date: &str, t: f64, dx: i32, dy: i32, dpi: u32) {
        if dpi == 0 || (dx == 0 && dy == 0) {
            return;
        }
        let counts = (dx as f64).hypot(dy as f64);
        let cm = counts / dpi as f64 * 2.54;
        let moving = match self.last {
            Some(l) if t > l && t - l < MOVING_GAP_S => t - l,
            _ => 0.0,
        };
        self.last = Some(t);
        // The speed over the last 50 ms or more, from the counts after the report that opened it.
        let speed = match self.window {
            Some((start, n)) if t - start < STALE_S && t >= start => {
                let n = n + counts;
                if t - start >= WINDOW_S {
                    self.window = Some((t, 0.0));
                    Some(n / dpi as f64 * 0.0254 / (t - start)).filter(|v| *v <= FASTEST_MPS)
                } else {
                    self.window = Some((start, n));
                    None
                }
            }
            _ => {
                self.window = Some((t, 0.0));
                None
            }
        };
        let turns = self.game.as_ref().and_then(|g| g.deg_per_count).map(|d| (dx as f64).abs() * d / 360.0);
        let game = self.game.as_ref().map(|g| g.exe.clone());
        let day = self.day(date);
        day.cm += cm;
        day.moving_s += moving;
        if let Some(v) = speed {
            day.top_mps = day.top_mps.max(v);
        }
        if let Some(exe) = game {
            let g = match day.games.iter().position(|g| g.exe == exe) {
                Some(i) => &mut day.games[i],
                None => {
                    day.games.push(GameDay { exe, ..GameDay::default() });
                    day.games.last_mut().unwrap()
                }
            };
            g.cm += cm;
            g.moving_s += moving;
            g.turns += turns.unwrap_or(0.0);
            if let Some(v) = speed {
                g.top_mps = g.top_mps.max(v);
            }
        }
        self.dirty = true;
    }

    fn day(&mut self, date: &str) -> &mut Day {
        if self.days.back().is_none_or(|d| d.date != date) {
            self.days.push_back(Day { date: date.to_string(), ..Day::default() });
            while self.days.len() > KEEP_DAYS {
                self.days.pop_front();
            }
        }
        self.days.back_mut().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_counts_at_the_dpi_in_use() {
        let mut a = Activity::default();
        // 1,600 counts at 1,600 DPI is an inch: 2.54 cm, however it was split.
        for i in 0..16 {
            a.report_on("2026-11-16", i as f64 * 0.001, 60, 80, 1600);
        }
        let d = &a.days()[0];
        assert!((d.cm - 2.54).abs() < 1e-9, "{}", d.cm);
        // Nothing without a DPI, or without movement.
        a.report_on("2026-11-16", 1.0, 100, 0, 0);
        a.report_on("2026-11-16", 1.1, 0, 0, 1600);
        assert!((a.days()[0].cm - 2.54).abs() < 1e-9);
    }

    #[test]
    fn its_fastest_moment_over_50_ms() {
        let mut a = Activity::default();
        // Reports 8 ms apart, merged as a background listener gets them: 400 counts each at 1,600
        // DPI is 0.25 inch per 8 ms, 0.79 m/s.
        for i in 0..20 {
            a.report_on("2026-11-16", i as f64 * 0.008, 400, 0, 1600);
        }
        let top = a.days()[0].top_mps;
        assert!((top - 0.79375).abs() < 0.01, "{top}");
        // A gap is a pause, not a slow speed, and a misread count isn't a speed at all.
        a.report_on("2026-11-16", 5.0, 400, 0, 1600);
        a.report_on("2026-11-16", 5.06, 1_000_000, 0, 1600);
        assert!((a.days()[0].top_mps - top).abs() < 1e-9);
    }

    #[test]
    fn moving_time_is_reports_close_together() {
        let mut a = Activity::default();
        for i in 0..=100 {
            a.report_on("2026-11-16", i as f64 * 0.01, 10, 0, 800);
        }
        // A second of movement, then a reach for the keyboard.
        a.report_on("2026-11-16", 4.0, 10, 0, 800);
        assert!((a.days()[0].moving_s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_game_gets_its_share_and_its_turns() {
        let mut a = Activity::default();
        a.report_on("2026-11-16", 0.0, 800, 0, 800);
        // Valorant at 0.145: 0.07 x 0.145 degrees a count, so 35,468 counts are a full turn.
        a.set_game(Some(InGame { exe: "valorant-win64-shipping.exe".into(), deg_per_count: Some(0.07 * 0.145) }));
        for i in 1..=10 {
            a.report_on("2026-11-16", i as f64, 35_468, 0, 800);
        }
        // Back to the desktop: counted in the day only.
        a.set_game(None);
        a.report_on("2026-11-16", 20.0, 800, 0, 800);
        let d = &a.days()[0];
        assert_eq!(d.games.len(), 1);
        let g = &d.games[0];
        assert!((g.turns - 10.0).abs() < 0.01, "{}", g.turns);
        assert!((g.cm - 35_468.0 * 10.0 / 800.0 * 2.54).abs() < 1e-6);
        assert!((d.cm - g.cm - 2.0 * 2.54).abs() < 1e-6);
        // A game whose sensitivity isn't read: its distance, no turns.
        a.set_game(Some(InGame { exe: "fortniteclient-win64-shipping.exe".into(), deg_per_count: None }));
        a.report_on("2026-11-16", 30.0, 800, 0, 800);
        assert_eq!(a.days()[0].games[1].turns, 0.0);
    }

    #[test]
    fn a_day_each_and_two_months_kept() {
        let mut a = Activity::default();
        for d in 0..70 {
            a.report_on(&format!("day {d:02}"), d as f64 * 100.0, 100, 0, 800);
        }
        let days = a.days();
        assert_eq!(days.len(), KEEP_DAYS);
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("day 10"));
    }

    #[test]
    fn kept_across_restarts() {
        let dir = std::env::temp_dir().join(format!("hrtz-activity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("activity.json");
        let mut a = Activity::default();
        a.report_on("2026-11-16", 0.0, 1600, 0, 1600);
        a.save(&path).unwrap();
        assert!(!a.dirty);
        assert_eq!(Activity::load(&path).days(), a.days());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
