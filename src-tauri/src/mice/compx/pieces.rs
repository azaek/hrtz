//! What a mouse's memory says, a piece at a time: for Inspect a mouse's report of a mouse hrtz
//! doesn't know. Each setting hrtz knows on this channel is looked for where a Zaopin or an ATK
//! keeps it, and read each way hrtz knows to read it. Nothing here talks to a mouse.
//!
//! A setting comes out as far as hrtz gets with it:
//! - **kept**: its check byte is right, so something is kept there the way this channel keeps
//!   things (a field and a byte that bring its sum to 0x55);
//! - **read**: a way hrtz knows turns its bytes into a value that way can hold.
//!
//! Neither says the value is right. A Z11's stages were kept and read, 50 DPI per step, and
//! wrong: its sensor keeps them by a table (Mayank_j's report, 2026-11-12). So a setting kept
//! more than one way comes out read each way, and Inspect a mouse asks its owner which is theirs,
//! or measures it (the polling rate).

use serde::Serialize;
use serde_json::{Value, json};

use super::atk_dpi::{self, Method};
use super::driver::Maker;
use super::image::{self, addr};
use super::protocol::group_valid;
use crate::mice::model::{ButtonAction, DpiAction, MouseButton, ScrollDirection};

/// The least memory the pieces look at: the settings, the buttons and their key combinations.
pub(crate) const NEEDS: usize = 0x300;

/// One way of reading a setting's bytes.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Way {
    /// Which piece read it: a DPI method (`step50`, `ultra`, `table`, `paw3955`), a maker's place
    /// for it (`zaopin`, `atk`) or a scale (`three`, `eleven`).
    pub piece: &'static str,
    /// As its owner would know it: "400 / 800 / 1,600 DPI".
    pub text: String,
    pub value: Value,
    /// Where this way keeps it, when it's not where the setting says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    /// The way its maker's own list gives this model.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub expected: bool,
}

/// One setting, as far as hrtz gets with it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Reading {
    pub id: &'static str,
    pub name: &'static str,
    /// Where it's kept: "0x000C".
    pub at: String,
    /// Its bytes as read, in hex.
    pub bytes: String,
    /// Its check byte is right.
    pub kept: bool,
    /// Each way hrtz knows to read it; none for a value no way holds.
    pub ways: Vec<Way>,
}

/// A memory image, read a piece at a time.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Picture {
    pub settings: Vec<Reading>,
    /// How many of them are kept where hrtz looked, of how many.
    pub kept: usize,
    pub of: usize,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn place(a: usize) -> String {
    format!("0x{a:04X}")
}

/// 12800 as "12,800".
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn way(piece: &'static str, text: impl Into<String>, value: Value) -> Way {
    Way { piece, text: text.into(), value, at: None, expected: false }
}

/// A one-byte setting and its check byte at `a`, read by `read` where its check byte is right.
fn pair(f: &[u8], id: &'static str, name: &'static str, a: usize, read: impl Fn(u8) -> Vec<Way>) -> Reading {
    let value = image::pair(f, a);
    Reading {
        id,
        name,
        at: place(a),
        bytes: hex(&f[a..a + 2]),
        kept: value.is_some(),
        ways: value.map(read).unwrap_or_default(),
    }
}

fn switch(v: u8) -> Vec<Way> {
    match v {
        0 => vec![way("switch", "Off", json!(false))],
        1 => vec![way("switch", "On", json!(true))],
        _ => Vec::new(),
    }
}

