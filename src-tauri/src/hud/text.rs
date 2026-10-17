//! The HUD's text through GDI, set the way the lab's browser sets it, so the native HUD matches
//! `Edge.tsx` (checked against the lab's styles rendered by Edge itself, at 100%):
//! - Segoe UI Variable Display, the lab's first choice of face (Text, then Segoe UI, where it's
//!   missing).
//! - Widths from the font's design units, not GDI's widths rounded at each size, and every digit
//!   as wide as a zero with the glyph centred in it, as CSS's `tabular-nums` does.
//! - Glyphs drawn at three times the size, each at its own unrounded place, and averaged down.
//!   The browser doesn't round glyphs to whole pixels; GDI at the real size does, and its rounded
//!   shapes at rounded places leave uneven gaps ("be late  while").
//!
//! The coverage is the true share of each pixel the text covers. The browser draws text bolder
//! than that; `draw` applies its curve.

use std::collections::HashMap;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, ExtTextOutW, GdiFlush, GetTextExtentExPointW,
    GetTextFaceW, GetTextMetricsW, SelectObject, SetBkMode, SetTextAlign, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, ETO_OPTIONS, FF_DONTCARE, FW_NORMAL,
    FW_SEMIBOLD, HDC, HFONT, HGDIOBJ, OUT_TT_PRECIS, TA_BASELINE, TA_LEFT, TEXTMETRICW, TRANSPARENT,
};

use super::draw::{Face, Fonts};

/// Faces to try, best first, with the weight to ask for. GDI lists each of a variable font's
/// named instances as its own face, cut to 31 characters ("…Display Semib"), and the RIBBI ones
/// under the family's name.
const REGULAR: &[(&str, i32)] = &[
    ("Segoe UI Variable Display", FW_NORMAL.0 as i32),
    ("Segoe UI Variable Text", FW_NORMAL.0 as i32),
    ("Segoe UI", FW_NORMAL.0 as i32),
];
const SEMIBOLD: &[(&str, i32)] = &[
    ("Segoe UI Variable Display Semib", FW_SEMIBOLD.0 as i32),
    ("Segoe UI Variable Text Semibold", FW_SEMIBOLD.0 as i32),
    ("Segoe UI Semibold", FW_SEMIBOLD.0 as i32),
    ("Segoe UI", FW_SEMIBOLD.0 as i32),
];

/// Widths are measured at this size, where they're the design units of a 2048-unit em and GDI's
/// rounding to whole pixels no longer shows.
const MEASURE_PX: i32 = 2048;

/// Glyphs are drawn this many times larger and averaged down. Three came closest to the browser,
/// closer than four or six.
const OVERSAMPLE: i32 = 3;

pub struct Gdi {
    dc: HDC,
    fonts: HashMap<(i32, Face), HFONT>,
}

// The DC and fonts are only used by whoever owns the `Gdi`; GDI objects can move between threads.
unsafe impl Send for Gdi {}

impl Gdi {
    pub fn new() -> Option<Self> {
        let dc = unsafe { CreateCompatibleDC(None) };
        (!dc.is_invalid()).then(|| Self { dc, fonts: HashMap::new() })
    }

    fn font(&mut self, px: i32, face: Face) -> HFONT {
        if let Some(f) = self.fonts.get(&(px, face)) {
            return *f;
        }
        let candidates = match face {
            Face::Regular => REGULAR,
            Face::Semibold => SEMIBOLD,
        };
        let mut chosen = None;
        for (i, (name, weight)) in candidates.iter().enumerate() {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let font = unsafe {
                CreateFontW(
                    -px,
                    0,
                    0,
                    0,
                    *weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_TT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    ANTIALIASED_QUALITY,
                    FF_DONTCARE.0 as u32,
                    PCWSTR(wide.as_ptr()),
                )
            };
            if font.is_invalid() {
                continue;
            }
            // GDI hands back a stand-in for a face it doesn't have; keep looking unless it's
            // the last one. It names the face in full, past the 31 characters asked for.
            let got = self.face_of(font).to_lowercase();
            if i + 1 == candidates.len() || got.starts_with(&name.to_lowercase()) {
                chosen = Some(font);
                break;
            }
            unsafe {
                let _ = DeleteObject(font.into());
            }
        }
        let font = chosen.unwrap_or_default();
        self.fonts.insert((px, face), font);
        font
    }

