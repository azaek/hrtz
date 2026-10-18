//! A read-only record of what a Logitech mouse says, for its first session on hardware and as test
//! fixtures (PLAN-mice.md, section 6). It only asks: feature lookups, and reads of identity, name,
//! firmware, battery, DPI, report rates and the onboard memory (every sector, and the factory
//! profiles from 0x0101). It changes no setting, selects no profile and writes nothing to the
//! mouse's memory.
//!
//! With the mouse on its receiver or cable, and G HUB closed (its traffic is recorded too, but it
//! can switch the mouse to host mode):
//! `cargo test --lib hidpp::probe -- --ignored --nocapture`
//! Writes `hrtz-notes/captures/logitech-<pid>-<time>.json`, which holds the mouse's unit ID (the
//! notes are private).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hidapi::HidApi;
use parking_lot::{Mutex, RwLock};
use serde_json::{Value, json};

use super::driver::{HidppLink, interface_of, is_hidpp};
use super::onboard::{self as ob, func};
use super::protocol::{self as p, feature};
use super::{Logitech, models};
use crate::battery_log::BatteryLog;
use crate::mice::{DeviceState, Family, Link, Notify, Worker};

const WAIT: Duration = Duration::from_millis(1000);
const PROBE_WAIT: Duration = Duration::from_millis(500);
const LISTEN: Duration = Duration::from_secs(10);

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// Asks, and keeps every request with everything heard until its answer.
struct Asker {
    link: HidppLink,
    log: Vec<Value>,
}

impl Asker {
    fn ask(&mut self, what: &str, index: u8, fi: u8, function: u8, params: &[u8], wait: Duration) -> Option<Vec<u8>> {
        let request = p::request(index, fi, function, params, false);
        if let Err(e) = self.link.write(&request) {
            self.log.push(json!({ "what": what, "request": hex(&request), "outcome": format!("write failed: {e}") }));
            return None;
        }
        let (mut heard, mut outcome, mut answer) = (Vec::new(), "silent".to_string(), None);
        let deadline = Instant::now() + wait;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let mut buf = [0u8; p::LONG_LEN + 1];
            let n = match self.link.read(&mut buf, left) {
                Ok(0) => break,
                Ok(n) => n.min(p::LONG_LEN),
                Err(e) => {
                    outcome = format!("read failed: {e}");
                    break;
                }
            };
            let r = &buf[..n];
            heard.push(hex(r));
            if p::answers(r, index, fi, function) {
                outcome = "answer".into();
                answer = Some(r[4..].to_vec());
                break;
            }
            if let Some(why) = p::refusal_for(r, index, fi, function) {
                outcome = format!("{why:?}");
                break;
            }
        }
        self.log.push(json!({ "what": what, "request": hex(&request), "heard": heard, "outcome": outcome }));
        answer
    }

    /// A sector, 16 bytes at a time, the tail read from `len - 16`.
    fn sector(&mut self, index: u8, fi: u8, sector: u16, len: usize) -> Option<Vec<u8>> {
        let mut out = vec![0u8; len];
        let mut offsets: Vec<usize> = (0..len / ob::CHUNK).map(|i| i * ob::CHUNK).collect();
        if !len.is_multiple_of(ob::CHUNK) {
            offsets.push(len.saturating_sub(ob::CHUNK));
        }
        for at in offsets {
            let [s0, s1] = sector.to_be_bytes();
            let [o0, o1] = (at as u16).to_be_bytes();
            let r = self.ask(&format!("sector {sector} at {at}"), index, fi, func::READ, &[s0, s1, o0, o1], WAIT)?;
            let n = ob::CHUNK.min(len - at);
            out[at..at + n].copy_from_slice(r.get(..n)?);
        }
        Some(out)
    }
}

fn open(api: &HidApi, long: &hidapi::DeviceInfo) -> HidppLink {
    let path = long.path().to_string_lossy().into_owned();
    let short = api.device_list().find(|d| {
        is_hidpp(d, p::USAGE_SHORT)
            && d.product_id() == long.product_id()
            && interface_of(&d.path().to_string_lossy()) == interface_of(&path)
    });
    HidppLink {
        long: long.open_device(api).expect("open the long collection"),
        short: short.map(|s| s.open_device(api).expect("open the short collection")),
    }
}

