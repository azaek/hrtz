//! The mice hrtz talks to. A family is one protocol (`compx`: Zaopin's; `hidpp`: Logitech's); its
//! driver runs one connected mouse on the engine's thread (`engine`), and `model` is what the UI
//! reads and writes. PLAN-mice.md in the notes has where this is going: several mice, of several
//! families.

pub mod compx;
pub mod engine;
pub mod hidpp;
pub mod hub;
pub mod input;
pub mod model;

use std::time::Duration;

use anyhow::{Result, bail};
use hidapi::HidDevice;

pub use engine::{Battery, Board, DeviceEvent, DeviceState, Engine, Notify, Worker};
pub use hub::Hub;
use model::Settings;

/// A mouse's family, from its key: `compx/7-4` a Zaopin, `hidpp/c094/…` a Logitech. A simulated
/// one (`sim/7-4`, `sim/c094/…`) is told by the shape of the rest.
pub fn family_of(key: &str) -> Option<&'static str> {
    if key.starts_with("compx/") {
        return Some("compx");
    }
    if key.starts_with("hidpp/") {
        return Some("hidpp");
    }
    let rest = key.strip_prefix("sim/")?;
    if rest.split_once('-').is_some_and(|(c, m)| c.parse::<u8>().is_ok() && m.parse::<u8>().is_ok()) {
        return Some("compx");
    }
    let usb = rest.split_once('/')?.0;
    (usb.len() == 4 && usb.chars().all(|c| c.is_ascii_hexdigit())).then_some("hidpp")
}

#[cfg(test)]
mod family_tests {
    use super::family_of;

    #[test]
    fn a_keys_family_is_told_by_its_shape() {
        assert_eq!(family_of("compx/7-4"), Some("compx"));
        assert_eq!(family_of("sim/7-3"), Some("compx"));
        assert_eq!(family_of("hidpp/c094/43c2f6b0"), Some("hidpp"));
        assert_eq!(family_of("sim/c09b/5e6f7081"), Some("hidpp"));
        assert_eq!(family_of("sim/what"), None);
        assert_eq!(family_of(""), None);
    }
}

/// A finder not asking yet, on purpose: a mouse that didn't answer is left alone until it's used
/// (asking a sleeping one can wake it). The reason it didn't answer was logged already.
#[derive(Debug)]
pub struct Waiting;

impl std::fmt::Display for Waiting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The mouse is asleep. Move it to wake it.")
    }
}

impl std::error::Error for Waiting {}

/// One open connection to a receiver, or to a mouse on its cable: reports out, reports in.
pub trait Link: Send {
    fn write(&mut self, report: &[u8]) -> Result<()>;
    /// Waits up to `timeout` for a report; `Ok(0)` when none came.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize>;
}

impl Link for HidDevice {
    fn write(&mut self, report: &[u8]) -> Result<()> {
        HidDevice::write(self, report)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        Ok(self.read_timeout(buf, timeout.as_millis().max(1) as i32)?)
    }
}

/// A protocol family: which connections plugged in now are its, and a finder for each.
pub trait Family: Send + Sync {
    /// Its connections plugged in now, each by an id that stays the same while it's plugged in
    /// (for HID, the path of the collection hrtz talks on).
    fn connections(&self, devices: &[hidapi::DeviceInfo]) -> Vec<String>;
    /// The finder for connection `id`, for the engine that runs it.
    fn finder(&self, id: &str) -> Box<dyn Finder>;
}

/// Finds a mouse's connection, the receiver or the mouse on its cable, and opens it.
pub trait Finder: Send {
    /// Open it and meet the mouse, posting what's learnt on `board`.
    fn find(&mut self, board: &Board) -> Result<Box<dyn Driver>>;
    /// How many are plugged in now: pairing wants exactly one.
    fn count(&mut self) -> usize;
}

/// What the battery says, before the engine's smoothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub level: u8,
    pub charging: bool,
    /// 0 when the mouse doesn't say.
    pub millivolts: u16,
    /// The level is to come from the millivolts, by the vendor curve the estimate knows (Zaopin's);
    /// otherwise `level` is the mouse's own, or its driver's.
    pub by_voltage: bool,
}

/// What the engine does after a driver took in what the mouse reported by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Follow {
    /// Read everything: the mouse changed before its settings were read.
    pub load: bool,
    /// The settings changed: the battery log notes the polling rate.
    pub settings: bool,
    /// Read the battery again.
    pub battery: bool,
}

/// One connected mouse, run by its family's protocol on the engine's thread, one call at a time.
/// A driver posts what it reads on the board; the engine does what's the same for every mouse:
/// finding it, the battery estimate and log, and telling the UI.
pub trait Driver: Send {
    /// The connection failed, or must be made again (another mouse paired): the engine lets go.
    fn lost(&self) -> bool;
    /// The mouse is awake and linked. Cheap: asked every second or so.
    fn online(&mut self) -> Result<bool>;
    /// Read what the UI shows: firmware, profile, settings. Not the battery.
    fn load(&mut self, board: &Board) -> Result<()>;
    fn battery(&mut self) -> Result<Reading>;
    /// Take in what the mouse reported by itself since the last call. `loaded` says whether its
    /// settings have been read since it connected or last woke.
    fn changes(&mut self, board: &Board, loaded: bool) -> Result<Follow>;
    /// Write `new`, checked against the mouse's limits, and post what the mouse holds after.
    fn apply(&mut self, board: &Board, new: &Settings) -> Result<()>;

    // The rest only where the mouse can; the defaults refuse.

    fn set_profile(&mut self, _board: &Board, _index: u8) -> Result<()> {
        unsupported()
    }

    fn set_long_range(&mut self, _board: &Board, _on: bool) -> Result<()> {
        unsupported()
    }

    /// Pair a mouse with the receiver. `connections` is how many of this family's are plugged in.
    fn pair(&mut self, _board: &Board, _connections: usize) -> Result<bool> {
        unsupported()
    }

    fn factory_reset(&mut self, _board: &Board) -> Result<()> {
        unsupported()
    }
    /// Save profile names on the mouse, where it keeps them.
    fn set_profile_names(&mut self, _board: &Board, _names: &[String]) -> Result<()> {
        unsupported()
    }

    /// All of the mouse's memory, for a backup; `progress` shows a progress bar.
    fn read_image(&mut self, _board: &Board, _progress: bool) -> Result<Vec<u8>> {
        unsupported()
    }

    /// Write `target` over memory that held `current` when just read: only what differs, each part
    /// checked after. Returns how many parts were written.
    fn write_image(&mut self, _board: &Board, _current: &[u8], _target: &[u8]) -> Result<usize> {
        unsupported()
    }

    /// Tell the mouse the app is leaving.
    fn goodbye(&mut self) {}
}

fn unsupported<T>() -> Result<T> {
    bail!("This mouse can't do that.")
}
