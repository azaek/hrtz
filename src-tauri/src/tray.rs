//! Tray icon, in two styles: the battery percentage as digits on a dark tile, or the Windows 11
//! battery glyph (Segoe Fluent Icons "MobBattery") drawn in the taskbar's own monochrome colour.

use serde::{Deserialize, Serialize};

const SIZE: usize = 32;

// 3x5 digit glyphs, one row per u8 (low 3 bits, MSB = left column).
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 1, 1],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];
const DASH: [u8; 5] = [0, 0, 7, 0, 0];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrayStyle {
    /// Battery percentage as digits.
    #[default]
    Percent,
    /// Windows 11 battery glyph.
    Battery,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TrayLevel {
    /// No receiver plugged in.
    Disconnected,
    /// Receiver present, mouse asleep or out of range; `level` is the last known reading.
    Asleep { level: Option<u8> },
    Battery { level: u8, charging: bool, low_at: u8 },
}

pub struct Rendered {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn render(level: TrayLevel, style: TrayStyle) -> Rendered {
    render_at(level, style, None)
}

/// `glyph_size` overrides the tray icon size for the battery style (previews render larger).
pub fn render_at(level: TrayLevel, style: TrayStyle, glyph_size: Option<i32>) -> Rendered {
    match style {
        TrayStyle::Percent => percent(level),
        #[cfg(windows)]
        TrayStyle::Battery => {
            let size = glyph_size.unwrap_or_else(crate::win::small_icon_size);
            glyph(level, size).unwrap_or_else(|| percent(level))
        }
        #[cfg(not(windows))]
        TrayStyle::Battery => percent(level),
    }
}

// ---- digits style -------------------------------------------------------------------------------

struct Canvas([u8; SIZE * SIZE * 4]);

impl Canvas {
    fn px(&mut self, x: usize, y: usize, c: [u8; 4]) {
        if x < SIZE && y < SIZE {
            let i = (y * SIZE + x) * 4;
            self.0[i..i + 4].copy_from_slice(&c);
        }
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.px(xx, yy, c);
            }
        }
    }

    fn rounded_bg(&mut self, c: [u8; 4]) {
        let r = 6.0f32;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let cx = fx.clamp(r, SIZE as f32 - r);
                let cy = fy.clamp(r, SIZE as f32 - r);
                let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0);
                if a > 0.0 {
                    self.px(x, y, [c[0], c[1], c[2], (c[3] as f32 * a) as u8]);
                }
            }
        }
    }

    fn glyph(&mut self, g: &[u8; 5], x: usize, y: usize, scale: usize, c: [u8; 4]) {
        for (row, bits) in g.iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) != 0 {
                    self.rect(x + col * scale, y + row * scale, scale, scale, c);
                }
            }
        }
    }

    fn text(&mut self, glyphs: &[&[u8; 5]], y: usize, scale: usize, c: [u8; 4]) {
        let gap = scale.max(2) / 2 + 1;
        let w = glyphs.len() * 3 * scale + (glyphs.len() - 1) * gap;
        let mut x = (SIZE - w) / 2;
        for g in glyphs {
            self.glyph(g, x, y, scale, c);
            x += 3 * scale + gap;
        }
    }
}

const INK: [u8; 4] = [0x17, 0x1A, 0x19, 0xFF];
const PAPER: [u8; 4] = [0xF2, 0xF3, 0xEF, 0xFF];
const DIM: [u8; 4] = [0x7A, 0x80, 0x7C, 0xFF];
const OK: [u8; 4] = [0x4C, 0xC2, 0x8A, 0xFF];
const WARN: [u8; 4] = [0xF2, 0xB1, 0x3D, 0xFF];
const LOW: [u8; 4] = [0xF0, 0x5A, 0x4A, 0xFF];
const CHARGE: [u8; 4] = [0x6C, 0xB4, 0xF5, 0xFF];