/// What one receiver or cable says.
fn probe_connection(api: &HidApi, long: &hidapi::DeviceInfo) -> Value {
    let pid = long.product_id();
    let receiver = models::is_receiver(pid);
    let link = open(api, long);
    let long_only = link.short.is_none();
    let mut a = Asker { link, log: Vec::new() };
    println!("0x{pid:04X}: {}", if receiver { "a receiver" } else { "a mouse on its cable" });

    // Where it answers, and whether a sensor is there.
    let mut index = None;
    for &i in p::index_candidates(receiver) {
        let root = a.ask(&format!("index {i:#04x}: root"), i, 0, 0x00, &feature::FIRMWARE.to_be_bytes(), PROBE_WAIT);
        if root.is_none() {
            continue;
        }
        for id in [feature::EXTENDED_DPI, feature::ADJUSTABLE_DPI] {
            let at = a.ask(&format!("index {i:#04x}: {id:#06x}"), i, 0, 0x00, &id.to_be_bytes(), PROBE_WAIT);
            if index.is_none() && at.is_some_and(|r| r.first().is_some_and(|x| *x != 0)) {
                index = Some(i);
            }
        }
    }
    let Some(i) = index else {
        println!("  no mouse answered");
        return json!({ "productId": format!("{pid:04x}"), "longOnly": long_only, "mouse": null, "exchanges": a.log });
    };
    println!("  the mouse is on index {i:#04x}");

    // Every feature, from 0x0001.
    let mut features: BTreeMap<u16, u8> = BTreeMap::new();
    let mut table = Vec::new();
    if let Some(fs) =
        a.ask("feature set", i, 0, 0x00, &[0x00, 0x01], WAIT).and_then(|r| r.first().copied()).filter(|x| *x != 0)
    {
        let count = a.ask("feature count", i, fs, 0x00, &[], WAIT).and_then(|r| r.first().copied()).unwrap_or(0);
        for n in 0..=count {
            if let Some(r) = a.ask(&format!("feature {n}"), i, fs, 0x10, &[n], WAIT) {
                let id = u16::from_be_bytes([r[0], r[1]]);
                features.insert(id, n);
                table.push(json!({ "index": n, "id": format!("{id:04x}"), "type": r.get(2), "version": r.get(3) }));
            }
        }
    }
    let at = |id: u16| features.get(&id).copied();

    a.ask("ping", i, 0, 0x10, &[0, 0, 0x5A], WAIT);
    if let Some(fi) = at(feature::FIRMWARE) {
        let count = a.ask("identity", i, fi, 0x00, &[], WAIT).and_then(|r| r.first().copied()).unwrap_or(0);
        for e in 0..count.min(8) {
            a.ask(&format!("firmware {e}"), i, fi, 0x10, &[e], WAIT);
        }
    }
    if let Some(fi) = at(feature::NAME) {
        let len = a.ask("name length", i, fi, 0x00, &[], WAIT).and_then(|r| r.first().copied()).unwrap_or(0);
        for off in (0..len).step_by(16) {
            a.ask("name", i, fi, 0x10, &[off], WAIT);
        }
    }
    for (id, function, params) in [
        (feature::UNIFIED_BATTERY, 0x00, vec![]),
        (feature::UNIFIED_BATTERY, 0x10, vec![]),
        (feature::BATTERY_STATUS, 0x00, vec![]),
        (feature::BATTERY_VOLTAGE, 0x00, vec![]),
        (feature::ADC_MEASUREMENT, 0x00, vec![]),
        (feature::ADJUSTABLE_DPI, 0x00, vec![]),
        (feature::ADJUSTABLE_DPI, 0x10, vec![0]),
        (feature::ADJUSTABLE_DPI, 0x20, vec![0]),
        (feature::EXTENDED_DPI, 0x00, vec![]),
        (feature::EXTENDED_DPI, 0x10, vec![0]),
        (feature::EXTENDED_DPI, 0x50, vec![0]),
        (feature::REPORT_RATE, 0x00, vec![]),
        (feature::REPORT_RATE, 0x10, vec![]),
        (feature::EXTENDED_REPORT_RATE, 0x00, vec![0]),
        (feature::EXTENDED_REPORT_RATE, 0x00, vec![1]),
        (feature::EXTENDED_REPORT_RATE, 0x10, vec![0]),
        (feature::EXTENDED_REPORT_RATE, 0x10, vec![1]),
        (feature::EXTENDED_REPORT_RATE, 0x20, vec![0]),
        (feature::EXTENDED_REPORT_RATE, 0x20, vec![1]),
    ] {
        if let Some(fi) = at(id) {
            a.ask(&format!("{id:04x} fn {function:#04x}"), i, fi, function, &params, WAIT);
        }
    }
    if let Some(fi) = at(feature::EXTENDED_DPI) {
        let mut bytes = Vec::new();
        for page in 0..32u8 {
            let Some(r) = a.ask(&format!("dpi list page {page}"), i, fi, 0x20, &[0, 0, page], WAIT) else { break };
            bytes.extend_from_slice(r.get(3..).unwrap_or_default());
            if bytes.windows(2).any(|w| w == [0, 0]) {
                break;
            }
        }
    }

    // The onboard profiles: the directory and every profile it lists, whole.
    let mut sectors = Vec::new();
    if let Some(fi) = at(feature::ONBOARD_PROFILES)
        && let Some(info) = a.ask("profiles info", i, fi, func::INFO, &[], WAIT).map(|r| ob::Info::parse(&r))
    {
        a.ask("mode", i, fi, func::GET_MODE, &[], WAIT);
        a.ask("current profile", i, fi, func::GET_CURRENT, &[], WAIT);
        let len = info.sector_len();
        // Every sector, as a backup reads them (erased ones as 0xFF), then the factory profiles.
        let every = (0..info.sector_count as u16).chain((0..info.rom_count as u16).map(|r| ob::ROM_FIRST + r));
        for s in every {
            match a.sector(i, fi, s, len) {
                Some(b) => sectors.push(json!({
                    "sector": s,
                    "crcOk": ob::crc_ok(&b),
                    "erased": b.iter().all(|x| *x == 0xFF),
                    "bytes": hex(&b),
                })),
                None => sectors.push(json!({ "sector": s, "refused": true })),
            }
        }
        println!(
            "  profile format {}, {} sectors asked for (the factory ones from 0x0101)",
            info.format,
            sectors.len()
        );
    }

    // What the mouse sends by itself.
    println!("  For {} s: move and click the mouse. If a button changes DPI or profile, press it.", LISTEN.as_secs());
    let mut heard = Vec::new();
    let end = Instant::now() + LISTEN;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        let mut buf = [0u8; p::LONG_LEN + 1];
        match a.link.read(&mut buf, left.min(Duration::from_millis(100))) {
            Ok(0) => {}
            Ok(n) => heard.push(hex(&buf[..n.min(p::LONG_LEN)])),
            Err(_) => break,
        }
    }

    json!({
        "productId": format!("{pid:04x}"),
        "longOnly": long_only,
        "mouse": format!("{i:#04x}"),
        "features": table,
        "sectors": sectors,
        "listened": heard,
        "exchanges": a.log,
    })
}

