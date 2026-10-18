//! What the HUD says and when, with no Win32 in it: the tabs for a device state, which of them are
//! up at a given moment, and when that next changes. Times are milliseconds on one clock.
//!
//! Only what the hardware backs up (see the HUD brief): no signal strength, since the receiver
//! reports none, and no measured report rate, since Windows coalesces a background app's raw
//! input while a game is in front. The honest stand-in for the link is a yellow dot on hrtz's
//! mark while the mouse doesn't answer.

use crate::mice::DeviceState;
use crate::store::{HudFields, HudShow, MousePrefs, Prefs};

/// How long a tab stays up after its value changes, and after a game comes to the front.
pub const SHOW_MS: u64 = 3000;
/// How long a tab that just changed keeps its brighter plate.
pub const BRIGHT_MS: u64 = 1500;
/// In "always", OLED care dims the HUD after this long with nothing changing.
pub const IDLE_MS: u64 = 10_000;
/// In "always", OLED care moves the HUD a pixel or two this often.
pub const SHIFT_MS: u64 = 180_000;
/// The dimmed HUD's opacity.
pub const DIM: f32 = 0.55;
/// The HUD page's preview ends this long after the page last asked for it. The page asks
/// again every minute while its switch is on, so this only ends a test the page didn't (a crash,
/// or a window that went away without saying).
pub const TEST_MS: u64 = 5 * 60_000;
/// Battery levels worth a look on the way down, as in the lab, besides the low level itself.
const BATTERY_STEPS: [u8; 4] = [20, 15, 10, 5];

/// Every fact has its own tab and keeps its slot, so nothing jumps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Field {
    /// hrtz's mark, which always leads: the emblem, muted, with a yellow dot (tone `Warn`) while
    /// the mouse is away. It shows whenever a value does.
    Mark,
    Battery,
    Polling,
    Dpi,
    Profile,
    LongRange,
    /// The note that Zaopin's own app is open. It shows whenever a value does.
    #[default]
    Vendor,
}

impl Field {
    const COUNT: usize = 7;

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Plain,
    /// Amber: the battery is low. On the mark, the yellow dot: the mouse is away.
    Warn,
    /// Green: charging.
    Charge,
}

/// One "Label: value" tab, as Edge draws it: "Battery: 82% • 11h", "DPI: 800 2/4". Without
/// labels, the value alone: "82% • 11h", "800 DPI 2/4".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tab {
    pub field: Field,
    /// Empty when labels are off. The mark's is "hrtz", for tests and logs.
    pub label: &'static str,
    pub value: String,
    /// A second value after a "•", in the value's colour (time left).
    pub extra: Option<String>,
    /// Small grey text after the value ("charging", "battery saver", the DPI stage).
    pub note: Option<String>,
    pub tone: Tone,
    /// Stays up in every mode but off: a low battery is why you'd look.
    pub sticky: bool,
}

impl Tab {
    /// The tab's text as one line, for tests and logs. The mark is "hrtz", or "hrtz •" with its
    /// dot.
    pub fn text(&self) -> String {
        match self.field {
            Field::Vendor => return self.note.clone().unwrap_or_default(),
            Field::Mark if self.tone == Tone::Warn => return format!("{} •", self.label),
            Field::Mark => return self.label.to_string(),
            _ => {}
        }
        let mut s = if self.label.is_empty() { self.value.clone() } else { format!("{}: {}", self.label, self.value) };
        if let Some(x) = &self.extra {
            s += &format!(" • {x}");
        }
        if let Some(n) = &self.note {
            s += &format!(" {n}");
        }
        s
    }
}

pub const VENDOR_NOTE: &str = "Values may be late while Zaopin's app is open";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    NoReceiver,
    /// The receiver is there but the mouse doesn't answer: asleep, off or out of range. The
    /// receiver can't tell which, and a mouse nobody is using sleeps, so it's no alarm: only the
    /// mark's yellow dot.
    Away,
    Cable,
    Receiver,
}

/// What the HUD knows, gathered from the device state, the prefs and the app's status.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub link: Link,
    pub long_range: bool,
    /// Level and charging.
    pub battery: Option<(u8, bool)>,
    /// The rate set, capped at what this connection carries.
    pub polling_hz: Option<u32>,
    /// The battery saver has lowered the rate.
    pub saver: bool,
    /// DPI of the stage in use, the stage (0-based) and how many stages there are.
    pub dpi: Option<(u32, u8, u8)>,
    pub profile: Option<String>,
    /// Hours of use left at this rate, once the battery log has measured the rate.
    pub time_left_h: Option<f64>,
    pub low_at: u8,
    pub vendor_app: bool,
}