    fn face_of(&self, font: HFONT) -> String {
        unsafe {
            let old = SelectObject(self.dc, font.into());
            let mut buf = [0u16; 64];
            let n = GetTextFaceW(self.dc, Some(&mut buf));
            SelectObject(self.dc, old);
            String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())])
        }
    }

    /// The face GDI uses for `face` at 15 px, for tests and logs.
    pub fn face_name(&mut self, face: Face) -> String {
        let f = self.font(15, face);
        self.face_of(f)
    }

    fn with_font<T>(&mut self, px: i32, face: Face, f: impl FnOnce(HDC) -> T) -> T {
        let font = self.font(px, face);
        unsafe {
            let old = SelectObject(self.dc, font.into());
            let out = f(self.dc);
            SelectObject(self.dc, old);
            out
        }
    }

    /// Where each UTF-16 unit of `text` starts, and where the run ends, in pixels at `px`.
    fn origins(&mut self, text: &[u16], px: i32, face: Face) -> (Vec<f32>, f32) {
        let n = text.len();
        let (ends, zero) = self.with_font(MEASURE_PX, face, |dc| unsafe {
            let mut ends = vec![0i32; n];
            let mut size = SIZE::default();
            if n > 0 {
                let _ = GetTextExtentExPointW(dc, PCWSTR(text.as_ptr()), n as i32, 0, None, Some(ends.as_mut_ptr()), &mut size);
            }
            let zero = [u16::from(b'0')];
            let _ = GetTextExtentExPointW(dc, PCWSTR(zero.as_ptr()), 1, 0, None, None, &mut size);
            (ends, size.cx as f32)
        });
        let k = px as f32 / MEASURE_PX as f32;
        let (mut pen, mut prev) = (0.0f32, 0);
        let mut origins = Vec::with_capacity(n);
        for (unit, end) in text.iter().zip(&ends) {
            let advance = (end - prev) as f32;
            prev = *end;
            if (u16::from(b'0')..=u16::from(b'9')).contains(unit) {
                origins.push((pen + (zero - advance) / 2.0) * k);
                pen += zero;
            } else {
                origins.push(pen * k);
                pen += advance;
            }
        }
        (origins, pen * k)
    }
}

