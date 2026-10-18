//! A Logitech mouse's backup: its onboard memory, every sector in order, after the mouse's own
//! account of how that memory is laid out (getInfo's reply). A restore checks the account matches
//! the mouse it's going onto, and writes back only the sectors that differ.

use anyhow::{Result, bail};

use super::onboard::{self as ob, Info};
use crate::mice::model::{DpiStage, Settings};

/// getInfo's reply, as kept at the front of a backup (zero-padded).
pub const INFO_LEN: usize = 16;

/// The light a stage shows: the Superlights have none to set.
const NO_LIGHT: &str = "#FFFFFF";

/// A backup taken apart: the account, and the sectors it lays out.
pub struct Image<'a> {
    pub head: &'a [u8],
    pub info: Info,
    pub sectors: Vec<&'a [u8]>,
}

pub fn split(image: &[u8]) -> Result<Image<'_>> {
    if image.len() < INFO_LEN {
        bail!("This backup is cut short.");
    }
    let (head, rest) = image.split_at(INFO_LEN);
    let info = Info::parse(head);
    let len = info.sector_len();
    if info.format == 0 || info.sector_count == 0 || rest.len() != info.sector_count as usize * len {
        bail!("This backup doesn't hold what its own layout says.");
    }
    Ok(Image { head, info, sectors: rest.chunks(len).collect() })
}

/// Erased flash: a sector never written, or given up.
pub fn erased(sector: &[u8]) -> bool {
    sector.iter().all(|b| *b == 0xFF)
}

/// What a backup's profile `profile` (its place in the directory; the first when not known) held:
/// its DPI stages and polling rate, as the mouse's pages show them.
pub fn settings(image: &[u8], profile: Option<u8>, wired: bool) -> Result<Settings> {
    let img = split(image)?;
    let format = img.info.format;
    let directory = ob::directory(img.sectors[0]);
    let entry = profile.and_then(|p| directory.get(p as usize)).or(directory.first());
    let Some(bytes) = entry.and_then(|e| img.sectors.get(e.sector as usize)) else {
        bail!("This backup holds no profile.");
    };
    let p = ob::decode(bytes, format);
    Ok(Settings {
        polling_hz: if wired && ob::v6(format) { p.rate_wired } else { p.rate_wireless },
        dpi_stage_count: Some(p.stages.len() as u8),
        stages: Some(p.stages.iter().map(|s| DpiStage { dpi: s.x, color: NO_LIGHT.into() }).collect()),
        ..Settings::default()
    })
}
