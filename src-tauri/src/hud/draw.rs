//! How the HUD looks: Edge's tabs laid out and drawn into a premultiplied RGBA bitmap.
//!
//! Text comes from a `Fonts` (GDI on Windows, see `text.rs`), which only measures and rasterizes
//! coverage. Everything else is drawn here: the tabs' plates, the colours, the text's weight as
//! the browser draws it, its shadow or outline, hrtz's mark and its dot, and each tab's own
//! opacity. The layout is the browser's too: unrounded widths, each plate's edges rounded to the
//! pixel, the text between pixels. The window thread keeps a `Painter` so a fade only
//! re-composes; text is rasterized again only when it changes.

use super::model::{Field, Tab, Tone};
use crate::store::{HudAnchor, HudPrefs, HudSize, HudValueColor};

/// The label's grey.
const LABEL: [u8; 3] = [0xEC, 0xEC, 0xEC];
/// Valorant's stat value colour.
const TEAL: [u8; 3] = [0x62, 0xE3, 0xE8];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
/// Warning amber, the app's CardNote amber.
const AMBER: [u8; 3] = [0xFB, 0xBF, 0x24];
/// The charging green the battery bar uses.
const GREEN: [u8; 3] = [0x34, 0xD3, 0x99];
/// A tab's plate for a moment after its value changes.
const BRIGHT: [u8; 3] = [48, 48, 48];

/// Sizes at 100% scale and the medium size, in pixels.
const FONT: f32 = 15.0;
const NOTE_FONT: f32 = 13.0;
const TAB_H: f32 = 24.0;
const PAD_X: f32 = 7.0;
const SPAN_GAP: f32 = 5.0;
const ROW_GAP: f32 = 8.0;
const STACK_GAP: f32 = 4.0;
const RADIUS: f32 = 2.0;
/// Room around the tabs for the text's shadow and outline.
const BLEED: f32 = 4.0;
/// How far in from the screen's edge the tabs sit.
const MARGIN: f32 = 4.0;
/// The browser draws text bolder than the share of each pixel it covers: about this power of it,
/// fitted to the lab's text as Edge draws it.
const TEXT_GAMMA: f32 = 2.1;

/// hrtz's emblem, from the brand SVG: the short stroke and the tall one, in a 10 px box.
const EMBLEM: [&str; 2] = [
    "M4.82055 4.14743C4.79832 3.60699 4.78723 3.33677 4.99654 3.16974C5.20581 3.00271 5.43008 3.0635 5.87857 3.18508C7.81756 3.71069 9.18359 5.51537 9.18359 7.64316L9.18359 8.79305C9.18359 9.19871 9.18359 9.40156 9.10927 9.5585C9.03292 9.71962 8.90317 9.84936 8.74205 9.92568C8.58516 10 8.38231 10 7.9766 10L6.07504 10C5.59538 10 5.35555 10 5.20223 9.8529C5.04892 9.70576 5.03908 9.46613 5.01942 8.98688L4.82055 4.14743Z",
    "M2.92949e-05 4.55488C2.96674e-05 2.42424 1.45121 0.632519 3.4191 0.11422C3.86945 -0.00439428 4.09464 -0.0637013 4.30233 0.103069C4.51001 0.269839 4.49929 0.53944 4.47783 1.07864L4.16299 8.98554C4.14389 9.46536 4.13434 9.70527 3.98098 9.85266C3.82763 10 3.58752 10 3.10731 10L1.207 10C0.801315 10 0.598472 10 0.441563 9.92568C0.280435 9.84936 0.150691 9.71962 0.0743604 9.5585C2.84352e-05 9.40156 2.88534e-05 9.19871 2.89244e-05 8.79305L2.92949e-05 4.55488Z",
];
/// The emblem's width in its box.
pub(crate) const EMBLEM_W: f32 = 9.18359;
/// Lines per curve of the emblem.
const CURVE_STEPS: usize = 12;
/// The emblem's height: a fifth taller than the 15 px text's capitals (10.5 px).
const MARK_H: f32 = 12.6;
/// The emblem is muted: the label's grey, half see-through.
const MARK_ALPHA: f32 = 0.5;
/// The yellow dot's radius, in pixels like the sizes above, so it stays small on a bigger
/// emblem.
const DOT_R: f32 = 2.2;
/// The dot's centre in the emblem's box: at its top right corner, clear of the short stroke.
const DOT_AT: (f32, f32) = (9.2, 0.8);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub anchor: HudAnchor,
    pub size: HudSize,
    /// 0-100.
    pub plate: u8,
    pub value_color: HudValueColor,
}

impl Look {
    pub fn of(p: &HudPrefs) -> Self {
        Self { anchor: p.anchor, size: p.size, plate: p.plate.min(100), value_color: p.value_color }
    }

    /// Below 30 there's no tab: the text carries a dark outline instead.
    pub fn outlined(&self) -> bool {
        self.plate < 30
    }

    /// The tab's black, about 62% at the default 90: dark enough for the values to read on a
    /// bright sky, still see-through like Valorant's.
    fn chip(&self) -> f32 {
        self.plate as f32 * 0.69 / 100.0
    }

