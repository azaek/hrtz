//! Game mode: a screen of hrtz's own that takes the pointer the way a game does, for the Lab page's
//! timing tests.
//!
//! Over hrtz's page, Windows hands hrtz about 150 reports a second, every one marked as for an app
//! in the background: the page is a window of the web view's process, and with the pointer on it
//! Windows takes that process as the one in use. Over windows that aren't the page, and with the
//! pointer held still, hrtz got every report (7,949 a second at 8,000 Hz). Moving the pointer
//! aside by code, or while it was clipped, didn't change Windows' mind (the user's logs,
//! 2026-10-17).
//!
//! So the timing tests count on this screen. It covers hrtz's monitor and takes the keyboard; the
//! pointer is hidden over it, and once a report comes marked as for hrtz in front, the meter holds
//! the pointer still (`meter::capture_started`). The page sends what to show. Esc, switching
//! away, or the page closing it ends it, and the pointer goes back where it was.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteDC,
    DeleteObject, DrawTextW, EndPaint, FillRect, FillRgn, GdiFlush, GetMonitorInfoW, GetTextExtentPoint32W, InvalidateRect,
    MonitorFromWindow, SelectObject, SetBkMode, SetTextAlign, SetTextCharacterExtra, SetTextColor, TextOutW, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HBITMAP, HDC, HFONT, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, OUT_TT_PRECIS, PAINTSTRUCT, SRCCOPY, TA_BASELINE, TEXT_ALIGN_OPTIONS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, PostMessageW,
    PostQuitMessage, RegisterClassExW, SetCursor, SetCursorPos, SetForegroundWindow, SetTimer, ShowWindow, TranslateMessage, MSG,
    SW_SHOW, WM_ACTIVATE, WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::hud::draw::{emblem, fill_polygons, EMBLEM_W};

/// What the screen shows, sent by the page.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// What to do, big.
    pub title: String,
    /// A line under it.
    pub detail: String,
    /// The figure in the middle ("7,946") and what it is ("reports a second").
    pub figure: String,
    pub figure_label: String,
    /// How far along, 0 to 1, and a line under the bar.
    pub progress: f32,
    pub progress_label: String,
}

/// Why the screen closed.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Closed {
    /// The page closed it: the test has what it needs.
    Done,
    /// Esc.
    Esc,
    /// Another window was brought in front.
    Away,
}

/// Ask the screen's thread to close it; `wparam` is the reason.
const WM_CLOSE_SCREEN: u32 = WM_APP + 1;
/// Redraw for new text.
const WM_REDRAW: u32 = WM_APP + 2;
/// The longest a screen stays up, in case nothing closes it.
const LONGEST_MS: u32 = 3 * 60 * 1000;
const TIMER_LONGEST: usize = 1;

struct Open {
    screen: Screen,
    /// Where the pointer was, to put it back.
    home: POINT,
    /// hrtz's main window, brought back to the front afterwards.
    main: isize,
    /// It has been in front: losing that ends it.
    active: bool,
    on_close: Box<dyn Fn(Closed) + Send>,
}

static OPEN: Mutex<Option<Open>> = Mutex::new(None);
static HWND_NOW: AtomicIsize = AtomicIsize::new(0);

/// The screen's window while it's up, or 0.
pub fn hwnd() -> isize {
    HWND_NOW.load(Ordering::SeqCst)
}

/// Show the screen over hrtz's monitor (or change what an open one shows). `on_close` hears why
/// it closed.
pub fn open(screen: Screen, main: isize, on_close: impl Fn(Closed) + Send + 'static) {
    {
        let mut open = OPEN.lock();
        if let Some(o) = open.as_mut() {
            o.screen = screen;
            drop(open);
            redraw();
            return;
        }
        let mut home = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut home);
        }
        *open = Some(Open { screen, home, main, active: false, on_close: Box::new(on_close) });
    }
    thread::Builder::new().name("hrtz-capture".into()).spawn(run).expect("spawn capture thread");
}

/// New text for the open screen.
pub fn update(screen: Screen) {
    if let Some(o) = OPEN.lock().as_mut() {
        if o.screen == screen {
            return;
        }
        o.screen = screen;
    } else {
        return;
    }
    redraw();
}

/// Close the screen, if it's up: the test is done.
pub fn close() {
    let h = hwnd();
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE_SCREEN, WPARAM(Closed::Done as usize), LPARAM(0));
        }
    }
}