impl Facts {
    /// `life_h` is the hours a full charge lasts at a polling rate, from the battery log.
    pub fn of(
        s: &DeviceState,
        prefs: &Prefs,
        mouse: &MousePrefs,
        vendor_app: bool,
        life_h: impl Fn(u32) -> Option<f64>,
    ) -> Self {
        let st = s.settings.as_ref();
        let caps = s.capabilities.as_ref();
        let wired = caps.is_some_and(|c| c.wired);
        let link = match (s.connected, s.online, wired) {
            (false, ..) => Link::NoReceiver,
            (true, false, _) => Link::Away,
            (true, true, true) => Link::Cable,
            (true, true, false) => Link::Receiver,
        };
        let battery = s.battery.as_ref().map(|b| (b.level, b.charging));
        // The battery log keys life by the rate set, as the Battery page does; until it has one,
        // the maker's figure.
        let rated =
            |hz: u32| caps.and_then(|c| c.rated_hours.as_ref()?.iter().find(|(h, _)| *h == hz).map(|(_, l)| *l));
        let time_left_h = match (battery, st.and_then(|x| x.polling_hz)) {
            (Some((level, false)), Some(hz)) => life_h(hz).or_else(|| rated(hz)).map(|h| h * level as f64 / 100.0),
            _ => None,
        };
        // Zaopin's app gets in the way of a Zaopin only.
        let vendor_app = vendor_app && s.key.as_deref().and_then(crate::mice::family_of) == Some("compx");
        Self {
            link,
            long_range: s.long_range == Some(true),
            battery,
            polling_hz: st
                .and_then(|x| x.polling_hz)
                .map(|hz| caps.and_then(|c| c.top_rate()).map_or(hz, |top| hz.min(top))),
            saver: mouse.battery_saver_restore.is_some(),
            dpi: st.and_then(|x| Some((x.dpi()?, x.dpi_stage?, x.dpi_stage_count?))),
            profile: s.profile.map(|i| mouse.profile_name(i)),
            time_left_h,
            low_at: prefs.low_battery_percent,
            vendor_app,
        }
    }

    /// A mouse like this PC's when none is connected, so the HUD page's preview has something
    /// to show: 50%, 8,000 Hz, 1,600 DPI.
    pub fn sample(prefs: &Prefs) -> Self {
        Self {
            link: Link::Receiver,
            long_range: false,
            battery: Some((50, false)),
            polling_hz: Some(8000),
            saver: false,
            dpi: Some((1600, 0, 1)),
            profile: Some("Profile 1".into()),
            time_left_h: Some(6.5),
            low_at: prefs.low_battery_percent,
            vendor_app: false,
        }
    }

    fn low(&self) -> bool {
        self.battery.is_some_and(|(level, charging)| !charging && level <= self.low_at)
    }

    /// What counts as a change for each field but the battery and the mark (see `compare`).
    fn key(&self, field: Field) -> Option<Key> {
        Some(match field {
            Field::Mark | Field::Battery | Field::Vendor => return None,
            Field::Polling => Key::Polling(self.polling_hz?, self.saver),
            Field::Dpi => Key::Dpi(self.dpi?),
            Field::Profile => Key::Profile(self.profile.clone()?),
            Field::LongRange => Key::LongRange(self.long_range),
        })
    }