    fn value(&self) -> [u8; 3] {
        match self.value_color {
            HudValueColor::Teal => TEAL,
            HudValueColor::White => WHITE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Semibold,
}

/// A run of text in one font and colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    /// CSS pixels at 100%.
    pub size: f32,
    pub face: Face,
    pub color: [u8; 3],
    pub alpha: f32,
}

/// Measures and rasterizes text. Sizes are device pixels.
pub trait Fonts {
    /// Advance width of `text`, unrounded, as the browser lays it out.
    fn width(&mut self, text: &str, px: i32, face: Face) -> f32;
    /// Ascent and descent, rounded as the browser rounds them for a line box.
    fn metrics(&mut self, px: i32, face: Face) -> (i32, i32);
    /// Coverage (one byte per pixel, `w` x `h`) of `text` drawn from `x` on the baseline `y`.
    #[allow(clippy::too_many_arguments)]
    fn coverage(&mut self, text: &str, px: i32, face: Face, w: i32, h: i32, x: f32, y: i32) -> Vec<u8>;
}

/// A tab's runs, as Edge draws them: "Battery:" in grey (unless labels are off), the value
/// semibold in its colour, a dim "•" before a second value, and a small grey note.
pub fn spans(tab: &Tab, look: &Look) -> Vec<Span> {
    let span = |text: &str, size, face, color, alpha| Span { text: text.to_string(), size, face, color, alpha };
    match tab.field {
        // The mark is drawn, not written (see `mark_place`).
        Field::Mark => return Vec::new(),
        Field::Vendor => {
            return vec![span(tab.note.as_deref().unwrap_or_default(), NOTE_FONT, Face::Regular, LABEL, 0.8)];
        }
        _ => {}
    }
    let color = match tab.tone {
        Tone::Warn => AMBER,
        Tone::Charge => GREEN,
        Tone::Plain => look.value(),
    };
    let mut out = Vec::new();
    if !tab.label.is_empty() {
        out.push(span(&format!("{}:", tab.label), FONT, Face::Regular, LABEL, 1.0));
    }
    out.push(span(&tab.value, FONT, Face::Semibold, color, 1.0));
    if let Some(x) = &tab.extra {
        out.push(span("•", FONT, Face::Regular, LABEL, 0.55));
        out.push(span(x, FONT, Face::Semibold, color, 1.0));
    }
    if let Some(n) = &tab.note {
        out.push(span(n, NOTE_FONT, Face::Regular, LABEL, 0.7));
    }
    out
}

/// Pixel sizes at one scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// The monitor's scale times the size's: everything below, the text, its shadow and its
    /// outline are drawn at it.
    pub scale: f32,
    pub tab_h: i32,
    pub pad_x: i32,
    pub span_gap: i32,
    pub gap: i32,
    pub radius: f32,
    pub bleed: i32,
    pub margin: i32,
}

impl Metrics {
    /// `scale` is the monitor's; the look's size goes on top of it, so a large HUD on a 150%
    /// display is drawn at 1.8.
    pub fn new(scale: f32, look: &Look) -> Self {
        let scale = scale * look.size.factor();
        let px = |v: f32| (v * scale).round() as i32;
        Self {
            scale,
            tab_h: px(TAB_H),
            pad_x: if look.outlined() { 0 } else { px(PAD_X) },
            span_gap: px(SPAN_GAP),
            gap: px(if look.anchor.stacked() { STACK_GAP } else { ROW_GAP }),
            radius: RADIUS * scale,
            bleed: (BLEED * scale).ceil() as i32,
            margin: px(MARGIN),
        }
    }

