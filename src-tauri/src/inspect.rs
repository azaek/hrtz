//! Inspect a mouse: what a mouse hrtz doesn't support yet tells Windows about itself, saved as a
//! report its owner can attach to a request. Read from Windows' own records of the device (its IDs,
//! names, and each interface's usages and report descriptor): nothing is sent to the mouse. Serial
//! numbers are left out.

use std::collections::BTreeMap;

use hidapi::{DeviceInfo, HidApi};
use serde::Serialize;
use serde_json::{Value, json};

use crate::mice::DeviceState;

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
    /// What's saved for it: the device part of the report.
    pub device: Value,
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
            // The connection hrtz runs it on, before one it left alone (an ATK's second channel).
            let mine: Vec<&DeviceState> = ifaces
                .iter()
                .filter_map(|d| known.iter().find(|(id, _)| *id == d.path().to_string_lossy()).map(|(_, s)| s))
                .collect();
            let found = mine.iter().find(|s| !s.foreign).or(mine.first()).copied();
            let supported = found.filter(|s| !s.foreign).and_then(|s| s.model.clone());
            let key = found.filter(|s| !s.foreign).and_then(|s| s.key.clone());
            let hrtz = found.map(|s| {
                if s.foreign {
                    json!({ "answeredAs": { "cid": s.cid, "mid": s.mid, "deviceType": s.device_type } })
                } else {
                    json!({ "model": s.model, "key": s.key })
                }
            });
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
            Inspected { name, vendor_id: vid, product_id: pid, supported, key, device }
        })
        .collect();
    // A dev build's simulated mice, so the check can be tried on them.
    #[cfg(debug_assertions)]
    out.extend(known.iter().filter(|(id, s)| id.starts_with("sim:") && s.connected && !s.foreign).map(|(_, s)| {
        let name = format!("{} (simulated)", s.model.as_deref().unwrap_or("Mouse"));
        Inspected {
            name: name.clone(),
            vendor_id: 0,
            product_id: 0,
            supported: s.model.clone(),
            key: s.key.clone(),
            device: json!({ "name": name, "simulated": true, "hrtz": { "model": s.model, "key": s.key } }),
        }
    }));
    out.sort_by_key(|m| m.supported.is_some());
    out
}

/// The whole report for one device, as saved.
pub fn report(device: &Value, hrtz: &str, windows: &str) -> Value {
    json!({
        "kind": "hrtz mouse inspection",
        "format": 1,
        "hrtz": hrtz,
        "windows": windows,
        "created": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "device": device,
    })
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

    #[test]
    fn a_report_says_what_it_is_and_carries_the_device() {
        let device = json!({ "name": "U2 Plus", "vendorId": "0x373B" });
        let r = report(&device, "0.4.0", "Windows 11 26300");
        assert_eq!(r["kind"], "hrtz mouse inspection");
        assert_eq!(r["device"]["vendorId"], "0x373B");
        assert_eq!(r["hrtz"], "0.4.0");
    }
}
