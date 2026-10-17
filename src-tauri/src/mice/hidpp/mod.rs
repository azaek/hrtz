//! Logitech's HID++ 2.0, ported from OpenMouse (PORTED.md): `protocol` is the wire format, `onboard`
//! the profiles the mouse keeps its settings in, `models` the mice hrtz claims, and the driver runs
//! a connected one.

pub(crate) mod driver;
#[cfg(any(test, debug_assertions))]
pub(crate) mod fixtures;
pub mod models;
pub mod onboard;
pub mod protocol;
#[cfg(any(test, debug_assertions))]
pub(crate) mod sim;
#[cfg(test)]
mod tests;

pub use driver::{HidppFinder, Logitech};