    fn font_px(&self, size: f32) -> i32 {
        (size * self.scale).round().max(1.0) as i32
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedSpan {
    pub span: Span,
    /// From the tab's left edge, unrounded: text keeps its place between pixels, as in the
    /// browser.
    pub x: f32,
    pub w: f32,
}

/// Where the mark's emblem and dot go, in pixels from its tab's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MarkPlace {
    /// The emblem's box: left, top and height (its width follows).
    pub emblem: (f32, f32, f32),
    /// The dot's centre and radius, when it shows.
    pub dot: Option<(f32, f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The tab's box, from the layout's top left (the bitmap's, less the bleed), its edges
    /// rounded to the pixel as the browser paints a background.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// From the tab's top.
    pub baseline: i32,
    pub spans: Vec<PlacedSpan>,
    /// The mark's tab draws this instead of text.
    pub mark: Option<MarkPlace>,
}

/// Where the mark's emblem and dot go in its tab, and the tab's width. The mark has no plate, so
/// no padding: the emblem stands bare on the text's baseline, with the dot at its top right. Its
/// strokes are heavier at the foot, so standing there its ink centres on the text's capitals.
fn mark_place(t: &Tab, m: &Metrics, baseline: i32) -> (MarkPlace, f32) {
    // Whole pixels tall and on whole pixels, so its foot and its left side are sharp.
    let h = (MARK_H * m.scale).round().max(1.0);
    let k = h / 10.0;
    let (top, r) = (baseline as f32 - h, DOT_R * m.scale);
    let dot = (t.tone == Tone::Warn).then_some((DOT_AT.0 * k, top + DOT_AT.1 * k, r));
    (MarkPlace { emblem: (0.0, top, h), dot }, EMBLEM_W * k)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The tabs' bounding box.
    pub inner_w: i32,
    pub inner_h: i32,
    /// The bitmap: the box plus the bleed on every side.
    pub width: i32,
    pub height: i32,
    pub bleed: i32,
    /// How far in from the screen's edge the box goes.
    pub margin: i32,
    pub tabs: Vec<Placed>,
}

/// Where everything goes. A row at the top or bottom of the screen, a stack at the left or right
/// (aligned to that edge); each tab keeps its slot whether or not it's showing.
pub fn layout(tabs: &[Tab], scale: f32, look: &Look, fonts: &mut impl Fonts) -> Layout {
    let m = Metrics::new(scale, look);
    let (px15, px13) = (m.font_px(FONT), m.font_px(NOTE_FONT));
    let (a15, d15) = fonts.metrics(px15, Face::Regular);
    let (a13, d13) = fonts.metrics(px13, Face::Regular);
    // Each run's line is the tab's height with its text centred; the runs share a baseline, the
    // 15px text's when there is any.
    let baseline =
        |px: i32| if px == px13 { (m.tab_h - (a13 + d13)) / 2 + a13 } else { (m.tab_h - (a15 + d15)) / 2 + a15 };

    // Each tab's runs (or the mark), its width unrounded as the browser lays it out, and its
    // baseline.
    let runs: Vec<(Vec<PlacedSpan>, Option<MarkPlace>, f32, i32)> = tabs
        .iter()
        .map(|t| {
            if t.field == Field::Mark {
                let base = baseline(px15);
                let (mark, w) = mark_place(t, &m, base);
                return (Vec::new(), Some(mark), w, base);
            }
            let mut x = m.pad_x as f32;
            let mut runs = Vec::new();
            let mut largest = px13;
            for s in spans(t, look) {
                let px = m.font_px(s.size);
                largest = largest.max(px);
                let w = fonts.width(&s.text, px, s.face);
                if !runs.is_empty() {
                    x += m.span_gap as f32;
                }
                runs.push(PlacedSpan { span: s, x, w });
                x += w;
            }
            (runs, None, x + m.pad_x as f32, baseline(largest))
        })
        .collect();

    // Where each tab starts, unrounded: in a row one after another, in a stack flush to its edge.
    let starts: Vec<(f32, i32)> = if look.anchor.stacked() {
        let widest = runs.iter().map(|r| r.2).fold(0.0, f32::max);
        let flush = |w: f32| if look.anchor.right() { widest - w } else { 0.0 };
        runs.iter().enumerate().map(|(i, r)| (flush(r.2), i as i32 * (m.tab_h + m.gap))).collect()
    } else {
        let mut x = 0.0;
        runs.iter()
            .map(|r| {
                let at = x;
                x += r.2 + m.gap as f32;
                (at, 0)
            })
            .collect()
    };

    // A tab's plate has its edges rounded to the pixel; its text keeps its unrounded place, and
    // the mark stays on whole pixels of its plate.
    let placed: Vec<Placed> = runs
        .into_iter()
        .zip(starts)
        .map(|((mut spans, mark, w, baseline), (x, y))| {
            let (left, right) = (x.round() as i32, (x + w).round() as i32);
            for s in &mut spans {
                s.x += x - left as f32;
            }
            Placed { x: left, y, w: right - left, h: m.tab_h, baseline, spans, mark }
        })
        .collect();
    let inner_w = placed.iter().map(|p| p.x + p.w).max().unwrap_or(0);
    let inner_h = placed.iter().map(|p| p.y + p.h).max().unwrap_or(0);
    Layout {
        inner_w,
        inner_h,
        width: inner_w + 2 * m.bleed,
        height: inner_h + 2 * m.bleed,
        bleed: m.bleed,
        margin: m.margin,
        tabs: placed,
    }
}

/// A premultiplied RGBA image.
#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Bitmap {
    /// Straight (not premultiplied) RGBA, for a PNG.
    pub fn straight(&self) -> Vec<u8> {
        let mut out = self.rgba.clone();
        for p in out.chunks_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        out
    }

    /// Premultiplied BGRA, what `UpdateLayeredWindow` takes.
    pub fn bgra(&self) -> Vec<u8> {
        let mut out = self.rgba.clone();
        for p in out.chunks_mut(4) {
            p.swap(0, 2);
        }
        out
    }
}

/// A float canvas, premultiplied, 0 to 1.
struct Canvas {
    w: i32,
    h: i32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: i32, h: i32) -> Self {
        Self { w, h, px: vec![[0.0; 4]; (w.max(0) * h.max(0)) as usize] }
    }

    /// Source-over at (x, y), the source scaled by `alpha`.
    fn over(&mut self, x: i32, y: i32, src: &Canvas, alpha: f32) {
        for sy in 0..src.h {
            let ty = y + sy;
            if ty < 0 || ty >= self.h {
                continue;
            }
            for sx in 0..src.w {
                let tx = x + sx;
                if tx < 0 || tx >= self.w {
                    continue;
                }
                let s = src.px[(sy * src.w + sx) as usize];
                if s[3] <= 0.0 {
                    continue;
                }
                let d = &mut self.px[(ty * self.w + tx) as usize];
                let k = 1.0 - s[3] * alpha;
                for c in 0..4 {
                    d[c] = s[c] * alpha + d[c] * k;
                }
            }
        }
    }

    /// Paint `color` (straight RGB, 0-255) at `alpha` through a coverage mask at (x, y).
    fn fill(&mut self, x: i32, y: i32, w: i32, mask: &[f32], color: [u8; 3], alpha: f32) {
        let h = mask.len() as i32 / w.max(1);
        let rgb = color.map(|c| c as f32 / 255.0);
        for my in 0..h {
            let ty = y + my;
            if ty < 0 || ty >= self.h {
                continue;
            }
            for mx in 0..w {
                let tx = x + mx;
                let a = mask[(my * w + mx) as usize] * alpha;
                if tx < 0 || tx >= self.w || a <= 0.0 {
                    continue;
                }
                let d = &mut self.px[(ty * self.w + tx) as usize];
                let k = 1.0 - a;
                for c in 0..3 {
                    d[c] = rgb[c] * a + d[c] * k;
                }
                d[3] = a + d[3] * k;
            }
        }
    }

    fn bitmap(&self) -> Bitmap {
        let mut rgba = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = p[3].clamp(0.0, 1.0);
            for c in &p[..3] {
                rgba.push((c.clamp(0.0, a) * 255.0).round() as u8);
            }
            rgba.push((a * 255.0).round() as u8);
        }
        Bitmap { width: self.w as u32, height: self.h as u32, rgba }
    }
}

/// A rounded rectangle's coverage, `w` x `h`, with antialiased corners.
fn rounded(w: i32, h: i32, r: f32) -> Vec<f32> {
    let r = r.min(w as f32 / 2.0).min(h as f32 / 2.0);
    let mut out = vec![0.0; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(r, w as f32 - r);
            let cy = fy.clamp(r, h as f32 - r);
            let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            out[(y * w + x) as usize] = if d == 0.0 { 1.0 } else { (r + 0.5 - d).clamp(0.0, 1.0) };
        }
    }
    out
}