fn seconds(s: u32) -> String {
    match s {
        s if s >= 60 && s.is_multiple_of(60) => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

/// How many stages are in use: what the mouse says, else as many as are kept from the first.
fn stages_in_use(f: &[u8]) -> usize {
    image::pair(f, addr::STAGE_COUNT).filter(|n| (1..=image::MAX_STAGES as u8).contains(n)).map_or_else(
        || (0..image::MAX_STAGES).take_while(|i| group_valid(&f[addr::DPI + 4 * i..addr::DPI + 4 * i + 4])).count(),
        |n| n as usize,
    )
}

/// The stages in use, read as `method` keeps them; None unless every one of them reads.
fn stages_as(f: &[u8], method: Method, count: usize) -> Option<Vec<u32>> {
    (0..count)
        .map(|i| {
            let a = atk_dpi::stage_addr(method, i);
            let bytes = f.get(a..a + atk_dpi::stage_len(method))?;
            match method {
                Method::Step50 => group_valid(bytes).then(|| image::dpi_decode(bytes[0], bytes[2])),
                _ => atk_dpi::decode_stage(method, bytes),
            }
        })
        .collect()
}

/// The DPI stages: at 0x0C in four bytes each, read by each of the methods hrtz knows, and a
/// PAW3955's from 0x1B00 where the memory read goes that far. `expected` is the method its maker's
/// list gives the model, where hrtz knows the model: it comes first.
fn stages(f: &[u8], expected: Option<Method>) -> Reading {
    let count = stages_in_use(f).max(1);
    let bytes = &f[addr::DPI..addr::DPI + 4 * count];
    let kept = bytes.chunks(4).all(group_valid);
    let mut ways: Vec<Way> = Vec::new();
    for (method, piece) in
        [(Method::Step50, "step50"), (Method::Ultra, "ultra"), (Method::Table, "table"), (Method::Paw3955, "paw3955")]
    {
        let Some(dpi) = stages_as(f, method, count) else { continue };
        // Read the same by two methods, it's the same answer: the first stands for both.
        let value = json!(dpi);
        if ways.iter().any(|w| w.value == value) {
            continue;
        }
        let text = format!("{} DPI", dpi.iter().map(|d| grouped(*d)).collect::<Vec<_>>().join(" / "));
        ways.push(Way {
            piece,
            text,
            value,
            at: (method == Method::Paw3955).then(|| place(atk_dpi::PAW3955_STAGES)),
            expected: expected == Some(method),
        });
    }
    ways.sort_by_key(|w| !w.expected);
    Reading { id: "stages", name: "DPI stages", at: place(addr::DPI), bytes: hex(bytes), kept, ways }
}

/// The sleep time: a Zaopin's at 0xBB in seconds, an ATK's at 0xAD in tens of seconds. Its
/// maker's place first; the other's too, where something is kept there.
fn sleep(f: &[u8], maker: Maker) -> Reading {
    let zaopin = image::pair(f, addr::SLEEP).filter(|v| *v > 0).map(|v| {
        let s = v as u32;
        Way { at: Some(place(addr::SLEEP)), ..way("zaopin", format!("After {}", seconds(s)), json!(s)) }
    });
    let atk = super::atk::sleep_s(f)
        .filter(|s| *s > 0)
        .map(|s| Way { at: Some(place(0xAD)), ..way("atk", format!("After {}", seconds(s)), json!(s)) });
    let first = if maker == Maker::Zaopin { addr::SLEEP } else { 0xAD };
    let ways: Vec<Way> =
        if maker == Maker::Zaopin { [zaopin, atk] } else { [atk, zaopin] }.into_iter().flatten().collect();
    Reading {
        id: "sleep",
        name: "Sleep",
        at: place(first),
        bytes: hex(&f[first..first + 2]),
        kept: group_valid(&f[first..first + 2]),
        ways,
    }
}

fn button_name(b: &ButtonAction) -> String {
    match b {
        ButtonAction::Disabled => "Off".into(),
        ButtonAction::Mouse { button } => match button {
            MouseButton::Left => "Left click",
            MouseButton::Right => "Right click",
            MouseButton::Middle => "Wheel click",
            MouseButton::Back => "Back",
            MouseButton::Forward => "Forward",
        }
        .into(),
        ButtonAction::Dpi { action } => match action {
            DpiAction::Up => "DPI up",
            DpiAction::Down => "DPI down",
            _ => "DPI cycle",
        }
        .into(),
        ButtonAction::Scroll { direction } => match direction {
            ScrollDirection::Left => "Scroll left",
            ScrollDirection::Right => "Scroll right",
            ScrollDirection::Up => "Scroll up",
            ScrollDirection::Down => "Scroll down",
        }
        .into(),
        ButtonAction::Fire { .. } => "Rapid fire".into(),
        ButtonAction::PollingSwitch => "Polling rate switch".into(),
        ButtonAction::DpiLock { .. } => "DPI lock".into(),
        ButtonAction::Media { .. } => "Media key".into(),
        ButtonAction::Keys { .. } => "Key combination".into(),
        ButtonAction::Macro { .. } => "Macro".into(),
        ButtonAction::Raw { code, p1, p2 } => format!("Not known ({code}, {p1}, {p2})"),
        _ => "Not known".into(),
    }
}

/// The buttons: sixteen slots of four bytes from 0x60, up to the last one that does something.
fn buttons(f: &[u8]) -> Reading {
    let slot = |i: usize| &f[addr::BUTTONS + 4 * i..addr::BUTTONS + 4 * i + 4];
    let kept = (0..image::BUTTON_SLOTS).take_while(|i| group_valid(slot(*i))).count();
    let actions: Vec<ButtonAction> = (0..kept).map(|i| image::decode_button(f, i, Method::Step50)).collect();
    let used = actions.iter().rposition(|b| *b != ButtonAction::Disabled).map_or(0, |i| i + 1);
    let actions = &actions[..used];
    let ways = if used == 0 {
        Vec::new()
    } else {
        vec![way("buttons", actions.iter().map(button_name).collect::<Vec<_>>().join(", "), json!(actions))]
    };
    Reading {
        id: "buttons",
        name: "Buttons",
        at: place(addr::BUTTONS),
        bytes: hex(&f[addr::BUTTONS..addr::BUTTONS + 4 * used.max(1)]),
        // At least the five every mouse has.
        kept: kept >= 5,
        ways,
    }
}

/// `f` (at least `NEEDS` bytes of a mouse's memory), read a piece at a time. `expected` is how
/// its maker's own list says the model keeps its DPI, where hrtz knows the model.
pub(crate) fn read(f: &[u8], maker: Maker, expected: Option<Method>) -> Option<Picture> {
    if f.len() < NEEDS {
        return None;
    }
    let settings = vec![
        pair(f, "polling", "Polling rate", addr::POLLING, |v| {
            image::polling_hz(v).map(|hz| way("rate", format!("{} Hz", grouped(hz)), json!(hz))).into_iter().collect()
        }),
        pair(f, "stageCount", "DPI stages in use", addr::STAGE_COUNT, |v| {
            if (1..=image::MAX_STAGES as u8).contains(&v) {
                vec![way("count", format!("{v} stage{}", if v == 1 { "" } else { "s" }), json!(v))]
            } else {
                Vec::new()
            }
        }),
        pair(f, "stage", "DPI stage in use", addr::STAGE, |v| {
            if (v as usize) < image::MAX_STAGES {
                vec![way("index", format!("Stage {}", v + 1), json!(v))]
            } else {
                Vec::new()
            }
        }),
        stages(f, expected),
        // Lift-off: 1 mm, 2 mm and 0.7 mm by codes 1, 2 and 3; or a PAW3950 Ultra's and a
        // PAW3955's eleven steps from 0.7 mm by codes 1 to 11 (`atk::Sensor::lod`).
        pair(f, "lod", "Lift-off distance", addr::LOD, |v| {
            let three = match v {
                1 => Some("1 mm"),
                2 => Some("2 mm"),
                3 => Some("0.7 mm"),
                _ => None,
            };
            let eleven = (1..=11).contains(&v).then(|| format!("{}.{} mm", (6 + v) / 10, (6 + v) % 10));
            three
                .map(|t| way("three", t, json!(t)))
                .into_iter()
                .chain(eleven.map(|t| way("eleven", t.clone(), json!(t))))
                .collect()
        }),
        pair(f, "debounce", "Debounce", addr::DEBOUNCE, |v| {
            if v <= 30 { vec![way("ms", format!("{v} ms"), json!(v))] } else { Vec::new() }
        }),
        pair(f, "motionSync", "Motion sync", addr::MOTION_SYNC, switch),
        pair(f, "angleSnapping", "Angle snapping", addr::ANGLE_SNAP, switch),
        pair(f, "rippleControl", "Ripple control", addr::RIPPLE, switch),
        sleep(f, maker),
        // A Zaopin's sensor mode: low power, high performance, corded.
        pair(f, "sensorMode", "Sensor mode", addr::SENSOR_MODE, |v| match v {
            0 => vec![way("zaopin", "Low power", json!("lp"))],
            1 => vec![way("zaopin", "High performance", json!("hp"))],
            10 => vec![way("zaopin", "Corded", json!("corded"))],
            _ => Vec::new(),
        }),
        buttons(f),
    ];
    let kept = settings.iter().filter(|s| s.kept).count();
    Some(Picture { kept, of: settings.len(), settings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mice::compx::image::sample_image;
    use crate::mice::compx::protocol::checksum;
    use crate::mice::compx::sim::atk_sample_image;

    fn find<'a>(p: &'a Picture, id: &str) -> &'a Reading {
        p.settings.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("no {id}"))
    }

    fn texts(r: &Reading) -> Vec<&str> {
        r.ways.iter().map(|w| w.text.as_str()).collect()
    }

    #[test]
    fn a_zaopins_memory_reads_whole() {
        let p = read(&sample_image(), Maker::Zaopin, None).unwrap();
        assert_eq!((p.kept, p.of), (12, 12), "{p:#?}");
        assert_eq!(texts(find(&p, "polling")), ["4,000 Hz"]);
        assert_eq!(texts(find(&p, "stageCount")), ["4 stages"]);
        assert_eq!(texts(find(&p, "stage")), ["Stage 2"]);
        assert_eq!(texts(find(&p, "debounce")), ["6 ms"]);
        assert_eq!(texts(find(&p, "motionSync")), ["On"]);
        assert_eq!(texts(find(&p, "angleSnapping")), ["Off"]);
        assert_eq!(texts(find(&p, "sensorMode")), ["High performance"]);
        // A Zaopin's sleep time first; what an ATK would read at its own place after.
        assert_eq!(texts(find(&p, "sleep")), ["After 20 s", "After 30 s"]);
        assert_eq!(find(&p, "sleep").at, "0x00BB");
        let b = find(&p, "buttons");
        assert!(b.ways[0].text.starts_with("Left click, Right click, Wheel click, Back, Forward, DPI cycle"), "{b:?}");
    }

    #[test]
    fn stages_come_out_each_way_they_could_be_read() {
        // 400, 800, 1,600 and 3,200 DPI kept 50 per step: an Ultra would read the same bytes as
        // a fifth of that, and the table has no such codes for all four.
        let p = read(&sample_image(), Maker::Zaopin, None).unwrap();
        let s = find(&p, "stages");
        assert!(s.kept);
        assert_eq!(s.ways[0].piece, "step50");
        assert_eq!(s.ways[0].text, "400 / 800 / 1,600 / 3,200 DPI");
        assert_eq!(s.ways[0].value, json!([400, 800, 1600, 3200]));
        let ultra = s.ways.iter().find(|w| w.piece == "ultra").expect("an Ultra's reading");
        assert_eq!(ultra.text, "80 / 160 / 320 / 640 DPI");
        assert!(s.ways.iter().all(|w| !w.expected && w.piece != "paw3955"));
    }

    #[test]
    fn a_z11s_stages_read_right_only_by_its_table() {
        // Mayank_j's Z11: 1,350, 800, 400 and 2,400 DPI in its PAW3311's table. Read 50 per step
        // they come out as other numbers, which is what hrtz showed him.
        let mut f = sample_image();
        for (i, dpi) in [1350u32, 800, 400, 2400].into_iter().enumerate() {
            let stage = atk_dpi::encode_stage(Method::Table, dpi).unwrap();
            f[0x0C + 4 * i..0x0C + 4 * i + 4].copy_from_slice(&stage);
        }
        let p = read(&f, Maker::Zaopin, Some(Method::Table)).unwrap();
        let s = find(&p, "stages");
        assert_eq!((s.ways[0].piece, s.ways[0].text.as_str()), ("table", "1,350 / 800 / 400 / 2,400 DPI"));
        assert!(s.ways[0].expected, "the model's own way comes first");
        let step50 = s.ways.iter().find(|w| w.piece == "step50").unwrap();
        assert_ne!(step50.value, s.ways[0].value);
    }

    #[test]
    fn a_paw3955s_stages_are_read_from_further_on_when_the_memory_goes_that_far() {
        let mut f = atk_sample_image();
        assert!(
            read(&f, Maker::Atk { receiver: true }, None).unwrap().settings[3]
                .ways
                .iter()
                .all(|w| w.piece != "paw3955")
        );
        f.resize(atk_dpi::PAW3955_STAGES + 6 * 8, 0xFF);
        for (i, dpi) in [450u32, 900, 1800, 3600].into_iter().enumerate() {
            let stage = atk_dpi::encode_stage(Method::Paw3955, dpi).unwrap();
            let a = atk_dpi::PAW3955_STAGES + 6 * i;
            f[a..a + 6].copy_from_slice(&stage);
        }
        let p = read(&f, Maker::Atk { receiver: true }, Some(Method::Paw3955)).unwrap();
        let s = find(&p, "stages");
        assert_eq!((s.ways[0].piece, s.ways[0].text.as_str()), ("paw3955", "450 / 900 / 1,800 / 3,600 DPI"));
        assert_eq!(s.ways[0].at.as_deref(), Some("0x1B00"));
        assert!(s.ways.iter().any(|w| w.piece == "step50"), "what's at 0x0C is still read");
    }

    #[test]
    fn an_atks_sleep_time_is_read_at_its_own_place_first() {
        let p = read(&atk_sample_image(), Maker::Atk { receiver: true }, None).unwrap();
        let s = find(&p, "sleep");
        assert_eq!((s.at.as_str(), s.ways[0].piece, s.ways[0].text.as_str()), ("0x00AD", "atk", "After 2 min"));
        // Lift-off code 1: 1 mm of three, or 0.7 mm of a finer scale's eleven.
        assert_eq!(texts(find(&p, "lod")), ["1 mm", "0.7 mm"]);
    }

    #[test]
    fn memory_kept_another_way_says_so() {
        // Nothing where hrtz looks: every field's check byte is off.
        let f = vec![0u8; NEEDS];
        let p = read(&f, Maker::Zaopin, None).unwrap();
        assert_eq!(p.kept, 0);
        assert!(p.settings.iter().all(|s| s.ways.is_empty()), "{p:#?}");
        // A value no way holds is kept, and not read.
        let mut f = sample_image();
        f[0] = 0x77;
        f[1] = checksum(&[0x77]);
        let p = read(&f, Maker::Zaopin, None).unwrap();
        let polling = find(&p, "polling");
        assert!(polling.kept && polling.ways.is_empty());
        // Too little memory to look at.
        assert!(read(&f[..0x100], Maker::Zaopin, None).is_none());
    }

    #[test]
    fn numbers_are_grouped_as_the_app_shows_them() {
        assert_eq!(
            [grouped(50), grouped(1000), grouped(26000), grouped(1234567)],
            ["50", "1,000", "26,000", "1,234,567"]
        );
        assert_eq!([seconds(20), seconds(60), seconds(90), seconds(1800)], ["20 s", "1 min", "90 s", "30 min"]);
    }
}