impl Drop for Gdi {
    fn drop(&mut self) {
        unsafe {
            for f in self.fonts.values() {
                let _ = DeleteObject((*f).into());
            }
            let _ = DeleteDC(self.dc);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl Fonts for Gdi {
    fn width(&mut self, text: &str, px: i32, face: Face) -> f32 {
        self.origins(&wide(text), px, face).1
    }

    fn metrics(&mut self, px: i32, face: Face) -> (i32, i32) {
        self.with_font(px, face, |dc| {
            let mut tm = TEXTMETRICW::default();
            unsafe {
                let _ = GetTextMetricsW(dc, &mut tm);
            }
            (tm.tmAscent, tm.tmDescent)
        })
    }

    fn coverage(&mut self, text: &str, px: i32, face: Face, w: i32, h: i32, x: f32, y: i32) -> Vec<u8> {
        let t = wide(text);
        let empty = vec![0; (w.max(0) * h.max(0)) as usize];
        if t.is_empty() || w <= 0 || h <= 0 {
            return empty;
        }
        let ss = OVERSAMPLE;
        let (sw, sh) = (w * ss, h * ss);
        // Each character at its own place on the finer grid; the last one's advance doesn't
        // matter.
        let (origins, end) = self.origins(&t, px, face);
        let at: Vec<i32> = origins.iter().chain(Some(&end)).map(|o| ((x + o) * ss as f32).round() as i32).collect();
        let dx: Vec<i32> = at.windows(2).map(|p| p[1] - p[0]).collect();
        let font = self.font(px * ss, face);
        let dc = self.dc;
        let big: Vec<u8> = unsafe {
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: sw,
                    biHeight: -sh,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return empty };
            if bits.is_null() {
                let _ = DeleteObject(bmp.into());
                return empty;
            }
            // White on black, so any channel is the coverage.
            let old_bmp: HGDIOBJ = SelectObject(dc, bmp.into());
            let old_font = SelectObject(dc, font.into());
            SetTextColor(dc, COLORREF(0x00FF_FFFF));
            SetBkMode(dc, TRANSPARENT);
            SetTextAlign(dc, TA_BASELINE | TA_LEFT);
            let _ = ExtTextOutW(dc, at[0], y * ss, ETO_OPTIONS(0), None, PCWSTR(t.as_ptr()), t.len() as u32, Some(dx.as_ptr()));
            let _ = GdiFlush();
            let px = std::slice::from_raw_parts(bits as *const u8, (sw * sh * 4) as usize);
            let big = px.chunks(4).map(|p| p[0].max(p[1]).max(p[2])).collect();
            SelectObject(dc, old_font);
            SelectObject(dc, old_bmp);
            let _ = DeleteObject(bmp.into());
            big
        };
        // Each pixel is the average of its block.
        let n = (ss * ss) as u32;
        let mut out = empty;
        for oy in 0..h {
            for ox in 0..w {
                let mut sum = 0u32;
                for by in 0..ss {
                    let row = ((oy * ss + by) * sw + ox * ss) as usize;
                    sum += big[row..row + ss as usize].iter().map(|v| *v as u32).sum::<u32>();
                }
                out[(oy * w + ox) as usize] = ((sum + n / 2) / n) as u8;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_faces_and_measures() {
        let mut g = Gdi::new().unwrap();
        let (regular, semibold) = (g.face_name(Face::Regular), g.face_name(Face::Semibold));
        eprintln!("regular: {regular}, semibold: {semibold}");
        assert!(regular.starts_with("Segoe UI"));
        assert!(semibold.starts_with("Segoe UI"));
        let (a, d) = g.metrics(15, Face::Regular);
        assert!((14..=18).contains(&a) && (3..=6).contains(&d), "{a} {d}");
        let thin = g.width("8,000 Hz", 15, Face::Regular);
        let bold = g.width("8,000 Hz", 15, Face::Semibold);
        assert!(thin > 40.0 && bold >= thin, "{thin} {bold}");
        let cov = g.coverage("Hz", 15, Face::Regular, 30, 24, 2.0, 18);
        assert!(cov.iter().any(|c| *c > 200) && cov.iter().any(|c| *c > 0 && *c < 200), "antialiased");
        // Half a pixel to the right moves the ink, not by a whole pixel.
        let half = g.coverage("Hz", 15, Face::Regular, 30, 24, 2.5, 18);
        assert_ne!(cov, half);
        let ink = |c: &[u8]| c.iter().map(|v| *v as u32).sum::<u32>() as f32;
        assert!((ink(&cov) - ink(&half)).abs() / ink(&cov) < 0.05, "the same glyphs");
    }

    #[test]
    fn digits_are_even_width_as_tabular_nums_sets_them() {
        let mut g = Gdi::new().unwrap();
        for face in [Face::Regular, Face::Semibold] {
            let (one, zero) = (g.width("1", 15, face), g.width("0", 15, face));
            assert!((one - zero).abs() < 1e-4, "{face:?}: {one} {zero}");
            let ten = g.width("1111111111", 15, face);
            assert!((ten - 10.0 * zero).abs() < 1e-3);
        }
        // Widths aren't rounded per size. These are the lab's browser's, in Display.
        if g.face_name(Face::Semibold).starts_with("Segoe UI Variable Display") {
            assert!((g.width("Battery:", 15, Face::Regular) - 50.48).abs() < 0.1);
            assert!((g.width("11h", 15, Face::Semibold) - 25.25).abs() < 0.1);
            assert!((g.width("Values may be late while Zaopin's app is open", 13, Face::Regular) - 256.86).abs() < 1.5);
        }
    }
}