/// A separable Gaussian blur of `sigma` pixels, as CSS blurs a shadow. Each tap is the Gaussian's
/// share of that whole pixel, which keeps small blurs (half a pixel) honest.
fn blur(mask: &[f32], w: i32, h: i32, sigma: f32) -> Vec<f32> {
    let reach = (3.0 * sigma).ceil().max(1.0) as i32;
    let cdf = |x: f32| 0.5 * (1.0 + erf(x / (sigma * std::f32::consts::SQRT_2)));
    let weights: Vec<f32> = (-reach..=reach).map(|i| cdf(i as f32 + 0.5) - cdf(i as f32 - 0.5)).collect();
    let pass = |src: &[f32], horizontal: bool| {
        let mut out = vec![0.0; src.len()];
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0.0;
                for (i, wt) in weights.iter().enumerate() {
                    let o = i as i32 - reach;
                    let (sx, sy) = if horizontal { (x + o, y) } else { (x, y + o) };
                    if sx >= 0 && sx < w && sy >= 0 && sy < h {
                        acc += src[(sy * w + sx) as usize] * wt;
                    }
                }
                out[(y * w + x) as usize] = acc;
            }
        }
        out
    };
    pass(&pass(mask, true), false)
}

/// The error function, to 1.5e-7 (Abramowitz and Stegun 7.1.26).
fn erf(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t * (0.254_829_6 + t * (-0.284_496_7 + t * (1.421_413_8 + t * (-1.453_152 + t * 1.061_405_4))));
    let y = 1.0 - poly * (-x * x).exp();
    if x < 0.0 { -y } else { y }
}

/// The mask moved down by `dy` pixels.
fn shifted(mask: &[f32], w: i32, dy: i32) -> Vec<f32> {
    let mut out = vec![0.0; mask.len()];
    let skip = (dy.max(0) * w) as usize;
    if skip < mask.len() {
        out[skip..].copy_from_slice(&mask[..mask.len() - skip]);
    }
    out
}

/// The emblem's two outlines in its 10 px box, its curves cut into short lines. The brand SVG's
/// paths use only absolute M, L, C and Z.
pub(crate) fn emblem() -> Vec<Vec<(f32, f32)>> {
    EMBLEM
        .iter()
        .map(|d| {
            let mut out: Vec<(f32, f32)> = Vec::new();
            let (mut cmd, mut nums) = ('M', Vec::<f32>::new());
            // Commands are capitals; the "e" in "2.9e-05" belongs to its number.
            let words: String =
                d.chars().map(|c| if c.is_ascii_uppercase() { format!(" {c} ") } else { c.to_string() }).collect();
            for word in words.split_whitespace() {
                let Ok(n) = word.parse::<f32>() else {
                    cmd = word.chars().next().unwrap_or('Z');
                    nums.clear();
                    continue;
                };
                nums.push(n);
                match (cmd, nums.as_slice()) {
                    ('M' | 'L', &[x, y]) => out.push((x, y)),
                    ('C', &[x1, y1, x2, y2, x, y]) => {
                        let (x0, y0) = out.last().copied().unwrap_or_default();
                        for i in 1..=CURVE_STEPS {
                            let t = i as f32 / CURVE_STEPS as f32;
                            let u = 1.0 - t;
                            let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                            out.push((a * x0 + b * x1 + c * x2 + e * x, a * y0 + b * y1 + c * y2 + e * y));
                        }
                    }
                    _ => continue,
                }
                nums.clear();
            }
            out
        })
        .collect()
}