fn redraw() {
    let h = hwnd();
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_REDRAW, WPARAM(0), LPARAM(0));
        }
    }
}

fn run() {
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: w!("hrtzGameScreen"),
            ..Default::default()
        };
        // A second registration fails harmlessly: the class is already there.
        let _ = RegisterClassExW(&class);

        let main = OPEN.lock().as_ref().map_or(0, |o| o.main);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(MonitorFromWindow(HWND(main as *mut _), MONITOR_DEFAULTTONEAREST), &mut info);
        let r = info.rcMonitor;
        let Ok(window) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("hrtzGameScreen"),
            w!("hrtz"),
            WS_POPUP,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            finish(Closed::Away);
            return;
        };
        HWND_NOW.store(window.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
        let _ = SetTimer(Some(window), TIMER_LONGEST, LONGEST_MS, None);
        // The pointer to the middle, hidden there; the meter holds it once Windows hands the mouse
        // to this window.
        let _ = SetCursorPos((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        crate::meter::capture_started();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// After the window is gone: let go of the pointer, put it back, tell the page.
fn finish(why: Closed) {
    HWND_NOW.store(0, Ordering::SeqCst);
    crate::meter::capture_ended();
    if let Some(o) = OPEN.lock().take() {
        unsafe {
            let _ = SetCursorPos(o.home.x, o.home.y);
        }
        (o.on_close)(why);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // No pointer over the screen, as in a game.
        WM_SETCURSOR => {
            unsafe { SetCursor(None) };
            LRESULT(1)
        }
        WM_KEYDOWN if wp.0 == 0x1B => {
            end(hwnd, Closed::Esc);
            LRESULT(0)
        }
        WM_ACTIVATE => {
            let active = wp.0 & 0xFFFF != 0;
            let was = OPEN.lock().as_mut().map(|o| {
                let was = o.active;
                o.active = was || active;
                was
            });
            if !active && was == Some(true) {
                end(hwnd, Closed::Away);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            end(hwnd, Closed::Esc);
            LRESULT(0)
        }
        WM_REDRAW => {
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_CLOSE_SCREEN => {
            let why = if wp.0 == Closed::Done as usize { Closed::Done } else { Closed::Esc };
            end(hwnd, why);
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Close the window: hrtz back in front first (unless the user went elsewhere), then let go.
/// Destroying the window deactivates it, which comes back here; only the first call counts.
fn end(hwnd: HWND, why: Closed) {
    if HWND_NOW.compare_exchange(hwnd.0 as isize, 0, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return;
    }
    let main = OPEN.lock().as_ref().map_or(0, |o| o.main);
    unsafe {
        if why != Closed::Away && main != 0 {
            let _ = SetForegroundWindow(HWND(main as *mut _));
        }
        let _ = DestroyWindow(hwnd);
    }
    finish(why);
}

// ---- Drawing --------------------------------------------------------------------------------------

const BACKGROUND: COLORREF = rgb(12, 12, 12);
const TEXT: COLORREF = rgb(242, 242, 242);
const MUTED: COLORREF = rgb(160, 160, 160);
const TRACK: COLORREF = rgb(56, 56, 56);
const FILL: COLORREF = rgb(222, 222, 222);
/// The wordmark's font size, at 100%.
const BRAND: f32 = 24.0;
/// The emblem's seam at its narrowest, in its 10 px box (`brand.tsx`'s 8.4 of 246).
const SEAM: f32 = 0.341;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

fn paint(hwnd: HWND) {
    let screen = OPEN.lock().as_ref().map(|o| o.screen.clone()).unwrap_or_default();
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        // Drawn off screen, then copied: no flicker as the figures change.
        if let Some(frame) = Frame::new(Some(dc), w, h) {
            draw(&frame, GetDpiForWindow(hwnd).max(96) as f32 / 96.0, &screen);
            let _ = BitBlt(dc, 0, 0, w, h, Some(frame.dc), 0, 0, SRCCOPY);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

/// An off-screen bitmap to draw into, its pixels at hand for the emblem.
struct Frame {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    /// BGRA, top row first.
    bits: *mut u32,
    w: i32,
    h: i32,
}

impl Frame {
    fn new(like: Option<HDC>, w: i32, h: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(like);
            if dc.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bmp.into());
            Some(Self { dc, bmp, old, bits: bits.cast(), w, h })
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// The screen at `s` times 100%.
fn draw(f: &Frame, s: f32, screen: &Screen) {
    let (dc, w, h) = (f.dc, f.w, f.h);
    let px = |v: f32| (v * s).round() as i32;
    unsafe {
        let back = CreateSolidBrush(BACKGROUND);
        FillRect(dc, &RECT { left: 0, top: 0, right: w, bottom: h }, back);
        let _ = DeleteObject(back.into());
        SetBkMode(dc, TRANSPARENT);

        // hrtz's wordmark at the top, as far down as "Esc to stop" is up.
        wordmark(f, px(BRAND), px(64.0), TEXT);

        // A column in the middle of the screen, a little above centre.
        let mid = h / 2 - px(40.0);
        line(dc, &screen.title, px(30.0), true, TEXT, w, mid - px(150.0));
        line(dc, &screen.detail, px(16.0), false, MUTED, w, mid - px(108.0));
        line(dc, &screen.figure, px(84.0), true, TEXT, w, mid);
        line(dc, &screen.figure_label, px(15.0), false, MUTED, w, mid + px(62.0));

        // The bar.
        let (bw, bh) = (px(420.0), px(4.0).max(2));
        let (bx, by) = ((w - bw) / 2, mid + px(120.0));
        bar(dc, bx, by, bw, bh, TRACK);
        let done = (screen.progress.clamp(0.0, 1.0) * bw as f32).round() as i32;
        if done > 0 {
            bar(dc, bx, by, done.max(bh), bh, FILL);
        }
        line(dc, &screen.progress_label, px(14.0), false, MUTED, w, by + px(30.0));
        line(dc, "Esc to stop", px(14.0), false, MUTED, w, h - px(64.0));
        let _ = GdiFlush();
    }
}

/// One line of text centred across `width`, its middle at `y`.
unsafe fn line(dc: HDC, text: &str, size: i32, strong: bool, color: COLORREF, width: i32, y: i32) {
    if text.is_empty() {
        return;
    }
    unsafe {
        let font = font(size, strong);
        let old = SelectObject(dc, font.into());
        SetTextColor(dc, color);
        let mut buf: Vec<u16> = text.encode_utf16().collect();
        let mut r = RECT { left: 0, top: y - size, right: width, bottom: y + size };
        DrawTextW(dc, &mut buf, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
}

/// hrtz's wordmark as the app writes it (`brand.tsx`): the emblem standing in for the h, 0.75 em
/// tall on the baseline, then "rtz" in the semibold face, 0.06 em after it and a little tight
/// (-0.02 em). Centred across the frame, the emblem's middle at `y`. GDI has no smooth edges for
/// shapes, so the emblem's coverage is worked out as the HUD's is, and blended into the pixels.
unsafe fn wordmark(f: &Frame, size: i32, y: i32, color: COLORREF) {
    let (dc, width, height) = (f.dc, f.w, f.h);
    let eh = (size as f32 * 0.75).round().max(1.0);
    let k = eh / 10.0;
    // Below 32 px the seam would close up: each half moves out to keep it about a pixel wide.
    let spread = if eh > 32.0 { 0.0 } else { ((1.1 / k - SEAM) / 2.0).max(0.0) };
    let ew = (EMBLEM_W + 2.0 * spread) * k;
    let gap = 0.06 * size as f32;
    unsafe {
        let font = font(size, true);
        let old = SelectObject(dc, font.into());
        let extra = SetTextCharacterExtra(dc, -(0.02 * size as f32).round() as i32);
        let text: Vec<u16> = "rtz".encode_utf16().collect();
        let mut extent = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &text, &mut extent);
        let x = ((width as f32 - (ew + gap + extent.cx as f32)) / 2.0).round() as i32;
        let baseline = y + (eh / 2.0).round() as i32;

        // The emblem, in a box a pixel bigger all round. The short stroke (the first) is the
        // right half.
        let (bx, by, bw, bh) = (x - 1, baseline - eh as i32 - 1, ew.ceil() as i32 + 3, eh as i32 + 2);
        let polys: Vec<Vec<(f32, f32)>> = emblem()
            .into_iter()
            .enumerate()
            .map(|(i, poly)| {
                let shift = if i == 0 { 2.0 * spread } else { 0.0 };
                poly.into_iter().map(|(u, v)| (1.0 + (u + shift) * k, 1.0 + v * k)).collect()
            })
            .collect();
        let cover = fill_polygons(&polys, bw, bh);
        let _ = GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(f.bits, (width * height).max(0) as usize);
        let channel = |v: u32, shift: u32| (v >> shift & 0xFF) as f32;
        // The bitmap is BGRA; a COLORREF is red first.
        let (r, g, b) = (channel(color.0, 0), channel(color.0, 8), channel(color.0, 16));
        for j in 0..bh {
            for i in 0..bw {
                let (px, py, c) = (bx + i, by + j, cover[(j * bw + i) as usize]);
                if c <= 0.0 || px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                let p = &mut pixels[(py * width + px) as usize];
                let mix = |under: f32, over: f32| (under + (over - under) * c).round() as u32;
                *p = mix(channel(*p, 16), r) << 16 | mix(channel(*p, 8), g) << 8 | mix(channel(*p, 0), b);
            }
        }

        // "rtz" on the same baseline.
        let align = SetTextAlign(dc, TA_BASELINE);
        SetTextColor(dc, color);
        let _ = TextOutW(dc, x + (ew + gap).round() as i32, baseline, &text);
        SetTextAlign(dc, TEXT_ALIGN_OPTIONS(align));
        SetTextCharacterExtra(dc, extra);
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
}

/// Windows 11's own face, as the HUD uses; GDI falls back to Segoe UI where it's missing.
unsafe fn font(size: i32, strong: bool) -> HFONT {
    let (face, weight) = if strong { (w!("Segoe UI Variable Display Semib"), FW_SEMIBOLD) } else { (w!("Segoe UI Variable Text"), FW_NORMAL) };
    unsafe {
        CreateFontW(
            -size,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_DONTCARE.0 as u32,
            PCWSTR(face.as_ptr()),
        )
    }
}

/// A bar with round ends.
unsafe fn bar(dc: HDC, x: i32, y: i32, width: i32, height: i32, color: COLORREF) {
    unsafe {
        let region = CreateRoundRectRgn(x, y, x + width + 1, y + height + 1, height, height);
        let brush = CreateSolidBrush(color);
        let _ = FillRgn(dc, region, brush);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(region.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The screen as drawn, to look at: `cargo test --lib game_screen_png -- --ignored` writes it
    /// at 100% and 150% to the temp folder, with the wordmark four times over beside each.
    #[test]
    #[ignore]
    fn game_screen_png() {
        let screen = Screen {
            title: "Move the mouse in quick circles".into(),
            detail: "Run 1. Only fast movement counts, and 10 seconds of it makes a run.".into(),
            figure: "7,946".into(),
            figure_label: "reports a second".into(),
            progress: 0.4,
            progress_label: "4.0 of 10 s counted".into(),
        };
        for (w, h, s) in [(1920, 1080, 1.0), (3840, 2160, 1.5)] {
            let frame = Frame::new(None, w, h).expect("frame");
            draw(&frame, s, &screen);
            let pixels = unsafe { std::slice::from_raw_parts(frame.bits, (w * h) as usize) };
            let rgba = |p: u32| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255];
            let all: Vec<u8> = pixels.iter().flat_map(|p| rgba(*p)).collect();
            let name = format!("hrtz-game-screen-{}", (s * 100.0) as i32);
            let out = std::env::temp_dir();
            std::fs::write(out.join(format!("{name}.png")), crate::win::encode_png(w as u32, h as u32, &all).unwrap()).unwrap();
            // The wordmark's corner of the screen, four times over.
            let (cw, ch, zoom) = ((200.0 * s) as i32, (110.0 * s) as i32, 4);
            let (cx, cy) = ((w - cw) / 2, 0);
            let mut big = Vec::new();
            for y in 0..ch * zoom {
                for x in 0..cw * zoom {
                    big.extend(rgba(pixels[((cy + y / zoom) * w + cx + x / zoom) as usize]));
                }
            }
            let png = crate::win::encode_png((cw * zoom) as u32, (ch * zoom) as u32, &big).unwrap();
            std::fs::write(out.join(format!("{name}-mark.png")), png).unwrap();
            println!("{}", out.join(name).display());
        }
    }
}
