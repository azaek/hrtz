//! Inspect a mouse: every mouse and receiver Windows lists, with how far hrtz gets with each, and
//! what Windows itself knows of it (its IDs, names, and each interface's usages and report
//! descriptor), for the report its owner sends. Listing sends nothing to any mouse, and serial
//! numbers are left out. A mouse hrtz leaves alone that speaks a protocol it knows can be read
//! further once its owner starts its check (`mice::Worker::read_only`).

use std::collections::BTreeMap;

use hidapi::{DeviceInfo, HidApi};
use serde::Serialize;
use serde_json::{Value, json};

use crate::mice::DeviceState;

/// How far hrtz gets with a listed mouse, which decides what its check can do.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Reach {
    /// hrtz runs it: its check reads it, measures it, and on a model it knows tries a change.
    Runs,
    /// hrtz has its connection, but its mouse hasn't answered yet: asleep, or switched off.
    Asleep,
    /// hrtz leaves it alone, but it speaks a protocol hrtz knows: its check reads it and changes
    /// nothing.
    Reads,
    /// Only what Windows knows of it.
    Windows,
}

/// A mouse, or a receiver for one, as the inspector lists it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Inspected {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    /// The model hrtz runs it as, where it does.
    pub supported: Option<String>,
    /// The key of the mouse hrtz runs there, where it does, so its check can be run on it.
    pub key: Option<String>,
    pub reach: Reach,
    /// The connection to read it on, where hrtz can (`Reach::Reads`).
    pub connection: Option<String>,
    /// What's saved for it: what Windows knows of the device.
    pub device: Value,
}

/// One of a device's connections hrtz has: its id, and what hrtz found there.
type Mine<'a> = (&'a str, &'a DeviceState);

/// Which of a device's connections says how far hrtz gets with it: the one it runs, before one
/// waiting for its mouse, before one it can read, before any other (an ATK's second channel,
/// which answers nothing).
fn reach_of<'a>(mine: &[Mine<'a>]) -> (Reach, Option<Mine<'a>>) {
    let pick = |f: &dyn Fn(&DeviceState) -> bool| mine.iter().find(|(_, s)| f(s)).copied();
    // A receiver that names no mouse (CID 0, MID 0) with none linked is waiting for one, too: a
    // Z11's answered so while its mouse slept, and its report came back empty (Mayank_j,
    // 2026-11-12).
    let nameless = |s: &DeviceState| s.key.as_deref().is_some_and(|k| k.ends_with("/0-0")) && !s.online;
    if let Some(c) = pick(&|s| !s.foreign && s.connected && s.key.is_some() && !nameless(s)) {
        return (Reach::Runs, Some(c));
    }
    if let Some(c) = pick(&|s| !s.foreign && (s.waiting || (s.connected && nameless(s)))) {
        return (Reach::Asleep, Some(c));
    }
    if let Some(c) = pick(&|s| s.foreign && s.speaks) {
        return (Reach::Reads, Some(c));
    }
    (Reach::Windows, pick(&|s| !s.foreign).or(mine.first().copied()))
}

/// What hrtz itself found on the connection, for the report.
fn found_by_hrtz(reach: Reach, s: &DeviceState) -> Value {
    match reach {
        Reach::Asleep => json!({ "waiting": true }),
        _ if s.foreign => json!({ "answeredAs": { "cid": s.cid, "mid": s.mid, "deviceType": s.device_type } }),
        _ => json!({ "model": s.model, "key": s.key }),
    }
}

/// Every mouse or mouse receiver Windows lists, the ones hrtz doesn't run first. `known` is each
/// connection hrtz has (its id, the HID path it opened, and what it found there).
pub fn inspect(known: &[(String, DeviceState)]) -> Vec<Inspected> {
    let Ok(api) = HidApi::new() else { return Vec::new() };
    // A device is its interfaces: grouped by its IDs and how it's connected.
    let mut devices: BTreeMap<(u16, u16, String), Vec<&DeviceInfo>> = BTreeMap::new();
    for d in api.device_list() {
        devices.entry((d.vendor_id(), d.product_id(), format!("{:?}", d.bus_type()))).or_default().push(d);
    }
    let mut out: Vec<Inspected> = devices
        .into_iter()
        // A mouse, or a receiver with one: it has a pointer's interface (generic desktop, mouse).
        .filter(|(_, ifaces)| ifaces.iter().any(|d| d.usage_page() == 0x01 && d.usage() == 0x02))
        .map(|((vid, pid, bus), mut ifaces)| {
            ifaces.sort_by_key(|d| (d.interface_number(), collection(d), d.usage_page(), d.usage()));
            let first = ifaces[0];
            let name = first.product_string().filter(|s| !s.trim().is_empty()).unwrap_or("Unnamed device").to_string();
            let mine: Vec<Mine> = ifaces
                .iter()
                .filter_map(|d| {
                    known.iter().find(|(id, _)| *id == d.path().to_string_lossy()).map(|(id, s)| (id.as_str(), s))
                })
                .collect();
            let (reach, at) = reach_of(&mine);
            let found = at.map(|(_, s)| s);
            let supported = found.filter(|s| !s.foreign).and_then(|s| s.model.clone());
            let key = found.filter(|s| !s.foreign).and_then(|s| s.key.clone());
            let connection = at.filter(|_| reach == Reach::Reads).map(|(id, _)| id.to_string());
            let hrtz = found.map(|s| found_by_hrtz(reach, s));
            let interfaces: Vec<Value> = ifaces
                .iter()
                .map(|d| {
                    json!({
                        "interface": d.interface_number(),
                        "collection": collection(d),
                        "usagePage": format!("0x{:04X}", d.usage_page()),
                        "usage": format!("0x{:04X}", d.usage()),
                        "reportDescriptor": descriptor(&api, d),
                    })
                })
                .collect();
            let device = json!({
                "name": name,
                "manufacturer": first.manufacturer_string(),
                "vendorId": format!("0x{vid:04X}"),
                "productId": format!("0x{pid:04X}"),
                "release": format!("0x{:04X}", first.release_number()),
                "bus": bus.to_lowercase(),
                "hrtz": hrtz,
                "interfaces": interfaces,
            });
            Inspected { name, vendor_id: vid, product_id: pid, supported, key, reach, connection, device }
        })
        .collect();
    // A dev build's simulated mice, so each kind of check can be tried on them.
    #[cfg(debug_assertions)]
    out.extend(known.iter().filter(|(id, _)| id.starts_with("sim:")).filter_map(|(id, s)| {
        let (reach, _) = reach_of(&[(id.as_str(), s)]);
        let what = match reach {
            Reach::Windows => return None,
            Reach::Reads => "Another maker's mouse",
            _ => s.model.as_deref().unwrap_or("Mouse"),
        };
        let name = format!("{what} (simulated)");
        Some(Inspected {
            name: name.clone(),
            vendor_id: 0,
            product_id: 0,
            supported: s.model.clone().filter(|_| !s.foreign),
            key: s.key.clone().filter(|_| !s.foreign),
            reach,
            connection: (reach == Reach::Reads).then(|| id.clone()),
            device: json!({ "name": name, "simulated": true, "hrtz": found_by_hrtz(reach, s) }),
        })
    }));
    out.sort_by_key(|m| m.supported.is_some());
    out
}

