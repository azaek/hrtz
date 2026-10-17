//! CompX's vendor channel, as Zaopin's mice speak it: 17-byte reports on report 8, settings in a
//! flash image. `protocol` is the wire format, `image` the settings in memory, `models` what each
//! model can do, and the driver runs a connected mouse.

mod driver;
pub mod image;
pub mod models;
pub mod protocol;
#[cfg(any(test, debug_assertions))]
pub(crate) mod sim;
#[cfg(test)]
mod tests;

pub use driver::{Compx, HidFinder};