/// A circle as a polygon fine enough for a dot.
fn circle(cx: f32, cy: f32, r: f32) -> Vec<(f32, f32)> {
    (0..48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// The coverage of closed polygons (even-odd), `w` x `h`: 16 rows of samples per pixel, each
/// adding the exact share of every pixel its spans cross.
pub(crate) fn fill_polygons(polys: &[Vec<(f32, f32)>], w: i32, h: i32) -> Vec<f32> {
    const ROWS: usize = 16;
    let mut out = vec![0.0f32; (w.max(0) * h.max(0)) as usize];
    let mut xs: Vec<f32> = Vec::new();
    for py in 0..h.max(0) {
        let row = &mut out[(py * w) as usize..((py + 1) * w) as usize];
        for sub in 0..ROWS {
            let y = py as f32 + (sub as f32 + 0.5) / ROWS as f32;
            xs.clear();
            for poly in polys {
                for (i, a) in poly.iter().enumerate() {
                    let b = poly[(i + 1) % poly.len()];
                    if (a.1 <= y) != (b.1 <= y) {
                        xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                    }
                }
            }
            xs.sort_by(f32::total_cmp);
            for [a, b] in xs.as_chunks::<2>().0 {
                let (x0, x1) = (a.max(0.0), b.min(w as f32));
                let mut px = x0.floor() as i32;
                while x1 > x0 && (px as f32) < x1 {
                    row[px as usize] += (x1.min(px as f32 + 1.0) - x0.max(px as f32)) / ROWS as f32;
                    px += 1;
                }
            }
        }
    }
    for v in &mut out {
        *v = v.min(1.0);
    }
    out
}

/// A tab's text with its shadow (on a plate) or outline (without one), the tab's box plus the
/// bleed on every side. The mark's tab draws its emblem and dot the same way, always outlined,
/// since it never has a plate.
fn text_layer(p: &Placed, m: &Metrics, look: &Look, fonts: &mut impl Fonts) -> Canvas {
    let (w, h) = (p.w + 2 * m.bleed, p.h + 2 * m.bleed);
    let mut union = vec![0.0f32; (w * h) as usize];
    let mut colour = Canvas::new(w, h);
    for s in &p.spans {
        let px = m.font_px(s.span.size);
        let cov = fonts.coverage(&s.span.text, px, s.span.face, w, h, m.bleed as f32 + s.x, m.bleed + p.baseline);
        // The shadow follows the glyphs' true shape; the glyphs themselves are drawn through the
        // browser's curve.
        let shape: Vec<f32> = cov.iter().map(|c| *c as f32 / 255.0).collect();
        for (u, c) in union.iter_mut().zip(&shape) {
            *u = u.max(*c);
        }
        let ink: Vec<f32> = shape.iter().map(|c| c.powf(1.0 / TEXT_GAMMA)).collect();
        colour.fill(0, 0, w, &ink, s.span.color, s.span.alpha);
    }
    // The mark's shapes aren't text, so their edges aren't curved, and their shadow is as see-
    // through as they are.
    if let Some(mark) = p.mark {
        let b = m.bleed as f32;
        let (x, y, eh) = mark.emblem;
        let k = eh / 10.0;
        let polys: Vec<Vec<(f32, f32)>> = emblem()
            .into_iter()
            .map(|poly| poly.into_iter().map(|(u, v)| (b + x + u * k, b + y + v * k)).collect())
            .collect();
        let mut shapes = vec![(polys, LABEL, MARK_ALPHA)];
        if let Some((x, y, r)) = mark.dot {
            shapes.push((vec![circle(b + x, b + y, r)], AMBER, 1.0));
        }
        for (polys, color, alpha) in shapes {
            let cov = fill_polygons(&polys, w, h);
            for (u, c) in union.iter_mut().zip(&cov) {
                *u = u.max(*c * alpha);
            }
            colour.fill(0, 0, w, &cov, color, alpha);
        }
    }
    let mut layer = Canvas::new(w, h);
    let down = (m.scale).round().max(1.0) as i32;
    if look.outlined() || p.mark.is_some() {
        // Edge's outline: "0 0 1px #000" three times over, on "0 1px 2px rgb(0 0 0 / 0.7)". CSS
        // blurs a shadow by half its radius.
        let shadow = blur(&shifted(&union, w, down), w, h, m.scale);
        layer.fill(0, 0, w, &shadow, [0, 0, 0], 0.7);
        let halo = blur(&union, w, h, 0.5 * m.scale);
        for _ in 0..3 {
            layer.fill(0, 0, w, &halo, [0, 0, 0], 1.0);
        }
    } else {
        // "0 1px 1px rgb(0 0 0 / 0.45)".
        let shadow = blur(&shifted(&union, w, down), w, h, 0.5 * m.scale);
        layer.fill(0, 0, w, &shadow, [0, 0, 0], 0.45);
    }
    layer.over(0, 0, &colour, 1.0);
    layer
}

/// Keeps the tabs' text rasterized between frames, so a fade or a brighter plate only
/// re-composes.
pub struct Painter<F: Fonts> {
    fonts: F,
    key: Option<(Vec<Tab>, u32, Look)>,
    pub layout: Option<Layout>,
    layers: Vec<Canvas>,
    /// Each tab's rounded plate, as coverage.
    plates: Vec<Vec<f32>>,
}

impl<F: Fonts> Painter<F> {
    pub fn new(fonts: F) -> Self {
        Self { fonts, key: None, layout: None, layers: Vec::new(), plates: Vec::new() }
    }

    /// Lay out and rasterize `tabs`, unless that's what's there already. Returns whether
    /// anything changed.
    pub fn set(&mut self, tabs: &[Tab], scale: f32, look: Look) -> bool {
        if let Some((t, s, l)) = &self.key
            && t.as_slice() == tabs
            && *s == scale.to_bits()
            && *l == look
        {
            return false;
        }
        let key = (tabs.to_vec(), scale.to_bits(), look);
        let m = Metrics::new(scale, &look);
        let l = layout(tabs, scale, &look, &mut self.fonts);
        self.layers = l.tabs.iter().map(|p| text_layer(p, &m, &look, &mut self.fonts)).collect();
        self.plates = l.tabs.iter().map(|p| rounded(p.w, p.h, m.radius)).collect();
        self.layout = Some(l);
        self.key = Some(key);
        true
    }

    /// The HUD with each tab at its own opacity, and its plate `bright` (0-1) of the way to the
    /// just-changed grey.
    pub fn compose(&self, alpha: &[f32], bright: &[f32]) -> Option<Bitmap> {
        let (l, (_, _, look)) = (self.layout.as_ref()?, self.key.as_ref()?);
        let mut c = Canvas::new(l.width, l.height);
        let chip = look.chip();
        if !look.outlined() {
            for (i, (p, mask)) in l.tabs.iter().zip(&self.plates).enumerate() {
                let (a, b) =
                    (alpha.get(i).copied().unwrap_or(1.0), bright.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0));
                // The mark stands bare.
                if a <= 0.0 || p.mark.is_some() {
                    continue;
                }
                let colour = BRIGHT.map(|v| (v as f32 * b).round() as u8);
                let plate = chip + ((chip + 0.15).min(1.0) - chip) * b;
                c.fill(l.bleed + p.x, l.bleed + p.y, p.w, mask, colour, plate * a);
            }
        }
        for (i, (p, layer)) in l.tabs.iter().zip(&self.layers).enumerate() {
            let a = alpha.get(i).copied().unwrap_or(1.0);
            if a > 0.0 {
                c.over(p.x, p.y, layer, a);
            }
        }
        Some(c.bitmap())
    }
}

/// The HUD with every tab showing, as it looks on screen at `scale` (the monitor's; the look
/// carries the size), on a transparent background.
pub fn render(tabs: &[Tab], scale: f32, look: Look, fonts: impl Fonts) -> Option<Bitmap> {
    if tabs.is_empty() {
        return None;
    }
    let mut p = Painter::new(fonts);
    p.set(tabs, scale, look);
    p.compose(&vec![1.0; tabs.len()], &vec![0.0; tabs.len()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::model::{Facts, tabs};
    use crate::store::{HudFields, Prefs};

    /// Every character 8 px wide at 15 px (scaled with the size), ascent 80% and descent 20%.
    struct Fixed;

    impl Fonts for Fixed {
        fn width(&mut self, text: &str, px: i32, _: Face) -> f32 {
            (text.chars().count() as i32 * px * 8 / 15) as f32
        }
        fn metrics(&mut self, px: i32, _: Face) -> (i32, i32) {
            (px * 4 / 5, px / 5)
        }
        fn coverage(&mut self, text: &str, px: i32, face: Face, w: i32, h: i32, x: f32, y: i32) -> Vec<u8> {
            // A solid block per run, so drawing can be checked by pixel.
            let mut out = vec![0u8; (w * h) as usize];
            let (a, _) = self.metrics(px, face);
            let x = x.round() as i32;
            for yy in (y - a).max(0)..y.min(h) {
                for xx in x.max(0)..(x + self.width(text, px, face) as i32).min(w) {
                    out[(yy * w + xx) as usize] = 255;
                }
            }
            out
        }
    }

    fn look(anchor: HudAnchor, plate: u8) -> Look {
        Look { anchor, size: HudSize::Medium, plate, value_color: HudValueColor::Teal }
    }

    /// The tabs for these facts less the mark: the values alone.
    fn values(facts: &Facts, fields: &HudFields) -> Vec<Tab> {
        tabs(facts, fields).into_iter().filter(|t| t.field != Field::Mark).collect()
    }

    fn yours() -> Vec<Tab> {
        values(&Facts::sample(&Prefs::default()), &HudFields::default())
    }

    #[test]
    fn edge_spans_and_colours() {
        let t = values(
            &Facts {
                dpi: Some((800, 1, 4)),
                battery: Some((14, false)),
                time_left_h: Some(11.0),
                ..Facts::sample(&Prefs::default())
            },
            &HudFields::default(),
        );
        let l = look(HudAnchor::TopRight, 90);
        let battery = spans(&t[0], &l);
        let texts: Vec<&str> = battery.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["Battery:", "14%", "•", "11h"]);
        assert_eq!(battery[1].color, AMBER, "low");
        assert_eq!((battery[1].face, battery[0].face), (Face::Semibold, Face::Regular));
        assert_eq!(battery[2].alpha, 0.55);
        let dpi = spans(&t[2], &l);
        assert_eq!((dpi[1].color, dpi[2].text.as_str(), dpi[2].size, dpi[2].alpha), (TEAL, "2/4", 13.0, 0.7));
        let white = Look { value_color: HudValueColor::White, ..l };
        assert_eq!(spans(&t[1], &white)[1].color, WHITE);
        let charging =
            values(&Facts { battery: Some((62, true)), ..Facts::sample(&Prefs::default()) }, &HudFields::default());
        assert_eq!(spans(&charging[0], &l)[1].color, GREEN);

        // Without labels the value leads, in the same colours.
        let bare = values(
            &Facts {
                dpi: Some((800, 1, 4)),
                battery: Some((14, false)),
                time_left_h: Some(11.0),
                ..Facts::sample(&Prefs::default())
            },
            &HudFields { labels: false, ..HudFields::default() },
        );
        let texts = |t: &Tab| spans(t, &l).iter().map(|s| s.text.clone()).collect::<Vec<_>>();
        assert_eq!(
            (texts(&bare[0]), texts(&bare[1]), texts(&bare[2])),
            (
                vec!["14%".to_string(), "•".into(), "11h".into()],
                vec!["8K Hz".into()],
                vec!["800 DPI".into(), "2/4".into()]
            )
        );
        assert_eq!((spans(&bare[0], &l)[0].color, spans(&bare[2], &l)[0].face), (AMBER, Face::Semibold));
    }

    #[test]
    fn the_emblem_is_the_brand_svg() {
        let e = emblem();
        assert_eq!(e.len(), 2);
        let xs = e.iter().flatten().map(|p| p.0);
        let ys = e.iter().flatten().map(|p| p.1);
        let (x0, x1) = (xs.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max));
        let (y0, y1) = (ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
        assert!(x0.abs() < 1e-3 && (x1 - EMBLEM_W).abs() < 1e-3, "{x0} {x1}");
        assert!(y0.abs() < 0.05 && (y1 - 10.0).abs() < 1e-3, "{y0} {y1}");
        // Filled, it covers what the strokes do: about half the box, all of a stroke's middle
        // and none of the gap between them.
        let k = 10;
        let big: Vec<Vec<(f32, f32)>> =
            e.iter().map(|p| p.iter().map(|(x, y)| (x * k as f32, y * k as f32)).collect()).collect();
        let cov = fill_polygons(&big, 10 * k, 10 * k);
        let at = |x: f32, y: f32| cov[((y * k as f32) as i32 * 10 * k + (x * k as f32) as i32) as usize];
        let share = cov.iter().sum::<f32>() / (100 * k * k) as f32;
        assert!((0.5..0.75).contains(&share), "{share}");
        assert_eq!((at(2.0, 6.0), at(7.0, 8.0), at(4.65, 6.0), at(8.5, 1.0)), (1.0, 1.0, 0.0, 0.0));
    }

    #[test]
    fn the_mark_leads_with_its_dot_at_its_top_right() {
        let sample = Facts::sample(&Prefs::default());
        let away = Facts { link: crate::hud::model::Link::Away, ..sample.clone() };
        let row = look(HudAnchor::TopRight, 90);
        let px = |b: &Bitmap, x: i32, y: i32| {
            let i = ((y * b.width as i32 + x) * 4) as usize;
            [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2], b.rgba[i + 3]]
        };

        // The emblem is 13 px tall at medium (12.6 rounded), on the baseline. It has no plate,
        // so its tab is just its 11.9 px, and the row's gap follows.
        let t = tabs(&sample, &HudFields::default());
        let l = layout(&t, 1.0, &row, &mut Fixed);
        let mark = l.tabs[0].mark.unwrap();
        assert_eq!(mark, MarkPlace { emblem: (0.0, (l.tabs[0].baseline - 13) as f32, 13.0), dot: None });
        assert_eq!((l.tabs[0].w, l.tabs[1].x), (12, 12 + 8));
        assert!(l.tabs[0].spans.is_empty());
        // The value tabs are as they were, after it.
        let values_only = layout(&yours(), 1.0, &row, &mut Fixed);
        assert_eq!(l.tabs[1].w, values_only.tabs[0].w);

        // Drawn muted and bare: the label's grey at half over its dark outline, in the tall
        // stroke's middle, and nothing around it where a plate would be.
        let b = render(&t, 1.0, row, Fixed).unwrap();
        let (ex, ey) = (l.bleed, l.bleed + l.tabs[0].baseline - 13);
        let stroke = px(&b, ex + 2, ey + 6);
        assert!(
            (stroke[0] as i32 - 0xEC / 2).abs() <= 3 && stroke[0] == stroke[1] && stroke[1] == stroke[2],
            "{stroke:?}"
        );
        assert!(stroke[3] > 200, "{stroke:?}");
        assert_eq!(px(&b, ex + 5, l.bleed + l.tabs[0].h - 1), [0; 4], "no plate");
        // The value tabs keep theirs.
        assert_eq!(px(&b, l.bleed + l.tabs[1].x + 3, l.bleed + 2), [0, 0, 0, (0.9f32 * 0.69 * 255.0).round() as u8]);
        // No dot while the mouse answers.
        assert_eq!(px(&b, ex + 10, ey + 1), [0; 4]);

        // Away: the dot, amber, at the emblem's top right corner, and the tab no wider. The dot
        // keeps its size whatever the emblem's.
        let t = tabs(&away, &HudFields::default());
        let l = layout(&t, 1.0, &row, &mut Fixed);
        let (dx, dy, r) = l.tabs[0].mark.unwrap().dot.unwrap();
        assert!((dx - 9.2 * 1.3).abs() < 1e-4 && (r - 2.2).abs() < 1e-4, "{dx} {dy} {r}");
        assert_eq!(l.tabs[0].w, 12);
        let b = render(&t, 1.0, row, Fixed).unwrap();
        let dot = px(&b, l.bleed + dx as i32, l.bleed + dy as i32);
        assert_eq!(dot, [AMBER[0], AMBER[1], AMBER[2], 255]);
    }

    #[test]
    fn the_mark_tops_a_stack_and_keeps_to_whole_pixels() {
        let t = tabs(&Facts::sample(&Prefs::default()), &HudFields::default());
        let s = layout(&t, 1.0, &look(HudAnchor::Right, 90), &mut Fixed);
        assert_eq!((s.tabs[0].y, s.tabs[0].x + s.tabs[0].w), (0, s.inner_w), "on top, flush right");
        let (x, ..) = s.tabs[0].mark.unwrap().emblem;
        assert_eq!(x, 0.0, "on the tab's rounded edge");
        // Larger on a 150% display: 1.5 x 1.2 x 12.6 is 22.7, so 23 px.
        let big = layout(&t, 1.5, &Look { size: HudSize::Large, ..look(HudAnchor::TopRight, 90) }, &mut Fixed);
        assert_eq!(big.tabs[0].mark.unwrap().emblem.2, 23.0);
        // Bare at every plate.
        let bare = layout(&t, 1.0, &look(HudAnchor::TopRight, 0), &mut Fixed);
        assert_eq!((bare.tabs[0].mark.unwrap().emblem.0, bare.tabs[0].w), (0.0, 12));
    }

    #[test]
    fn a_row_at_the_top_and_a_stack_at_the_side() {
        let t = yours();
        // "Battery:" 8 chars, "50%" 3, "•" 1, "6h 30m" 6: 18 chars of 8 px, three 5 px gaps and
        // 7 px either side.
        let l = layout(&t, 1.0, &look(HudAnchor::TopRight, 90), &mut Fixed);
        assert_eq!(l.tabs[0].w, 18 * 8 + 3 * 5 + 2 * 7);
        assert!(l.tabs.iter().all(|p| p.h == 24 && p.y == 0));
        assert_eq!(l.tabs[1].x, l.tabs[0].w + 8, "8 px apart in a row");
        assert_eq!(l.inner_w, l.tabs.iter().map(|p| p.w).sum::<i32>() + 2 * 8);
        assert_eq!((l.inner_h, l.width - l.inner_w, l.bleed), (24, 8, 4));
        // The baseline: the 15 px text's line centred in 24 px.
        assert_eq!(l.tabs[0].baseline, (24 - 15) / 2 + 12);

        let s = layout(&t, 1.0, &look(HudAnchor::Right, 90), &mut Fixed);
        assert_eq!(s.inner_h, 3 * 24 + 2 * 4, "4 px apart in a stack");
        assert_eq!(s.inner_w, s.tabs.iter().map(|p| p.w).max().unwrap());
        assert!(s.tabs.iter().all(|p| p.x + p.w == s.inner_w), "flush right");
        let left = layout(&t, 1.0, &look(HudAnchor::Left, 90), &mut Fixed);
        assert!(left.tabs.iter().all(|p| p.x == 0), "flush left");

        // No plate: no padding.
        let bare = layout(&t, 1.0, &look(HudAnchor::Top, 0), &mut Fixed);
        assert_eq!(bare.tabs[0].w, 18 * 8 + 3 * 5);

        // At 150% everything scales.
        let big = layout(&t, 1.5, &look(HudAnchor::TopRight, 90), &mut Fixed);
        assert_eq!(big.tabs[0].h, 36);
        assert_eq!(big.tabs[1].x - big.tabs[0].w, 12);
        assert_eq!((big.bleed, big.margin), (6, 6));
        assert_eq!(l.margin, 4, "4 px in from the edge at 100%");
    }

    #[test]
    fn the_size_scales_everything_on_top_of_the_monitor() {
        let sized = |size, plate| Look { size, ..look(HudAnchor::TopRight, plate) };
        let m = |scale, size| Metrics::new(scale, &sized(size, 90));
        // Medium is the lab's 100%.
        let medium = m(1.0, HudSize::Medium);
        assert_eq!(
            (medium.tab_h, medium.pad_x, medium.span_gap, medium.gap, medium.bleed, medium.margin),
            (24, 7, 5, 8, 4, 4)
        );
        assert_eq!((medium.scale, medium.radius), (1.0, 2.0));
        // Small is 0.85 of it: 20.4, 5.95, 4.25, 6.8 and 3.4 px, the bleed rounded up.
        let small = m(1.0, HudSize::Small);
        assert_eq!(
            (small.tab_h, small.pad_x, small.span_gap, small.gap, small.bleed, small.margin),
            (20, 6, 4, 7, 4, 3)
        );
        assert!((small.radius - 1.7).abs() < 1e-6 && (small.scale - 0.85).abs() < 1e-6);
        // Large is 1.2: 28.8, 8.4, 6, 9.6 and 4.8 px.
        let large = m(1.0, HudSize::Large);
        assert_eq!(
            (large.tab_h, large.pad_x, large.span_gap, large.gap, large.bleed, large.margin),
            (29, 8, 6, 10, 5, 5)
        );
        assert!((large.radius - 2.4).abs() < 1e-6);
        // On top of the monitor's scale: large at 150% is 1.8.
        let both = m(1.5, HudSize::Large);
        assert!((both.scale - 1.8).abs() < 1e-6);
        assert_eq!((both.tab_h, both.gap, both.bleed, both.margin), (43, 14, 8, 7));
        // A stack's gap too.
        assert_eq!(Metrics::new(1.0, &Look { size: HudSize::Large, ..look(HudAnchor::Right, 90) }).gap, 5);

        // The text follows: 15 and 13 px become 13 and 11 small, 18 and 16 large.
        assert_eq!((small.font_px(FONT), small.font_px(NOTE_FONT)), (13, 11));
        assert_eq!((large.font_px(FONT), large.font_px(NOTE_FONT)), (18, 16));
        let t = yours();
        let row = |size| layout(&t, 1.0, &sized(size, 90), &mut Fixed);
        let (s, md, lg) = (row(HudSize::Small), row(HudSize::Medium), row(HudSize::Large));
        // "Battery:", "50%", "•" and "6h 30m" at Fixed's widths, with the size's gaps and padding.
        let runs = |px: i32| [8, 3, 1, 6].iter().map(|n| n * px * 8 / 15).sum::<i32>();
        assert_eq!(s.tabs[0].w, runs(13) + 3 * 4 + 2 * 6);
        assert_eq!(lg.tabs[0].w, runs(18) + 3 * 6 + 2 * 8);
        assert_eq!((s.inner_h, md.inner_h, lg.inner_h), (20, 24, 29));
        assert_eq!((s.margin, md.margin, lg.margin), (3, 4, 5));
        assert_eq!(s.tabs[1].x - s.tabs[0].w, 7, "the row's gap");
        assert!(s.width < md.width && md.width < lg.width);
        // The baseline stays centred in the tab.
        assert_eq!(lg.tabs[0].baseline, (29 - (14 + 3)) / 2 + 14);
        // Without a plate there's no padding at any size.
        let bare = layout(&t, 1.0, &sized(HudSize::Large, 0), &mut Fixed);
        assert_eq!(bare.tabs[0].w, runs(18) + 3 * 6);

        // The painter draws again when only the size changes.
        let mut p = Painter::new(Fixed);
        assert!(p.set(&t, 1.0, sized(HudSize::Medium, 90)));
        assert!(p.set(&t, 1.0, sized(HudSize::Large, 90)));
        assert_eq!(p.layout.as_ref().unwrap().height, 29 + 2 * 5);
        let b = p.compose(&[1.0; 3], &[0.0; 3]).unwrap();
        assert_eq!((b.width as i32, b.height), (lg.width, 39));
    }

    #[test]
    fn plates_follow_the_plate_setting_and_each_tab_fades_alone() {
        let t = yours();
        let mut p = Painter::new(Fixed);
        assert!(p.set(&t, 1.0, look(HudAnchor::TopRight, 90)));
        assert!(!p.set(&t, 1.0, look(HudAnchor::TopRight, 90)), "nothing to redo");
        let l = p.layout.clone().unwrap();
        let at = |b: &Bitmap, x: i32, y: i32| {
            let i = ((y * b.width as i32 + x) * 4) as usize;
            [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2], b.rgba[i + 3]]
        };
        // Inside the first tab's padding, clear of text: the plate alone, black at 62%.
        let (x, y) = (l.bleed + l.tabs[0].x + 3, l.bleed + 2);
        let full = p.compose(&[1.0, 1.0, 1.0], &[0.0; 3]).unwrap();
        assert_eq!(at(&full, x, y), [0, 0, 0, (0.9f32 * 0.69 * 255.0).round() as u8]);
        // Just changed: a grey plate, a little darker.
        let bright = p.compose(&[1.0; 3], &[1.0, 0.0, 0.0]).unwrap();
        let px = at(&bright, x, y);
        assert_eq!(px[3], ((0.9f32 * 0.69 + 0.15) * 255.0).round() as u8);
        assert!(px[0] > 0 && px[0] == px[1]);
        // The first tab faded out, the others not.
        let one = p.compose(&[0.0, 1.0, 1.0], &[0.0; 3]).unwrap();
        assert_eq!(at(&one, x, y)[3], 0);
        let (x2, y2) = (l.bleed + l.tabs[1].x + 3, l.bleed + 2);
        assert_eq!(at(&one, x2, y2), at(&full, x2, y2));

        // Below 30, no plate: only the text and its outline.
        p.set(&t, 1.0, look(HudAnchor::TopRight, 20));
        let bare = p.compose(&[1.0; 3], &[0.0; 3]).unwrap();
        let l = p.layout.clone().unwrap();
        assert_eq!(at(&bare, l.bleed + l.tabs[0].x, l.bleed + 1)[3], 0);
        // Fixed's block for "Battery:" is solid grey text; just above it the soft dark outline,
        // about 40% as CSS draws three 1px shadows, and nothing a few pixels out.
        let top = l.bleed + l.tabs[0].baseline - 12;
        assert_eq!(at(&bare, l.bleed + 4, top + 2), [0xEC, 0xEC, 0xEC, 255]);
        let edge = at(&bare, l.bleed + 4, top - 1);
        assert!((90..=115).contains(&edge[3]) && edge[0] == 0, "{edge:?}");
        assert_eq!(at(&bare, l.bleed + 4, top - 3)[3], 0);
    }

    #[test]
    fn straight_alpha_for_png() {
        let b = Bitmap { width: 1, height: 1, rgba: vec![50, 0, 100, 128] };
        assert_eq!(b.straight(), vec![100, 0, 199, 128]);
        assert_eq!(b.bgra(), vec![100, 0, 50, 128]);
    }
}
