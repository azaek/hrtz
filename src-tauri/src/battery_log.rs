//! A log of battery readings, and what it says about battery life and health.
//!
//! The mouse reports its level, voltage and whether it's charging, and nothing about health. So a
//! row is kept whenever one of those, the polling rate or sleep changes (and every so often
//! regardless), and battery life per polling rate, charge cycles and capacity are worked out from
//! the rows.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A row is kept at least this often while the mouse is awake, and while it sleeps.
const EVERY_AWAKE: u64 = 10 * 60;
const EVERY_ASLEEP: u64 = 30 * 60;
/// A longer gap between rows means the app wasn't running, so what happened in it is unknown.
const GAP: u64 = 45 * 60;
const DAY: u64 = 86_400;
/// Battery life at a rate needs at least this much use and this much drain behind it.
const MIN_HOURS: f64 = 1.0;
const MIN_DROP: f64 = 3.0;
/// Capacity compares the first weeks with the last ones, once there's this much history.
const CAPACITY_AFTER: u64 = 30 * DAY;
const WINDOW: u64 = 14 * DAY;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    /// Unix seconds.
    pub t: u64,
    /// The level the app shows (smoothed, see `battery.rs`).
    pub level: u8,
    pub mv: u16,
    pub charging: bool,
    /// Polling rate set at the time.
    pub hz: u32,
    /// Awake and linked to the receiver; false while it sleeps.
    pub awake: bool,
}

#[derive(Default)]
pub struct BatteryLog {
    path: Option<PathBuf>,
    entries: Vec<Entry>,
}

impl BatteryLog {
    pub fn load(path: Option<PathBuf>) -> Self {
        let entries = path
            .as_ref()
            .and_then(|p| fs::read_to_string(p).ok())
            .map(|text| text.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()).collect())
            .unwrap_or_default();
        Self { path, entries }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Keep `e` if it says something new, or the last row is getting old. Returns whether it did.
    pub fn record(&mut self, e: Entry) -> bool {
        if let Some(last) = self.entries.last() {
            let same = last.level == e.level && last.charging == e.charging && last.hz == e.hz && last.awake == e.awake;
            let every = if e.awake { EVERY_AWAKE } else { EVERY_ASLEEP };
            // A clock that went back would scramble the order.
            if e.t < last.t || (same && e.t < last.t + every) {
                return false;
            }
        }
        if let Some(p) = &self.path {
            let line = serde_json::to_string(&e).unwrap_or_default();
            let written = OpenOptions::new().create(true).append(true).open(p).and_then(|mut f| writeln!(f, "{line}"));
            if let Err(err) = written {
                eprintln!("battery log: {err}");
            }
        }
        self.entries.push(e);
        true
    }

    /// Rows from `since` on, thinned to about `max`, always keeping the rows where charging,
    /// sleep or the polling rate changed.
    pub fn history(&self, since: u64, max: usize) -> Vec<Entry> {
        let start = self.entries.partition_point(|e| e.t < since);
        let rows = &self.entries[start..];
        if rows.len() <= max {
            return rows.to_vec();
        }
        let span = rows.last().map_or(0, |l| l.t) - rows[0].t;
        let step = span / max.max(1) as u64;
        let mut out: Vec<Entry> = Vec::with_capacity(max * 2);
        for (i, e) in rows.iter().enumerate() {
            let changed = i > 0 && {
                let p = rows[i - 1];
                p.charging != e.charging || p.awake != e.awake || p.hz != e.hz
            };
            let last = i + 1 == rows.len();
            if out.last().is_none_or(|o| e.t >= o.t + step) || changed || last {
                out.push(*e);
            }
        }
        out
    }
}

/// How long a full charge lasts in use at one polling rate.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLife {
    pub hz: u32,
    /// Hours of use from full to empty.
    pub hours: f64,
    /// Hours of use it was measured over.
    pub measured_hours: f64,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// When the log starts (unix seconds).
    pub since: Option<u64>,
    /// Rates with enough use behind them, lowest first.
    pub life: Vec<RateLife>,
    /// Percent lost per day asleep.
    pub standby_per_day: Option<f64>,
    /// Hours a day the mouse is awake, over the last two weeks.
    pub awake_per_day: Option<f64>,
    /// The rate used most over the last two weeks.
    pub usual_hz: Option<u32>,
    /// Days a full charge lasts at that use and that rate.
    pub days_per_charge: Option<f64>,
    /// Full discharges' worth of use since the log starts.
    pub cycles: f64,
    pub charges: u32,
    /// Hours of use per charge in the last two weeks as a share of the first two, at the same rate.
    pub capacity: Option<f64>,
}

#[derive(Default, Clone, Copy)]
struct Use {
    secs: f64,
    drop: f64,
}

