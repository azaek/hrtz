//! Tray icon, in four styles: the battery percentage as digits on a dark tile; the Windows 11
//! battery glyph (Segoe Fluent Icons "MobBattery") in the taskbar's own monochrome colour; the hrtz
//! emblem filling from the bottom like a battery; or the emblem with a battery bar under it.

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
    Percent,
    /// Windows 11 battery glyph.
    Battery,
    /// The hrtz emblem, filling from the bottom with the battery.
    Logo,
    /// The hrtz emblem, with a battery bar under it: the default since 0.4.0 (a prefs file that
    /// names a style keeps it).
    #[default]
    LogoBar,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TrayLevel {
    /// No receiver plugged in.
    Disconnected,
    /// Receiver present, mouse asleep or out of range; `level` is the last known reading.
    Asleep {
        level: Option<u8>,
    },
    Battery {
        level: u8,
        charging: bool,
        low_at: u8,
    },
}

pub struct Rendered {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub fn render(level: TrayLevel, style: TrayStyle) -> Rendered {
    render_at(level, style, None)
}

/// `glyph_size` overrides the tray icon size for the drawn styles (previews render larger).
pub fn render_at(level: TrayLevel, style: TrayStyle, glyph_size: Option<i32>) -> Rendered {
    match style {
        TrayStyle::Percent => percent(level),
        TrayStyle::Logo => logo(level, icon_size(glyph_size), taskbar_light()),
        TrayStyle::LogoBar => logo_bar(level, icon_size(glyph_size), taskbar_light()),
        #[cfg(windows)]
        TrayStyle::Battery => {
            let size = glyph_size.unwrap_or_else(crate::win::small_icon_size);
            glyph(level, size).unwrap_or_else(|| percent(level))
        }
        #[cfg(not(windows))]
        TrayStyle::Battery => percent(level),
    }
}

/// The taskbar's small icon size (16 px at 100% scaling, 24 at 150%), unless a preview asks for one.
fn icon_size(asked: Option<i32>) -> usize {
    #[cfg(windows)]
    let size = asked.unwrap_or_else(crate::win::small_icon_size);
    #[cfg(not(windows))]
    let size = asked.unwrap_or(32);
    size.clamp(12, 64) as usize
}

fn taskbar_light() -> bool {
    #[cfg(windows)]
    {
        crate::win::taskbar_is_light()
    }
    #[cfg(not(windows))]
    false
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

// ---- emblem styles ------------------------------------------------------------------------------

/// The emblem (brand.tsx, branding/emblem.svg), 226 x 247: a mouse shell split at the button seam,
/// the left button tall and the right one short, so it reads as an h.
const EMBLEM_LEFT: &str = "M0 112.091C0 59.6577 35.7123 15.565 84.1404 2.81008C95.2232 -0.108917 100.765 -1.56842 105.876 2.53566C110.987 6.63973 110.723 13.2744 110.195 26.5436L102.447 221.126C101.977 232.934 101.742 238.838 97.968 242.465C94.1942 246.091 88.2853 246.091 76.4677 246.091H29.7025C19.719 246.091 14.7272 246.091 10.8658 244.262C6.90057 242.384 3.70768 239.191 1.82925 235.226C0 231.364 0 226.372 0 216.389V112.091Z";
const EMBLEM_RIGHT: &str = "M118.629 102.064C118.082 88.7644 117.809 82.1144 122.96 78.0039C128.11 73.8934 133.629 75.3895 144.666 78.3815C192.383 91.3162 226 135.728 226 188.091V216.389C226 226.372 226 231.364 224.171 235.226C222.292 239.191 219.099 242.384 215.134 244.262C211.273 246.091 206.281 246.091 196.297 246.091H149.501C137.697 246.091 131.795 246.091 128.022 242.471C124.249 238.85 124.007 232.953 123.523 221.159L118.629 102.064Z";
const EMBLEM_W: f32 = 226.0;
const EMBLEM_H: f32 = 247.0;
/// The seam at its narrowest, at the top, in emblem units.
const SEAM: f32 = 8.4;
/// Low battery and charging, as the logo styles show them.
const LOGO_LOW: [u8; 3] = [0xE8, 0x47, 0x4B];
const LOGO_CHARGE: [u8; 3] = [0x4C, 0xA0, 0xE8];
/// The empty part of the emblem or bar, and an asleep or unplugged mouse's whole icon.
const TRACK: f32 = 0.32;
const DIMMED: f32 = 0.45;

/// A path's outline as points. Takes the absolute `M`, `L`, `H`, `V`, `C` and `Z` the emblem uses.
fn outline(d: &str) -> Outline {
    let mut tokens: Vec<Result<f32, char>> = Vec::new();
    let mut chars = d.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() {
            tokens.push(Err(c));
            chars.next();
        } else if c == '-' || c == '.' || c.is_ascii_digit() {
            let mut n = String::from(c);
            chars.next();
            while let Some(&c) = chars.peek().filter(|c| c.is_ascii_digit() || **c == '.') {
                n.push(c);
                chars.next();
            }
            tokens.push(Ok(n.parse().unwrap_or(0.0)));
        } else {
            chars.next();
        }
    }
    let num = |i: usize| tokens.get(i).and_then(|t| t.ok()).unwrap_or(0.0);
    let mut pts = Vec::new();
    let (mut cur, mut i, mut cmd) = ((0.0f32, 0.0f32), 0, ' ');
    while i < tokens.len() {
        if let Err(c) = tokens[i] {
            cmd = c;
            i += 1;
            if c == 'Z' {
                continue;
            }
        }
        match cmd {
            'M' | 'L' => {
                cur = (num(i), num(i + 1));
                pts.push(cur);
                i += 2;
            }
            'H' => {
                cur.0 = num(i);
                pts.push(cur);
                i += 1;
            }
            'V' => {
                cur.1 = num(i);
                pts.push(cur);
                i += 1;
            }
            'C' => {
                let (p1, p2, p3) = ((num(i), num(i + 1)), (num(i + 2), num(i + 3)), (num(i + 4), num(i + 5)));
                for k in 1..=12 {
                    let t = k as f32 / 12.0;
                    let u = 1.0 - t;
                    let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    pts.push((a * cur.0 + b * p1.0 + c * p2.0 + e * p3.0, a * cur.1 + b * p1.1 + c * p2.1 + e * p3.1));
                }
                cur = p3;
                i += 6;
            }
            _ => i += 1,
        }
    }
    pts
}

/// A closed outline, as points.
type Outline = Vec<(f32, f32)>;

fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// How much of each pixel of a `size` square the emblem covers, drawn `h` px tall from `top` and
/// centred across, as (above `cut`, below it), `cut` being a y in px. Below 32 px the halves are
/// pushed apart so the seam stays about a pixel wide, as brand.tsx does.
fn emblem_cover(size: usize, h: f32, top: f32, cut: f32) -> Vec<(f32, f32)> {
    static SHAPES: std::sync::OnceLock<(Outline, Outline)> = std::sync::OnceLock::new();
    let (left, right) = SHAPES.get_or_init(|| (outline(EMBLEM_LEFT), outline(EMBLEM_RIGHT)));
    let spread = if h > 32.0 { 0.0 } else { ((1.1 * EMBLEM_H / h - SEAM) / 2.0).max(0.0) };
    let scale = h / EMBLEM_H;
    let x0 = (size as f32 - (EMBLEM_W + 2.0 * spread) * scale) / 2.0;
    const N: usize = 4;
    let mut out = vec![(0.0, 0.0); size * size];
    for py in 0..size {
        for px in 0..size {
            let (mut above, mut below) = (0, 0);
            for sy in 0..N {
                let y = py as f32 + (sy as f32 + 0.5) / N as f32;
                let ey = (y - top) / scale;
                if !(0.0..=EMBLEM_H).contains(&ey) {
                    continue;
                }
                for sx in 0..N {
                    let ex = (px as f32 + (sx as f32 + 0.5) / N as f32 - x0) / scale;
                    if inside(left, ex, ey) || inside(right, ex - 2.0 * spread, ey) {
                        if y < cut {
                            above += 1;
                        } else {
                            below += 1;
                        }
                    }
                }
            }
            let n = (N * N) as f32;
            out[py * size + px] = (above as f32 / n, below as f32 / n);
        }
    }
    out
}

/// Lay colour `c` at `alpha` over pixel `i` of `rgba`.
fn over(rgba: &mut [u8], i: usize, c: [u8; 3], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let px = &mut rgba[i * 4..i * 4 + 4];
    let below = px[3] as f32 / 255.0;
    let a = alpha + below * (1.0 - alpha);
    for k in 0..3 {
        px[k] = ((c[k] as f32 * alpha + px[k] as f32 * below * (1.0 - alpha)) / a).round() as u8;
    }
    px[3] = (a * 255.0).round() as u8;
}

/// What the logo styles show: how full (0 to 1), in what colour, and how strongly.
fn logo_state(level: TrayLevel, mono: [u8; 3]) -> (f32, [u8; 3], f32) {
    match level {
        TrayLevel::Disconnected => (0.0, mono, DIMMED),
        TrayLevel::Asleep { level } => (level.map_or(0.0, |l| l.min(100) as f32 / 100.0), mono, DIMMED),
        TrayLevel::Battery { level, charging, low_at } => {
            let tone = if charging {
                LOGO_CHARGE
            } else if level <= low_at {
                LOGO_LOW
            } else {
                mono
            };
            (level.min(100) as f32 / 100.0, tone, 1.0)
        }
    }
}

/// The emblem, filling from the bottom with the battery; the empty part faint.
fn logo(level: TrayLevel, size: usize, light: bool) -> Rendered {
    let mono = if light { [0, 0, 0] } else { [255, 255, 255] };
    let (full, tone, strength) = logo_state(level, mono);
    let h = (size as f32 * 0.94).round();
    let top = (size as f32 - h) / 2.0;
    let mut rgba = vec![0; size * size * 4];
    for (i, (empty, filled)) in emblem_cover(size, h, top, top + h * (1.0 - full)).into_iter().enumerate() {
        over(&mut rgba, i, mono, empty * TRACK * strength);
        over(&mut rgba, i, tone, filled * strength);
    }
    Rendered { width: size as u32, height: size as u32, rgba }
}

/// The emblem, with the battery as a bar under it.
fn logo_bar(level: TrayLevel, size: usize, light: bool) -> Rendered {
    let mono = if light { [0, 0, 0] } else { [255, 255, 255] };
    let (full, tone, strength) = logo_state(level, mono);
    let bar = (size as f32 / 8.0).round().max(2.0) as usize;
    let h = (size - 2 * bar) as f32;
    let mut rgba = vec![0; size * size * 4];
    for (i, (cover, _)) in emblem_cover(size, h, 0.0, f32::MAX).into_iter().enumerate() {
        over(&mut rgba, i, mono, cover * strength);
    }
    let width = size - 2;
    let filled = if full > 0.0 { ((width as f32 * full).round() as usize).max(1) } else { 0 };
    for y in size - bar..size {
        for x in 1..size - 1 {
            let tone = if x - 1 < filled { (tone, strength) } else { (mono, TRACK * strength) };
            over(&mut rgba, y * size + x, tone.0, tone.1);
        }
    }
    Rendered { width: size as u32, height: size as u32, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(r: &Rendered, x: usize, y: usize) -> [u8; 4] {
        let i = (y * r.width as usize + x) * 4;
        [r.rgba[i], r.rgba[i + 1], r.rgba[i + 2], r.rgba[i + 3]]
    }

    #[test]
    fn the_emblem_traces_as_two_closed_shells() {
        let (left, right) = (outline(EMBLEM_LEFT), outline(EMBLEM_RIGHT));
        assert!(left.len() > 50 && right.len() > 50);
        // Inside each button, and not in the seam between them.
        assert!(inside(&left, 50.0, 200.0) && !inside(&right, 50.0, 200.0));
        assert!(inside(&right, 180.0, 200.0) && !inside(&left, 180.0, 200.0));
        assert!(!inside(&left, 114.0, 150.0) && !inside(&right, 114.0, 150.0));
    }

    #[test]
    fn the_logo_fills_from_the_bottom_with_the_battery() {
        let r = logo(TrayLevel::Battery { level: 50, charging: false, low_at: 15 }, 32, false);
        // Low on the left button: filled, in the taskbar's colour.
        assert_eq!(at(&r, 8, 28), [255, 255, 255, 255]);
        // High on it: the faint empty part.
        let top = at(&r, 8, 8);
        assert!(top[3] > 0 && top[3] <= (TRACK * 255.0).round() as u8 + 1, "{top:?}");
        // Low battery is red, charging blue; on a light taskbar the rest is black.
        let low = logo(TrayLevel::Battery { level: 10, charging: false, low_at: 15 }, 32, true);
        assert_eq!(at(&low, 8, 29)[..3], LOGO_LOW);
        assert_eq!(at(&low, 8, 8)[..3], [0, 0, 0]);
        let charging = logo(TrayLevel::Battery { level: 50, charging: true, low_at: 15 }, 32, false);
        assert_eq!(at(&charging, 8, 28)[..3], LOGO_CHARGE);
        // Asleep, the whole icon is dimmed.
        let asleep = logo(TrayLevel::Asleep { level: Some(50) }, 32, false);
        assert!(at(&asleep, 8, 28)[3] < 128);
    }

    #[test]
    fn the_seam_stays_open_at_the_smallest_size() {
        let r = logo(TrayLevel::Battery { level: 100, charging: false, low_at: 15 }, 16, false);
        // In the middle columns, a row near the bottom has a gap between the two buttons.
        let gap = (6..10).any(|x| at(&r, x, 13)[3] < 100 && at(&r, x - 2, 13)[3] > 200 && at(&r, x + 2, 13)[3] > 200);
        assert!(gap);
    }

    #[test]
    fn the_bar_shows_the_level_under_the_emblem() {
        let r = logo_bar(TrayLevel::Battery { level: 50, charging: false, low_at: 15 }, 32, false);
        let y = 32 - 4;
        let lit = (1..31).filter(|x| at(&r, *x, y)[3] == 255).count();
        assert_eq!(lit, 15, "half of 30");
        assert!(at(&r, 30, y)[3] < 100, "the rest is faint");
        assert_eq!(at(&r, 16, y - 1)[3], 0, "a gap above the bar");
    }

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
            for (style_name, style) in [
                ("percent", TrayStyle::Percent),
                ("battery", TrayStyle::Battery),
                ("logo", TrayStyle::Logo),
                ("logobar", TrayStyle::LogoBar),
            ] {
                // 32 px, and the logo styles also at 16 and 24, the tray's sizes at 100% and 150%.
                let sizes: &[i32] = if style_name.starts_with("logo") { &[32, 16, 24] } else { &[32] };
                for &size in sizes {
                    let r = render_at(level, style, Some(size));
                    let png = crate::win::encode_png(r.width, r.height, &r.rgba).unwrap();
                    let key =
                        if size == 32 { format!("{name}-{style_name}") } else { format!("{name}-{style_name}-{size}") };
                    out.insert(
                        key,
                        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
                            .into(),
                    );
                }
            }
        }
        std::fs::write(std::env::var("HRTZ_OUT").unwrap(), serde_json::Value::Object(out).to_string()).unwrap();
    }
}
