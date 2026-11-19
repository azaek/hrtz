//! A Logitech mouse's backup starts with the mouse's own memory descriptor (getInfo's reply).
//! Normally every sector follows in order. The G304/G305 adds a marker and a byte that says
//! which sectors could be read, so a refusal never stands in for erased bytes.

use std::borrow::Cow;

use anyhow::{Result, bail};

use super::onboard::{self as ob, Info};
use crate::mice::model::{DpiStage, Settings};

/// getInfo's reply, as kept at the front of a backup (zero-padded).
pub const INFO_LEN: usize = 16;
pub const G304_MARKER: &[u8; 4] = b"G304";
pub const G304_MASK_AT: usize = INFO_LEN + G304_MARKER.len();

/// The light a stage shows: the Superlights have none to set.
const NO_LIGHT: &str = "#FFFFFF";

/// A backup taken apart: the account, and the sectors it lays out.
pub struct Image<'a> {
    pub head: &'a [u8],
    pub info: Info,
    pub sectors: Vec<Option<&'a [u8]>>,
    pub g304: bool,
}

pub fn g304_layout(info: Info) -> bool {
    info == Info {
        memory_model: 1,
        format: 3,
        macro_format: 1,
        profile_count: 1,
        rom_count: 1,
        button_count: 6,
        sector_count: 6,
        sector_size: 255,
    }
}

pub fn split(image: &[u8]) -> Result<Image<'_>> {
    if image.len() < INFO_LEN {
        bail!("This backup is cut short.");
    }
    let (head, rest) = image.split_at(INFO_LEN);
    let info = Info::parse(head);
    let len = info.sector_len();
    let count = info.sector_count as usize;
    // Prefer the existing dense format at its exact length, even if sector 0 starts with the marker.
    if info.format != 0 && count != 0 && rest.len() == count * len {
        return Ok(Image { head, info, sectors: rest.chunks_exact(len).map(Some).collect(), g304: false });
    }
    if g304_layout(info)
        && let Some((&mask, pages)) = rest.strip_prefix(G304_MARKER).and_then(<[u8]>::split_first)
        && mask & 0b1100_0011 == 0b0000_0011
        && pages.len() == mask.count_ones() as usize * len
    {
        let mut pages = pages.chunks_exact(len);
        let sectors = (0..count).map(|sector| if mask & (1 << sector) != 0 { pages.next() } else { None }).collect();
        return Ok(Image { head, info, sectors, g304: true });
    }
    bail!("This backup doesn't hold what its own layout says.");
}

/// Older macro data can have an unset checksum field, even when it holds actual macros.
/// Keep those bytes in the backup and add the checksum only when preparing a flash write.
pub fn g304_write_bytes(sector: usize, bytes: &[u8]) -> Result<Cow<'_, [u8]>> {
    if ob::crc_ok(bytes) {
        return Ok(Cow::Borrowed(bytes));
    }
    if sector >= 2 && ob::stored_crc(bytes) == 0xFFFF {
        let mut checked = bytes.to_vec();
        ob::apply_crc(&mut checked);
        return Ok(Cow::Owned(checked));
    }
    bail!("The checksum for G304/G305 sector {sector} is invalid.");
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
    let directory = ob::directory(img.sectors[0].unwrap_or_default());
    let entry = profile.and_then(|p| directory.get(p as usize)).or(directory.first());
    let Some(bytes) = entry.and_then(|e| img.sectors.get(e.sector as usize).copied().flatten()) else {
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