fn percent(level: TrayLevel) -> Rendered {
    let mut c = Canvas([0; SIZE * SIZE * 4]);
    c.rounded_bg(INK);
    match level {
        TrayLevel::Disconnected => {
            c.text(&[&DASH, &DASH], 6, 4, DIM);
        }
        TrayLevel::Asleep { .. } => {
            c.text(&[&DASH, &DASH], 6, 4, PAPER);
            c.rect(4, 26, 24, 2, DIM);
        }
        TrayLevel::Battery { level, charging, low_at } => {
            let level = level.min(100);
            if level == 100 {
                c.text(&[&DIGITS[1], &DIGITS[0], &DIGITS[0]], 7, 3, PAPER);
            } else {
                let tens = (level / 10) as usize;
                let ones = (level % 10) as usize;
                let glyphs: Vec<&[u8; 5]> =
                    if tens == 0 { vec![&DIGITS[ones]] } else { vec![&DIGITS[tens], &DIGITS[ones]] };
                c.text(&glyphs, 4, 4, PAPER);
            }
            let color = if charging {
                CHARGE
            } else if level <= low_at {
                LOW
            } else if level <= low_at.saturating_mul(2) {
                WARN
            } else {
                OK
            };
            c.rect(4, 26, 24, 2, [0x33, 0x38, 0x36, 0xFF]);
            let filled = ((24 * level as usize) + 50) / 100;
            c.rect(4, 26, filled.max(1), 2, color);
        }
    }
    Rendered { width: SIZE as u32, height: SIZE as u32, rgba: c.0.to_vec() }
}

// ---- Windows 11 battery glyph style -------------------------------------------------------------

/// Segoe Fluent Icons: MobBattery0..10, MobBatteryCharging0..10.
const BATTERY: u32 = 0xEBA0;
const CHARGING: u32 = 0xEBAB;
/// Fluent "danger" red, used when the battery is at or below the alert level.
const RED: [u8; 3] = [0xD1, 0x34, 0x38];

#[cfg(windows)]
fn glyph(level: TrayLevel, size: i32) -> Option<Rendered> {
    let light = crate::win::taskbar_is_light();
    let mono = if light { [0, 0, 0] } else { [255, 255, 255] };
    let step = |l: u8| ((l.min(100) as u32 + 5) / 10).min(10);
    let (code, color, opacity) = match level {
        TrayLevel::Disconnected => (BATTERY, mono, 0.4),
        TrayLevel::Asleep { level } => (BATTERY + level.map_or(0, step), mono, 0.5),
        TrayLevel::Battery { level, charging: true, .. } => (CHARGING + step(level), mono, 1.0),
        TrayLevel::Battery { level, low_at, .. } if level <= low_at => (BATTERY + step(level), RED, 1.0),
        TrayLevel::Battery { level, .. } => (BATTERY + step(level), mono, 1.0),
    };
    let alpha = crate::win::glyph_alpha(char::from_u32(code)?, size)?;
    let mut rgba = Vec::with_capacity(alpha.len() * 4);
    for a in alpha {
        rgba.extend_from_slice(&[color[0], color[1], color[2], (a as f32 * opacity) as u8]);
    }
    Some(Rendered { width: size as u32, height: size as u32, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes both tray styles at a few battery states as PNG data URLs (JSON), for design labs.
    /// Run with `HRTZ_OUT=<json> cargo test --lib dump_previews -- --ignored`.
    #[test]
    #[cfg(windows)]
    #[ignore = "writes a file named by HRTZ_OUT"]
    fn dump_previews() {
        use base64::Engine as _;
        let states = [
            ("61", TrayLevel::Battery { level: 61, charging: false, low_at: 15 }),
            ("12", TrayLevel::Battery { level: 12, charging: false, low_at: 15 }),
            ("charging", TrayLevel::Battery { level: 52, charging: true, low_at: 15 }),
            ("asleep", TrayLevel::Asleep { level: Some(61) }),
        ];
        let mut out = serde_json::Map::new();
        for (name, level) in states {
            for (style_name, style) in [("percent", TrayStyle::Percent), ("battery", TrayStyle::Battery)] {
                let r = render_at(level, style, Some(32));
                let png = crate::win::encode_png(r.width, r.height, &r.rgba).unwrap();
                out.insert(format!("{name}-{style_name}"), format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)).into());
            }
        }
        std::fs::write(std::env::var("HRTZ_OUT").unwrap(), serde_json::Value::Object(out).to_string()).unwrap();
    }
}