impl Use {
    fn hours_per_charge(&self) -> Option<f64> {
        let hours = self.secs / 3600.0;
        (hours >= MIN_HOURS && self.drop >= MIN_DROP).then(|| 100.0 * hours / self.drop)
    }
}

/// Use per polling rate between `from` and `to`, and time and drain asleep.
fn usage(entries: &[Entry], from: u64, to: u64) -> (BTreeMap<u32, Use>, Use) {
    let mut awake = BTreeMap::<u32, Use>::new();
    let mut asleep = Use::default();
    for w in entries.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dt = b.t.saturating_sub(a.t);
        // Charging, or a stretch the app didn't see.
        if a.t < from || a.t >= to || dt == 0 || dt > GAP || a.charging || b.charging {
            continue;
        }
        let u = if a.awake { awake.entry(a.hz).or_default() } else { &mut asleep };
        u.secs += dt as f64;
        u.drop += a.level.saturating_sub(b.level) as f64;
    }
    (awake, asleep)
}

pub fn summary(entries: &[Entry], now: u64) -> Summary {
    let Some(first) = entries.first() else { return Summary::default() };
    let (all, asleep) = usage(entries, 0, u64::MAX);
    let life = all
        .iter()
        .filter_map(|(&hz, u)| Some(RateLife { hz, hours: u.hours_per_charge()?, measured_hours: u.secs / 3600.0 }))
        .collect::<Vec<_>>();
    let asleep_hours = asleep.secs / 3600.0;
    let standby_per_day = (asleep_hours >= 12.0).then(|| asleep.drop / asleep_hours * 24.0);

    // Typical use: the last two weeks, or as much of them as the log covers. Time the PC was off
    // counts as days without use, which is what they were.
    let recent_from = now.saturating_sub(WINDOW);
    let (recent, _) = usage(entries, recent_from, u64::MAX);
    let last = entries.last().map_or(first.t, |e| e.t);
    let covered = last.saturating_sub(first.t.max(recent_from)) as f64 / DAY as f64;
    let recent_secs: f64 = recent.values().map(|u| u.secs).sum();
    let awake_per_day = (covered >= 1.0 && recent_secs > 0.0).then(|| recent_secs / 3600.0 / covered);
    let usual_hz = recent.iter().max_by(|a, b| a.1.secs.total_cmp(&b.1.secs)).map(|(&hz, _)| hz);
    let days_per_charge = (|| {
        let per_hour = 100.0 / life.iter().find(|l| Some(l.hz) == usual_hz)?.hours;
        let awake = awake_per_day?.min(24.0);
        let standby = standby_per_day.unwrap_or(0.0) / 24.0;
        Some(100.0 / (awake * per_hour + (24.0 - awake) * standby))
    })();

    // Every drop while not charging counts toward cycles, gaps included: the battery was used
    // whether or not the app saw it.
    let mut drained = 0.0;
    let mut charges = 0;
    for w in entries.windows(2) {
        let (a, b) = (w[0], w[1]);
        if !a.charging && !b.charging {
            drained += a.level.saturating_sub(b.level) as f64;
        }
        if !a.charging && b.charging {
            charges += 1;
        }
    }
    charges += first.charging as u32;

    // Capacity: hours per charge at the same rate, first two weeks against the last two.
    let capacity = (now.saturating_sub(first.t) >= CAPACITY_AFTER)
        .then(|| {
            let (early, _) = usage(entries, first.t, first.t + WINDOW);
            let (hz, then) = early.iter().max_by(|a, b| a.1.secs.total_cmp(&b.1.secs))?;
            let ratio = recent.get(hz)?.hours_per_charge()? / then.hours_per_charge()?;
            (0.5..=1.2).contains(&ratio).then_some(ratio.min(1.0))
        })
        .flatten();

    Summary {
        since: Some(first.t),
        life,
        standby_per_day,
        awake_per_day,
        usual_hz,
        days_per_charge,
        cycles: drained / 100.0,
        charges,
        capacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints a log's summary and chart rows as JSON, for design labs. Run with
    /// `HRTZ_LOG=<jsonl> HRTZ_OUT=<json> HRTZ_NOW=<unix> cargo test --lib dump_log -- --ignored`.
    #[test]
    #[ignore = "reads a log named by HRTZ_LOG"]
    fn dump_log() {
        let log = BatteryLog::load(Some(PathBuf::from(std::env::var("HRTZ_LOG").unwrap())));
        let now = std::env::var("HRTZ_NOW").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| log.entries().last().map_or(0, |e| e.t));
        let out = serde_json::json!({
            "now": now,
            "summary": summary(log.entries(), now),
            "history": { "1": log.history(now.saturating_sub(DAY), 720), "7": log.history(now.saturating_sub(7 * DAY), 720), "30": log.history(now.saturating_sub(30 * DAY), 720) },
        });
        fs::write(std::env::var("HRTZ_OUT").unwrap(), serde_json::to_string(&out).unwrap()).unwrap();
    }


    const T0: u64 = 1_800_000_000;

    /// Play a mouse at `hz` awake for `awake_h` a day, asleep the rest, draining `per_h` percent
    /// an hour awake and `sleep_per_day` a day asleep, recharged to 100 below 15 %.
    fn simulate(days: u64, hz: u32, awake_h: u64, per_h: f64, sleep_per_day: f64) -> BatteryLog {
        let mut log = BatteryLog::default();
        let mut level = 100.0f64;
        for d in 0..days {
            for m in 0..24 * 60 {
                let t = T0 + d * DAY + m * 60;
                let awake = m < awake_h * 60;
                level -= if awake { per_h / 60.0 } else { sleep_per_day / 1440.0 };
                let charging = level < 15.0;
                if charging {
                    log.record(Entry { t, level: level.round() as u8, mv: 3700, charging: true, hz, awake });
                    level = 100.0;
                    log.record(Entry { t: t + 1, level: 100, mv: 4150, charging: true, hz, awake });
                }
                log.record(Entry { t: t + 2, level: level.round() as u8, mv: 3900, charging: false, hz, awake });
            }
        }
        log
    }

    #[test]
    fn keeps_rows_only_when_something_changes_or_time_passes() {
        let mut log = BatteryLog::default();
        let e = Entry { t: T0, level: 80, mv: 3900, charging: false, hz: 8000, awake: true };
        assert!(log.record(e));
        assert!(!log.record(Entry { t: T0 + 60, mv: 3890, ..e }));
        assert!(log.record(Entry { t: T0 + 120, level: 79, ..e }));
        assert!(log.record(Entry { t: T0 + 130, level: 79, hz: 1000, ..e }));
        assert!(log.record(Entry { t: T0 + 130 + EVERY_AWAKE, level: 79, hz: 1000, ..e }));
        assert!(!log.record(Entry { t: T0, ..e }), "the clock went back");
    }

    #[test]
    fn works_out_battery_life_at_each_rate() {
        // 6 % an hour awake at 8K, 4 hours a day, 1 % a day asleep, for ten days.
        let log = simulate(10, 8000, 4, 6.0, 1.0);
        let s = summary(log.entries(), T0 + 10 * DAY);
        let life = &s.life[0];
        assert_eq!(life.hz, 8000);
        assert!((life.hours - 100.0 / 6.0).abs() < 1.0, "{life:?}");
        assert!((s.awake_per_day.unwrap() - 4.0).abs() < 0.3, "{:?}", s.awake_per_day);
        assert!((s.standby_per_day.unwrap() - 1.0).abs() < 0.5, "{:?}", s.standby_per_day);
        // 24 + 0.8 % a day: about 4 days a charge.
        assert!((s.days_per_charge.unwrap() - 100.0 / 24.8).abs() < 0.5, "{:?}", s.days_per_charge);
        // 248 % drained over ten days, in two recharges.
        assert!((s.cycles - 2.4).abs() < 0.3, "{}", s.cycles);
        assert_eq!(s.charges, 2);
        assert_eq!(s.capacity, None, "too early to say");
    }

    #[test]
    fn capacity_compares_the_first_weeks_with_the_last() {
        let mut log = simulate(14, 8000, 4, 6.0, 1.0);
        // A month later the same use drains 20 % faster.
        let later = simulate(14, 8000, 4, 7.2, 1.0);
        for e in later.entries() {
            log.record(Entry { t: e.t + 30 * DAY, ..*e });
        }
        let s = summary(log.entries(), T0 + 44 * DAY);
        let c = s.capacity.unwrap();
        assert!((c - 6.0 / 7.2).abs() < 0.05, "{c}");
    }

    #[test]
    fn gaps_the_app_did_not_see_are_left_out_of_battery_life() {
        let mut log = BatteryLog::default();
        let e = Entry { t: T0, level: 90, mv: 3950, charging: false, hz: 8000, awake: true };
        log.record(e);
        // The PC was off for a day; the level fell meanwhile.
        log.record(Entry { t: T0 + DAY, level: 60, ..e });
        let s = summary(log.entries(), T0 + DAY);
        assert!(s.life.is_empty());
        assert!((s.cycles - 0.3).abs() < 1e-9, "still counts toward cycles");
    }

    #[test]
    fn history_thins_but_keeps_changes() {
        let log = simulate(3, 8000, 4, 6.0, 1.0);
        let h = log.history(T0, 100);
        assert!(h.len() < 200, "{}", h.len());
        let wakes = log.entries().windows(2).filter(|w| w[0].awake != w[1].awake).count();
        let kept = h.windows(2).filter(|w| w[0].awake != w[1].awake).count();
        assert_eq!(wakes, kept);
    }
}