/// Which top-level collection of its interface this is, from Windows' path (`…&col02#…`).
fn collection(d: &DeviceInfo) -> Option<u8> {
    let path = d.path().to_string_lossy().to_lowercase();
    let at = path.find("&col")? + 4;
    u8::from_str_radix(path.get(at..at + 2)?, 16).ok()
}

/// The interface's report descriptor in hex, as Windows rebuilds it from its own record. Opening an
/// interface to ask sends nothing to the device.
fn descriptor(api: &HidApi, d: &DeviceInfo) -> Option<String> {
    let dev = d.open_device(api).ok()?;
    let mut buf = [0u8; 4096];
    let n = dev.get_report_descriptor(&mut buf).ok()?;
    Some(buf[..n].iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the inspector finds on this PC. Read-only: run with `--ignored --nocapture`.
    #[test]
    #[ignore = "reads this PC's devices"]
    fn dump_this_pc() {
        for m in inspect(&[]) {
            println!("{}", serde_json::to_string_pretty(&m.device).unwrap());
        }
    }

    fn run() -> DeviceState {
        DeviceState { connected: true, key: Some("compx/2-78".into()), ..Default::default() }
    }

    fn left_alone(speaks: bool) -> DeviceState {
        DeviceState { foreign: true, speaks, cid: 1, mid: 86, ..Default::default() }
    }

    #[test]
    fn the_connection_hrtz_runs_counts_before_its_other_channel() {
        // An ATK's receiver: the channel it runs, and the 19-byte one it leaves alone.
        let (other, mine) = (left_alone(false), run());
        let (reach, at) = reach_of(&[("col04", &other), ("col05", &mine)]);
        assert_eq!((reach, at.map(|c| c.0)), (Reach::Runs, Some("col05")));
    }

    #[test]
    fn a_mouse_left_alone_that_answered_is_read_on_the_channel_that_answered() {
        let (other, spoke) = (left_alone(false), left_alone(true));
        let (reach, at) = reach_of(&[("col04", &other), ("col05", &spoke)]);
        assert_eq!((reach, at.map(|c| c.0)), (Reach::Reads, Some("col05")));
        assert_eq!(found_by_hrtz(reach, at.unwrap().1)["answeredAs"]["mid"], 86);
    }

    #[test]
    fn a_receiver_waiting_for_its_mouse_says_so() {
        let (other, waiting) = (left_alone(false), DeviceState { waiting: true, ..Default::default() });
        let (reach, at) = reach_of(&[("col04", &other), ("col05", &waiting)]);
        assert_eq!((reach, at.map(|c| c.0)), (Reach::Asleep, Some("col05")));
        assert_eq!(found_by_hrtz(reach, at.unwrap().1), json!({ "waiting": true }));
    }

    #[test]
    fn a_receiver_that_names_no_mouse_waits_for_one_until_it_wakes() {
        let mut receiver = DeviceState { connected: true, key: Some("compx/0-0".into()), ..Default::default() };
        assert_eq!(reach_of(&[("col01", &receiver)]).0, Reach::Asleep);
        // Awake and still nameless, it's a mouse hrtz runs without knowing the model.
        receiver.online = true;
        assert_eq!(reach_of(&[("col01", &receiver)]).0, Reach::Runs);
        // A named mouse that sleeps is still one hrtz runs: its own check waits for it.
        let asleep = DeviceState { connected: true, key: Some("compx/7-9".into()), ..Default::default() };
        assert_eq!(reach_of(&[("col01", &asleep)]).0, Reach::Runs);
    }

    #[test]
    fn a_mouse_hrtz_has_no_connection_for_is_only_what_windows_knows() {
        assert_eq!(reach_of(&[]).0, Reach::Windows);
        // One it left alone that never answered as speaking its protocol, too.
        let other = left_alone(false);
        assert_eq!(reach_of(&[("col04", &other)]).0, Reach::Windows);
    }
}