    /// The battery changes when charging starts or stops, when it goes low or stops being low,
    /// and when it drops past 20, 15, 10 or 5%: not with every percent.
    fn battery_changed(&self, after: &Facts) -> bool {
        let (Some((a, a_charging)), Some((b, b_charging))) = (self.battery, after.battery) else { return false };
        let dropped_past = BATTERY_STEPS.iter().any(|&w| a > w && b <= w);
        a_charging != b_charging || self.low() != after.low() || (!b_charging && dropped_past)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Key {
    Polling(u32, bool),
    Dpi((u32, u8, u8)),
    Profile(String),
    LongRange(bool),
}

/// 8000 → "8,000".
pub fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A polling rate, compact as the Sensor page writes it: 8000 → "8K Hz", 500 → "500 Hz".
pub fn rate(hz: u32) -> String {
    match hz {
        0..1000 => format!("{hz} Hz"),
        _ if hz.is_multiple_of(1000) => format!("{}K Hz", hz / 1000),
        _ => format!("{:.1}K Hz", hz as f64 / 1000.0),
    }
}

/// Time left, compact for a tab: 11 → "11h", 6.5 → "6h 30m", 0.75 → "45m".
pub fn short_hours(h: f64) -> String {
    let m = (h * 60.0).round().max(0.0) as u64;
    if m < 60 {
        return format!("{m}m");
    }
    let (hh, mm) = (m / 60, m % 60);
    if hh >= 10 || mm == 0 { format!("{}h", h.round() as u64) } else { format!("{hh}h {mm}m") }
}

/// The tabs for these facts, in their fixed order: the mark, the values, then the note about
/// Zaopin's app.
pub fn tabs(f: &Facts, fields: &HudFields) -> Vec<Tab> {
    // Without the receiver there's no mouse to say anything about.
    if f.link == Link::NoReceiver {
        return Vec::new();
    }
    let away = f.link == Link::Away;
    // Without labels each value says what it is by itself: "82%", "8K Hz", "800 DPI".
    let labels = fields.labels;
    let label = |name: &'static str| if labels { name } else { "" };
    let mut out = Vec::new();
    if let (true, Some((level, charging))) = (fields.battery, f.battery) {
        // Away, the level is the last one read, and nothing is being used up.
        let left = f.time_left_h.filter(|_| fields.time_left && !away && !charging).map(short_hours);
        out.push(Tab {
            field: Field::Battery,
            label: label("Battery"),
            value: format!("{level}%"),
            extra: left,
            note: charging.then(|| "charging".into()),
            tone: if charging {
                Tone::Charge
            } else if f.low() {
                Tone::Warn
            } else {
                Tone::Plain
            },
            sticky: f.low(),
        });
    }
    if let (true, Some(hz)) = (fields.polling, f.polling_hz) {
        out.push(Tab {
            field: Field::Polling,
            label: label("Polling"),
            value: rate(hz),
            note: f.saver.then(|| "battery saver".into()),
            ..Tab::default()
        });
    }
    if let (true, Some((dpi, stage, count))) = (fields.dpi, f.dpi) {
        out.push(Tab {
            field: Field::Dpi,
            label: label("DPI"),
            value: if labels { thousands(dpi) } else { format!("{} DPI", thousands(dpi)) },
            note: (count > 1).then(|| format!("{}/{count}", stage + 1)),
            ..Tab::default()
        });
    }
    if let (true, Some(name)) = (fields.profile, &f.profile) {
        out.push(Tab { field: Field::Profile, label: label("Profile"), value: name.clone(), ..Tab::default() });
    }
    // No tab for the cable: the battery's says it's charging.
    if f.long_range {
        let value = if labels { "On" } else { "Long range" };
        out.push(Tab { field: Field::LongRange, label: label("Long range"), value: value.into(), ..Tab::default() });
    }
    // Zaopin's app switches the mouse's status pushes off, so what the HUD shows may lag.
    if f.vendor_app && !out.is_empty() {
        out.push(Tab { field: Field::Vendor, note: Some(VENDOR_NOTE.into()), ..Tab::default() });
    }
    // hrtz's mark leads whatever there is to show, with the yellow dot while the mouse is away.
    if !out.is_empty() || away {
        let tone = if away { Tone::Warn } else { Tone::Plain };
        out.insert(0, Tab { field: Field::Mark, label: "hrtz", tone, ..Tab::default() });
    }
    out
}

/// When each field last changed, when a game last came to the front, and when anything last did.
#[derive(Clone, Debug, Default)]
pub struct Moments {
    changed: [Option<u64>; Field::COUNT],
    game_at: Option<u64>,
    last_any: Option<u64>,
}

impl Moments {
    /// Note what differs between two sets of facts. Only a known value turning into another
    /// counts: a value read for the first time (on connect, or once the settings load) doesn't.
    pub fn compare(&mut self, before: &Facts, after: &Facts, now: u64) {
        if before.battery_changed(after) {
            self.changed(Field::Battery, now);
        }
        // The mouse going away brings the mark's dot up for a moment. Coming back is the usual
        // state again, and a receiver plugged in while the mouse sleeps is a first reading.
        if matches!(before.link, Link::Receiver | Link::Cable) && after.link == Link::Away {
            self.changed(Field::Mark, now);
        }
        for field in [Field::Polling, Field::Dpi, Field::Profile, Field::LongRange] {
            if let (Some(a), Some(b)) = (before.key(field), after.key(field))
                && a != b
            {
                self.changed(field, now);
            }
        }
    }

    pub fn changed(&mut self, field: Field, now: u64) {
        self.changed[field.index()] = Some(now);
        self.last_any = Some(now);
    }

    /// A game came to the front: every tab shows for a moment.
    pub fn game(&mut self, now: u64) {
        self.game_at = Some(now);
        self.last_any = Some(now);
    }

    fn within(at: Option<u64>, now: u64, ms: u64) -> bool {
        at.is_some_and(|t| now >= t && now - t < ms)
    }
}

/// Everything that decides what's up, besides the tabs themselves.
#[derive(Clone, Copy, Debug)]
pub struct Scene {
    pub show: HudShow,
    pub oled_care: bool,
    /// A game the HUD shows in is in front (known, not hidden, not in exclusive fullscreen).
    pub in_game: bool,
    /// When the HUD page last asked to see the HUD on screen without a game (see `TEST_MS`).
    pub test_at: Option<u64>,
    pub now: u64,
}

impl Scene {
    /// The HUD page's preview is on: asked for in the last 5 minutes, with the HUD on.
    /// It shows every chosen tab over whatever is in front, so the page can show the real thing.
    pub fn testing(&self) -> bool {
        self.show != HudShow::Off && Moments::within(self.test_at, self.now, TEST_MS)
    }
}

/// What the HUD should look like now, and when that next changes on its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub visible: Vec<bool>,
    /// The tab just changed: its plate is brighter for a moment.
    pub bright: Vec<bool>,
    /// OLED care: dimmed after a while with nothing changing.
    pub dim: bool,
    /// OLED care: pixels to move in from the edge (0, 1, 2, 1 in turn).
    pub shift: i32,
    pub next: Option<u64>,
}

pub fn plan(tabs: &[Tab], scene: &Scene, m: &Moments) -> Plan {
    let now = scene.now;
    // The preview needs no game: the page is in front, and the HUD shows over it.
    let testing = scene.testing();
    let on = scene.show != HudShow::Off && (scene.in_game || testing);
    let preflight = Moments::within(m.game_at, now, SHOW_MS);
    let changed = |t: &Tab| m.changed[t.field.index()];
    // A tab on its own account: sticky, the test, a game just come to the front, or the mode.
    let alone = |t: &Tab| {
        on && t.field != Field::Vendor
            && (t.sticky
                || testing
                || preflight
                || match scene.show {
                    HudShow::Always => true,
                    HudShow::Change => Moments::within(changed(t), now, SHOW_MS),
                    HudShow::Off => false,
                })
    };
    // The mark and the note about Zaopin's app go with the values. The mark also shows on its
    // own for a moment when its dot comes on.
    let any = tabs.iter().any(|t| t.field != Field::Mark && alone(t));
    let visible = tabs
        .iter()
        .map(|t| match t.field {
            Field::Mark => any || alone(t),
            Field::Vendor => any,
            _ => alone(t),
        })
        .collect();
    let bright = tabs.iter().map(|t| t.field != Field::Vendor && Moments::within(changed(t), now, BRIGHT_MS)).collect();

    // OLED care, for the always-on mode: dim after 10 s with nothing changing, full again on a
    // change; and every 3 minutes move a pixel or two, so nothing sits on the same pixels for
    // hours. Not during the preview, which is short and should be easy to see.
    let always = on && scene.show == HudShow::Always && !testing && scene.oled_care;
    let dim = always && m.last_any.is_none_or(|t| now.saturating_sub(t) > IDLE_MS);
    let shift = if always { [0, 1, 2, 1][(now / SHIFT_MS % 4) as usize] } else { 0 };

    // The next moment any of that changes by itself.
    let mut next: Option<u64> = None;
    let mut at = |t: u64| {
        if t > now {
            next = Some(next.map_or(t, |n| n.min(t)));
        }
    };
    if on {
        for t in m.changed.iter().flatten() {
            at(t + SHOW_MS);
            at(t + BRIGHT_MS);
        }
        if let Some(g) = m.game_at {
            at(g + SHOW_MS);
        }
        // The test ends by itself if the page stops asking for it.
        if let (true, Some(t)) = (testing, scene.test_at) {
            at(t + TEST_MS);
        }
        if always {
            if let Some(t) = m.last_any {
                at(t + IDLE_MS + 1);
            }
            at((now / SHIFT_MS + 1) * SHIFT_MS);
        }
    }
    Plan { visible, bright, dim, shift, next }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mice::Battery;
    use crate::mice::compx::{image, models};
    use crate::mice::model::{Capabilities, DpiStage, Settings};

    fn prefs() -> Prefs {
        Prefs::default()
    }

    /// This PC's mouse: 50%, 8,000 Hz, 1,600 DPI on one stage, on its receiver.
    fn yours() -> Facts {
        Facts::sample(&prefs())
    }

    fn texts(f: &Facts) -> Vec<String> {
        tabs(f, &HudFields::default()).iter().map(Tab::text).collect()
    }

    /// The tab for `field`, with the tabs chosen by default.
    fn tab(f: &Facts, field: Field) -> Tab {
        tabs(f, &HudFields::default()).into_iter().find(|t| t.field == field).unwrap()
    }

    #[test]
    fn compact_time_left() {
        assert_eq!(short_hours(11.0), "11h");
        assert_eq!(short_hours(10.7), "11h");
        assert_eq!(short_hours(6.5), "6h 30m");
        assert_eq!(short_hours(1.8), "1h 48m");
        assert_eq!(short_hours(2.0), "2h");
        assert_eq!(short_hours(0.75), "45m");
        assert_eq!(short_hours(0.0), "0m");
        assert_eq!(short_hours(9.999), "10h");
        assert_eq!(thousands(8000), "8,000");
        assert_eq!(thousands(800), "800");
        assert_eq!(thousands(26000), "26,000");
    }

    #[test]
    fn tabs_for_each_scenario() {
        assert_eq!(texts(&yours()), ["hrtz", "Battery: 50% • 6h 30m", "Polling: 8K Hz", "DPI: 1,600"]);

        let stages = Facts { dpi: Some((800, 1, 4)), battery: Some((82, false)), time_left_h: Some(10.7), ..yours() };
        assert_eq!(texts(&stages), ["hrtz", "Battery: 82% • 11h", "Polling: 8K Hz", "DPI: 800 2/4"]);

        let low = Facts { battery: Some((14, false)), time_left_h: Some(1.8), ..yours() };
        let t = tab(&low, Field::Battery);
        assert_eq!(t.text(), "Battery: 14% • 1h 48m");
        assert_eq!((t.tone, t.sticky), (Tone::Warn, true));
        let at_level = Facts { battery: Some((15, false)), ..yours() };
        assert!(tab(&at_level, Field::Battery).sticky, "at the low level counts as low");

        let charging = Facts { battery: Some((62, true)), ..yours() };
        let t = tab(&charging, Field::Battery);
        assert_eq!(t.text(), "Battery: 62% charging", "no time left while charging");
        assert_eq!((t.tone, t.sticky), (Tone::Charge, false));
        let charging_low = Facts { battery: Some((5, true)), ..yours() };
        assert!(!tab(&charging_low, Field::Battery).sticky);

        let saver =
            Facts { polling_hz: Some(2000), saver: true, time_left_h: None, battery: Some((48, false)), ..yours() };
        assert_eq!(texts(&saver), ["hrtz", "Battery: 48%", "Polling: 2K Hz battery saver", "DPI: 1,600"]);

        // On its cable the battery's tab says it's charging, and there's no tab for the cable.
        let cable = Facts { link: Link::Cable, battery: Some((71, true)), ..yours() };
        assert_eq!(texts(&cable), ["hrtz", "Battery: 71% charging", "Polling: 8K Hz", "DPI: 1,600"]);
        let long_range = Facts { long_range: true, ..yours() };
        assert_eq!(texts(&long_range).last().unwrap(), "Long range: On");

        let vendor = Facts { vendor_app: true, ..yours() };
        assert_eq!(texts(&vendor).last().unwrap(), VENDOR_NOTE);

        let all = HudFields { profile: true, ..HudFields::default() };
        assert_eq!(tabs(&yours(), &all)[4].text(), "Profile: Profile 1");
        let few = HudFields { battery: false, polling: false, time_left: false, ..HudFields::default() };
        assert_eq!(tabs(&yours(), &few).iter().map(Tab::text).collect::<Vec<_>>(), ["hrtz", "DPI: 1,600"]);
        let no_time = HudFields { time_left: false, ..HudFields::default() };
        assert_eq!(tabs(&yours(), &no_time)[1].text(), "Battery: 50%");
        // Nothing to say, so neither the mark nor the note about Zaopin's app.
        let nothing = HudFields {
            battery: false,
            polling: false,
            dpi: false,
            time_left: false,
            profile: false,
            ..HudFields::default()
        };
        assert!(tabs(&vendor, &nothing).is_empty());
    }

    #[test]
    fn rates_are_compact_from_1k() {
        let all = [125, 250, 500, 1000, 2000, 4000, 8000, 1500].map(rate);
        assert_eq!(all, ["125 Hz", "250 Hz", "500 Hz", "1K Hz", "2K Hz", "4K Hz", "8K Hz", "1.5K Hz"]);
    }

    #[test]
    fn without_labels_each_value_says_what_it_is() {
        let bare = HudFields { labels: false, profile: true, ..HudFields::default() };
        let texts = |f: &Facts| tabs(f, &bare).iter().map(Tab::text).collect::<Vec<_>>();
        assert_eq!(texts(&yours()), ["hrtz", "50% • 6h 30m", "8K Hz", "1,600 DPI", "Profile 1"]);
        // The DPI keeps its unit, and its stage after it.
        let stages = Facts { dpi: Some((800, 1, 4)), polling_hz: Some(500), ..yours() };
        assert_eq!(texts(&stages)[2..4], ["500 Hz", "800 DPI 2/4"]);
        let notes =
            Facts { battery: Some((62, true)), saver: true, polling_hz: Some(2000), long_range: true, ..yours() };
        assert_eq!(texts(&notes)[1..3], ["62% charging", "2K Hz battery saver"]);
        assert_eq!(texts(&notes).last().unwrap(), "Long range");
        // Colours and the low battery's stickiness don't change.
        let low = tabs(&Facts { battery: Some((14, false)), ..yours() }, &bare);
        assert_eq!((low[1].label, low[1].tone, low[1].sticky), ("", Tone::Warn, true));
    }

    #[test]
    fn the_mark_leads_and_its_dot_shows_while_the_mouse_is_away() {
        let fields = HudFields::default();
        let away = Facts { link: Link::Away, ..yours() };
        let t = tabs(&away, &fields);
        assert_eq!(
            t.iter().map(Tab::text).collect::<Vec<_>>(),
            ["hrtz •", "Battery: 50%", "Polling: 8K Hz", "DPI: 1,600"],
            "no time left while away"
        );
        assert_eq!((t[0].field, t[0].tone, t[0].sticky), (Field::Mark, Tone::Warn, false), "no alarm");
        // The mark and its dot show even with no values chosen.
        let nothing = HudFields {
            battery: false,
            polling: false,
            dpi: false,
            time_left: false,
            profile: false,
            ..HudFields::default()
        };
        assert_eq!(tabs(&away, &nothing).iter().map(Tab::text).collect::<Vec<_>>(), ["hrtz •"]);
        // No receiver, no HUD: there's no mouse to say anything about.
        assert!(tabs(&Facts { link: Link::NoReceiver, ..yours() }, &fields).is_empty());
    }

    #[test]
    fn facts_come_from_the_device_state_and_prefs() {
        let mut caps = models::capabilities(4, 1); // a 4K receiver
        caps.buttons = None;
        let st = Settings {
            polling_hz: Some(8000),
            dpi_stage_count: Some(4),
            dpi_stage: Some(1),
            stages: Some([400, 800, 1600, 3200].iter().map(|&dpi| DpiStage { dpi, color: "#ffffff".into() }).collect()),
            ..image::decode(&[0xFF; crate::mice::compx::protocol::FLASH_SIZE])
        };
        let s = DeviceState {
            key: Some("compx/7-4".into()),
            connected: true,
            online: true,
            battery: Some(Battery { level: 80, raw_level: 80, charging: false, millivolts: 4000 }),
            profile: Some(2),
            long_range: Some(false),
            settings: Some(st),
            capabilities: Some(caps),
            ..DeviceState::default()
        };
        let p = prefs();
        let m = MousePrefs { battery_saver_restore: Some(8000), ..MousePrefs::default() };
        let f = Facts::of(&s, &p, &m, true, |hz| (hz == 8000).then_some(12.0));
        assert_eq!(f.polling_hz, Some(4000), "capped at what the receiver carries");
        assert_eq!(f.dpi, Some((800, 1, 4)));
        assert_eq!(f.profile.as_deref(), Some("Profile 3"));
        assert!((f.time_left_h.unwrap() - 9.6).abs() < 1e-9);
        assert!(f.saver && f.vendor_app && f.link == Link::Receiver);
        // Not measured yet: the maker's figure, where it gives one.
        let rated = DeviceState {
            capabilities: Some(Capabilities {
                rated_hours: Some(vec![(8000, 20.0)]),
                ..s.capabilities.clone().unwrap()
            }),
            ..s.clone()
        };
        assert!((Facts::of(&rated, &p, &m, false, |_| None).time_left_h.unwrap() - 16.0).abs() < 1e-9);
        assert!(Facts::of(&s, &p, &m, false, |_| None).time_left_h.is_none(), "neither");
        let logitech = DeviceState { key: Some("hidpp/c094/1a2b3c4d".into()), ..s.clone() };
        assert!(!Facts::of(&logitech, &p, &m, true, |_| None).vendor_app, "Zaopin's app is no matter to a Logitech");
        let asleep = Facts::of(&DeviceState { online: false, ..s.clone() }, &p, &m, false, |_| None);
        assert_eq!(asleep.link, Link::Away);
        assert_eq!(Facts::of(&DeviceState::default(), &p, &m, false, |_| None).link, Link::NoReceiver);
    }

    fn scene(show: HudShow, now: u64) -> Scene {
        Scene { show, oled_care: true, in_game: true, test_at: None, now }
    }

    fn shown(f: &Facts, s: &Scene, m: &Moments) -> Vec<String> {
        let t = tabs(f, &HudFields::default());
        let p = plan(&t, s, m);
        t.iter().zip(&p.visible).filter(|(_, v)| **v).map(|(t, _)| t.label.to_string()).collect()
    }

    #[test]
    fn each_mode_shows_what_it_should() {
        let f = yours();
        let mut m = Moments::default();
        m.game(0);
        // Every tab for 3 s when a game comes to the front, in every mode but off.
        for show in [HudShow::Change, HudShow::Always] {
            assert_eq!(shown(&f, &scene(show, 2999), &m), ["hrtz", "Battery", "Polling", "DPI"], "{show:?}");
        }
        assert!(shown(&f, &scene(HudShow::Off, 100), &m).is_empty());
        assert!(shown(&f, &scene(HudShow::Change, 3000), &m).is_empty());
        assert_eq!(shown(&f, &scene(HudShow::Always, 60_000), &m).len(), 4);

        // "change": only what changed, for 3 s, led by the mark.
        let dpi = Facts { dpi: Some((800, 0, 1)), ..f.clone() };
        m.compare(&f, &dpi, 10_000);
        assert_eq!(shown(&dpi, &scene(HudShow::Change, 12_999), &m), ["hrtz", "DPI"]);
        assert!(shown(&dpi, &scene(HudShow::Change, 13_000), &m).is_empty());

        // Nothing at all without a game in front.
        let away = Scene { in_game: false, ..scene(HudShow::Always, 1000) };
        assert!(shown(&f, &away, &m).is_empty());
    }

    #[test]
    fn the_on_screen_test_shows_every_tab_without_a_game_and_ends_by_itself() {
        let f = yours();
        let t = tabs(&f, &HudFields::default());
        let m = Moments::default();
        // Asked for at 1 s, with the HUD page in front and no game anywhere. A hidden game counts
        // the same: `in_game` is false for it, and the test shows all the same.
        let at = |show, now| Scene { in_game: false, test_at: Some(1000), ..scene(show, now) };
        for show in [HudShow::Change, HudShow::Always] {
            assert_eq!(shown(&f, &at(show, 1000), &m), ["hrtz", "Battery", "Polling", "DPI"], "{show:?}");
        }
        // Only the tabs chosen.
        let few = HudFields { polling: false, ..HudFields::default() };
        assert_eq!(plan(&tabs(&f, &few), &at(HudShow::Change, 2000), &m).visible, [true, true, true]);
        // Nothing while the HUD is off, test or not.
        assert!(shown(&f, &at(HudShow::Off, 2000), &m).is_empty());
        assert!(!at(HudShow::Off, 2000).testing());

        // A change still brightens its tab.
        let mut changed = Moments::default();
        changed.changed(Field::Dpi, 60_000);
        let p = plan(&t, &at(HudShow::Change, 60_500), &changed);
        assert_eq!((p.visible.clone(), p.bright.clone()), (vec![true; 4], vec![false, false, false, true]));
        assert_eq!(p.next, Some(60_000 + BRIGHT_MS));

        // No OLED dimming or moving, even in Always long after the last change.
        let p = plan(&t, &at(HudShow::Always, 1000 + IDLE_MS * 20), &m);
        assert!(!p.dim && p.shift == 0);

        // Five minutes after the last ask it ends, and the plan wakes the HUD then to hide it.
        assert_eq!(plan(&t, &at(HudShow::Change, 1000 + TEST_MS - 1), &m).next, Some(1000 + TEST_MS));
        assert_eq!(shown(&f, &at(HudShow::Change, 1000 + TEST_MS - 1), &m).len(), 4);
        assert!(shown(&f, &at(HudShow::Change, 1000 + TEST_MS), &m).is_empty());
        assert_eq!(plan(&t, &at(HudShow::Change, 1000 + TEST_MS), &m).next, None);
        // Each ask the page sends every minute starts the five minutes again.
        let kept = Scene { test_at: Some(1000 + 4 * 60_000), ..at(HudShow::Change, 1000 + TEST_MS + 60_000) };
        assert_eq!(shown(&f, &kept, &m).len(), 4);
        // A time before the ask (the clock never goes back) isn't a test either.
        assert!(!at(HudShow::Change, 500).testing());
    }

    #[test]
    fn a_low_battery_stays_up_and_the_dot_shows_for_a_moment() {
        let m = Moments::default();
        let low = Facts { battery: Some((14, false)), ..yours() };
        for show in [HudShow::Change, HudShow::Always] {
            assert!(shown(&low, &scene(show, 99_000), &m).contains(&"Battery".to_string()), "{show:?}");
        }
        assert_eq!(shown(&low, &scene(HudShow::Change, 99_000), &m), ["hrtz", "Battery"]);
        assert!(shown(&low, &scene(HudShow::Off, 99_000), &m).is_empty());

        // The mouse going away: in "change" the mark shows with its dot for 3 s, on its own.
        let away = Facts { link: Link::Away, ..yours() };
        let mut m = Moments::default();
        m.compare(&yours(), &away, 10_000);
        assert_eq!(shown(&away, &scene(HudShow::Change, 12_999), &m), ["hrtz"]);
        assert!(shown(&away, &scene(HudShow::Change, 13_000), &m).is_empty(), "not sticky");
        // In "always" it stays, with everything else.
        assert_eq!(shown(&away, &scene(HudShow::Always, 99_000), &m).len(), 4);
        // Coming back isn't news, and nor is a receiver plugged in while the mouse sleeps.
        let mut m = Moments::default();
        m.compare(&away, &yours(), 20_000);
        m.compare(&Facts { link: Link::NoReceiver, ..yours() }, &away, 30_000);
        assert_eq!(m.last_any, None);
    }

    #[test]
    fn the_note_about_zaopins_app_follows_the_values() {
        let f = Facts { vendor_app: true, ..yours() };
        let mut m = Moments::default();
        assert!(shown(&f, &scene(HudShow::Change, 5000), &m).is_empty());
        m.compare(&f, &Facts { polling_hz: Some(1000), ..f.clone() }, 5000);
        assert_eq!(shown(&f, &scene(HudShow::Change, 6000), &m), ["hrtz", "Polling", ""]);
        // Not the mark's dot alone: there's no value to be late.
        let away = Facts { link: Link::Away, ..f.clone() };
        let mut m = Moments::default();
        m.compare(&f, &away, 5000);
        assert_eq!(shown(&away, &scene(HudShow::Change, 6000), &m), ["hrtz"]);
    }

    #[test]
    fn changes_are_what_the_user_would_call_one() {
        let f = yours();
        let mut m = Moments::default();
        // A percent off the battery isn't news; charging, going low and dropping past 20, 15, 10
        // or 5% are, as in the lab.
        m.compare(&f, &Facts { battery: Some((49, false)), ..f.clone() }, 1000);
        assert_eq!(m.changed[Field::Battery.index()], None);
        m.compare(&f, &Facts { battery: Some((49, true)), ..f.clone() }, 2000);
        assert_eq!(m.changed[Field::Battery.index()], Some(2000));
        m.compare(&f, &Facts { battery: Some((15, false)), ..f.clone() }, 3000);
        assert_eq!(m.changed[Field::Battery.index()], Some(3000));
        let at = |level| Facts { battery: Some((level, false)), ..f.clone() };
        let mut m = Moments::default();
        m.compare(&at(22), &at(21), 1000);
        assert_eq!(m.changed[Field::Battery.index()], None);
        m.compare(&at(21), &at(20), 2000);
        assert_eq!(m.changed[Field::Battery.index()], Some(2000), "past 20");
        m.compare(&at(11), &at(10), 3000);
        assert_eq!(m.changed[Field::Battery.index()], Some(3000), "past 10, already low");
        m.compare(&at(20), &at(21), 4000);
        assert_eq!(m.changed[Field::Battery.index()], Some(3000), "not on the way up");
        let charging = |level| Facts { battery: Some((level, true)), ..f.clone() };
        m.compare(&charging(21), &charging(20), 5000);
        assert_eq!(m.changed[Field::Battery.index()], Some(3000), "not while charging");
        // The saver lowering the rate is a polling change.
        m.compare(&f, &Facts { saver: true, ..f.clone() }, 4000);
        assert_eq!(m.changed[Field::Polling.index()], Some(4000));
        // Values read for the first time aren't changes.
        let mut m = Moments::default();
        let empty = Facts { polling_hz: None, dpi: None, battery: None, profile: None, ..f.clone() };
        m.compare(&empty, &f, 1000);
        assert_eq!(m.last_any, None);
    }

    #[test]
    fn bright_plate_and_wake_ups() {
        let f = yours();
        let t = tabs(&f, &HudFields::default());
        let mut m = Moments::default();
        m.changed(Field::Dpi, 1000);
        let p = plan(&t, &scene(HudShow::Change, 2000), &m);
        assert_eq!(p.bright, [false, false, false, true]);
        assert_eq!(p.next, Some(2500), "the plate dims back first");
        let p = plan(&t, &scene(HudShow::Change, 2600), &m);
        assert_eq!((p.bright[3], p.visible[3], p.next), (false, true, Some(4000)));
        assert_eq!(plan(&t, &scene(HudShow::Change, 4000), &m).next, None, "nothing left to time");
    }

    #[test]
    fn oled_care_dims_and_moves_only_in_always() {
        let t = tabs(&yours(), &HudFields::default());
        let mut m = Moments::default();
        m.game(0);
        let p = plan(&t, &scene(HudShow::Always, 5000), &m);
        assert!(!p.dim);
        assert_eq!(p.next, Some(IDLE_MS + 1));
        assert!(plan(&t, &scene(HudShow::Always, IDLE_MS + 1), &m).dim);
        m.changed(Field::Polling, 20_000);
        assert!(!plan(&t, &scene(HudShow::Always, 21_000), &m).dim, "full again on a change");
        let shifts: Vec<i32> = (0..5).map(|i| plan(&t, &scene(HudShow::Always, i * SHIFT_MS + 5), &m).shift).collect();
        assert_eq!(shifts, [0, 1, 2, 1, 0]);
        let off = Scene { oled_care: false, ..scene(HudShow::Always, SHIFT_MS + IDLE_MS * 9) };
        let p = plan(&t, &off, &m);
        assert!(!p.dim && p.shift == 0);
        let change = plan(&t, &scene(HudShow::Change, SHIFT_MS + IDLE_MS * 9), &m);
        assert!(!change.dim && change.shift == 0);
    }
}
