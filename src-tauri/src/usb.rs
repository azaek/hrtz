//! Where the receiver is plugged in, from Windows' device tree. The same port always reads the
//! same, so the connection test can tell when the receiver moved, and a hub in between shows.

use serde::{Deserialize, Serialize};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_LOCATE_DEVNODE_NORMAL,
    CM_Locate_DevNodeW, CONFIGRET, CR_SUCCESS,
};
use windows::Win32::Devices::Properties::{DEVPKEY_Device_InstanceId, DEVPKEY_Device_LocationPaths, DEVPROPTYPE};
use windows::core::PCWSTR;

/// Where the receiver, or a mouse on its cable, is plugged in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsbPort {
    /// Windows' location path, such as `PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(11)`.
    pub path: String,
    /// Hubs between it and the PC's own port: 0 when it's plugged straight into the PC.
    pub hubs: u8,
}

/// The port of the HID device at `hid_path`, as hidapi names it.
pub fn port_of(hid_path: &str) -> Option<UsbPort> {
    let iface = wide(hid_path);
    let id = strings(|ty, buf, len| unsafe {
        CM_Get_Device_Interface_PropertyW(PCWSTR(iface.as_ptr()), &DEVPKEY_Device_InstanceId, ty, buf, len, 0)
    })
    .into_iter()
    .next()?;
    let id = wide(&id);
    let mut node = 0u32;
    if unsafe { CM_Locate_DevNodeW(&mut node, PCWSTR(id.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
        return None;
    }
    // Up from the HID collection, past the USB interface ("…#USBMI(1)"), to the device on the port.
    for _ in 0..8 {
        let paths = strings(|ty, buf, len| unsafe {
            CM_Get_DevNode_PropertyW(node, &DEVPKEY_Device_LocationPaths, ty, buf, len, 0)
        });
        if let Some(port) = port_from(&paths) {
            return Some(port);
        }
        let mut parent = 0u32;
        if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
            return None;
        }
        node = parent;
    }
    None
}

/// The port, from one device node's location paths: none from a USB interface's node. Windows can
/// give an interface a second path that reads like a device behind a hub (a LIGHTSPEED receiver's
/// first interface: `…#USB(8)#USBMI(0)` and `…#USB(8)#USB(8)`), so a node with any `#USBMI(` path
/// is passed over whole.
fn port_from(paths: &[String]) -> Option<UsbPort> {
    if paths.iter().any(|p| p.contains("#USBMI(")) {
        return None;
    }
    let path = paths.iter().find(|p| p.starts_with("PCIROOT") && p.contains("#USB("))?.clone();
    let hubs = path.matches("#USB(").count().saturating_sub(1).min(u8::MAX as usize) as u8;
    Some(UsbPort { path, hubs })
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A string or string-list property, read by `get(type, buffer, size)`: asked for its size first.
fn strings(get: impl Fn(*mut DEVPROPTYPE, Option<*mut u8>, *mut u32) -> CONFIGRET) -> Vec<String> {
    let mut ty = DEVPROPTYPE::default();
    let mut len = 0u32;
    let _ = get(&mut ty, None, &mut len);
    if len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2)];
    if get(&mut ty, Some(buf.as_mut_ptr().cast()), &mut len) != CR_SUCCESS {
        return Vec::new();
    }
    buf.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

#[cfg(test)]
mod tests {
    use super::{UsbPort, port_from};

    #[test]
    fn an_interfaces_node_is_passed_over_for_its_devices() {
        let strs = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // A LIGHTSPEED receiver's mouse interface, as Windows names it on this PC.
        let interface = strs(&[
            "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(8)#USBMI(0)",
            "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(8)#USB(8)",
            "ACPI(_SB_)#ACPI(PC00)#ACPI(XHCI)#ACPI(RHUB)#ACPI(HS08)#USBMI(0)",
        ]);
        assert_eq!(port_from(&interface), None);
        let device =
            strs(&["PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(8)", "ACPI(_SB_)#ACPI(PC00)#ACPI(XHCI)#ACPI(RHUB)#ACPI(HS08)"]);
        assert_eq!(
            port_from(&device),
            Some(UsbPort { path: "PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(8)".into(), hubs: 0 })
        );
        let behind_a_hub = strs(&["PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(10)#USB(1)"]);
        assert_eq!(port_from(&behind_a_hub).map(|p| p.hubs), Some(1));
        assert_eq!(port_from(&strs(&["ACPI(_SB_)#ACPI(PC00)"])), None);
    }

    /// What the device tree gives for a HID collection named in `HID_INSTANCE`, such as
    /// `HID\VID_3554&PID_F527&MI_01&COL01\7&8D75429&0&0000` (Device Manager, Details, Device instance
    /// path). Reads Windows' records only; nothing goes to the device.
    #[test]
    #[ignore]
    fn this_pcs_receiver() {
        let id = std::env::var("HID_INSTANCE").expect("HID_INSTANCE");
        let path = format!(r"\\?\{}#{{4d1e55b2-f16f-11cf-88cb-001111000030}}", id.replace('\\', "#"));
        println!("{path}\n  {:?}", super::port_of(&path));
    }

    /// Each node up from a HID interface (`HID_PATH`), with its location paths: why two interfaces
    /// of one receiver can read as different ports. Windows' records only.
    #[test]
    #[ignore]
    fn walk_up() {
        use super::*;
        let path = std::env::var("HID_PATH").expect("HID_PATH");
        let iface = wide(&path);
        let id = strings(|ty, buf, len| unsafe {
            CM_Get_Device_Interface_PropertyW(PCWSTR(iface.as_ptr()), &DEVPKEY_Device_InstanceId, ty, buf, len, 0)
        });
        let id = wide(&id[0]);
        let mut node = 0u32;
        assert_eq!(unsafe { CM_Locate_DevNodeW(&mut node, PCWSTR(id.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) }, CR_SUCCESS);
        for _ in 0..8 {
            let me = strings(|ty, buf, len| unsafe {
                CM_Get_DevNode_PropertyW(node, &DEVPKEY_Device_InstanceId, ty, buf, len, 0)
            });
            let paths = strings(|ty, buf, len| unsafe {
                CM_Get_DevNode_PropertyW(node, &DEVPKEY_Device_LocationPaths, ty, buf, len, 0)
            });
            println!(
                "{me:?}
  {paths:?}"
            );
            let mut parent = 0u32;
            if unsafe { CM_Get_Parent(&mut parent, node, 0) } != CR_SUCCESS {
                break;
            }
            node = parent;
        }
    }
}