/// The driver as hrtz runs it, on the same connection: what the pages would show.
fn as_hrtz(path: &str) -> Value {
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = seen.clone();
    let notify: Notify = Arc::new(move |s: &DeviceState, e| {
        sink.lock().push(json!({ "event": serde_json::to_value(&e).ok(), "online": s.online }));
    });
    let state = Arc::new(RwLock::new(DeviceState::default()));
    let log = Arc::new(Mutex::new(BatteryLog::load(None)));
    let mut w = Worker::new(Logitech.finder(path), state, notify, None, log);
    w.housekeeping();
    json!({ "state": w.snapshot(), "published": seen.lock().clone() })
}

#[test]
#[ignore = "talks to a Logitech mouse, read-only; run with --ignored --nocapture"]
fn probe() {
    let mut api = HidApi::new().unwrap();
    api.refresh_devices().unwrap();
    let devices: Vec<Value> = api
        .device_list()
        .filter(|d| d.vendor_id() == p::VENDOR_ID)
        .map(|d| {
            json!({
                "productId": format!("{:04x}", d.product_id()),
                "product": d.product_string(),
                "usagePage": format!("{:04x}", d.usage_page()),
                "usage": format!("{:04x}", d.usage()),
                "interface": d.interface_number(),
                "path": d.path().to_string_lossy(),
            })
        })
        .collect();
    println!("{} Logitech HID collections", devices.len());
    let longs: Vec<hidapi::DeviceInfo> = api.device_list().filter(|d| is_hidpp(d, p::USAGE_LONG)).cloned().collect();
    assert!(!longs.is_empty(), "no Superlight receiver or cable is plugged in");
    let connections: Vec<Value> = longs
        .iter()
        .map(|long| {
            let mut c = probe_connection(&api, long);
            c["asHrtz"] = as_hrtz(&long.path().to_string_lossy());
            c
        })
        .collect();

    let when = chrono::Local::now();
    let out = PathBuf::from("../hrtz-notes/captures");
    std::fs::create_dir_all(&out).unwrap();
    let pid = longs[0].product_id();
    let file = out.join(format!("logitech-{pid:04x}-{}.json", when.format("%Y%m%d-%H%M%S")));
    let record = json!({ "when": when.to_rfc3339(), "devices": devices, "connections": connections });
    std::fs::write(&file, format!("{}\n", serde_json::to_string_pretty(&record).unwrap())).unwrap();
    println!("Saved {}", file.display());
}
