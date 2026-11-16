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
//!
//! For the blind test it's an arena too (`Screen::arena`): hrtz draws a dot to follow and a
//! crosshair the mouse moves, every frame of the screen, the crosshair from the mouse's own reports
//! as a game reads them, and says how closely it followed (`arena_take`). Clicks and a few keys go
//! back to the page (`on_input`): left or 1 for the first run, right or 2 for the second, Enter or
//! Space to go on.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::thread;
use std::time::Instant;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D_SIZE_U, D2D1_COLOR_F};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
    D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget,
    ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DEFAULT_CHARSET,
    DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, FillRect, FillRgn, GdiFlush, GetMonitorInfoW, GetTextExtentPoint32W, HBITMAP,
    HDC, HFONT, HGDIOBJ, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, OUT_TT_PRECIS,
    PAINTSTRUCT, SRCCOPY, SelectObject, SetBkMode, SetTextAlign, SetTextCharacterExtra, SetTextColor, TA_BASELINE,
    TEXT_ALIGN_OPTIONS, TRANSPARENT, TextOutW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, MSG,
    PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW, SetCursor, SetCursorPos, SetForegroundWindow, SetTimer,
    ShowWindow, TranslateMessage, WM_ACTIVATE, WM_APP, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_PAINT,
    WM_RBUTTONDOWN, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

use crate::hud::draw::{EMBLEM_W, emblem, fill_polygons};

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
    /// The blind test's arena, in place of the text: the dot to follow and the crosshair.
    #[serde(default)]
    pub arena: Option<Arena>,
}

/// The blind test's arena, as the page sets it for each part of a round.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Arena {
    /// The dot is moving, and how closely the crosshair follows it counts. Before a run it waits
    /// where it will start, faint, with the crosshair on it.
    pub running: bool,
    /// Its path: both runs of a round follow the same one, so they compare.
    pub seed: f64,
    /// A line at the top: "Round 3 of 10 · First run".
    pub label: String,
}

/// How closely the crosshair followed the dot in the run just done, and the screen's refresh rate.
#[derive(Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArenaRun {
    /// Mean distance from the dot, as a share of the screen's height.
    pub miss: f64,
    pub frames: u32,
    pub refresh_hz: Option<u32>,
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
    /// A click or key the page may want: "left", "right", "one", "two", "next".
    on_input: Box<dyn Fn(&'static str) + Send>,
}

static OPEN: Mutex<Option<Open>> = Mutex::new(None);
static HWND_NOW: AtomicIsize = AtomicIsize::new(0);

/// The screen's window while it's up, or 0.
pub fn hwnd() -> isize {
    HWND_NOW.load(Ordering::SeqCst)
}

/// Show the screen over hrtz's monitor (or change what an open one shows). `on_close` hears why
/// it closed, and `on_input` the clicks and keys the page may want.
pub fn open(
    screen: Screen,
    main: isize,
    on_close: impl Fn(Closed) + Send + 'static,
    on_input: impl Fn(&'static str) + Send + 'static,
) {
    let arena = screen.arena.clone();
    {
        let mut open = OPEN.lock();
        if let Some(o) = open.as_mut() {
            o.screen = screen;
            drop(open);
            set_play(arena);
            redraw();
            return;
        }
        let mut home = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut home);
        }
        *open = Some(Open {
            screen,
            home,
            main,
            active: false,
            on_close: Box::new(on_close),
            on_input: Box::new(on_input),
        });
    }
    thread::Builder::new().name("hrtz-capture".into()).spawn(run).expect("spawn capture thread");
    set_play(arena);
}

/// New text for the open screen.
pub fn update(screen: Screen) {
    let arena = screen.arena.clone();
    if let Some(o) = OPEN.lock().as_mut() {
        if o.screen == screen {
            return;
        }
        o.screen = screen;
    } else {
        return;
    }
    set_play(arena);
    redraw();
}

// ---- The blind test's arena ----------------------------------------------------------------------

/// The arena as it stands: what the page set, when the run began, and whether the crosshair goes
/// back onto the dot (a new round's run, or the second run of one).
struct Play {
    arena: Arena,
    since: Instant,
    reset: bool,
}

static PLAY: Mutex<Option<Play>> = Mutex::new(None);
/// The run being measured: the sum of misses and the frames they came from.
static RUN: Mutex<(f64, u32)> = Mutex::new((0.0, 0));
/// The last frames' lengths (ms), for the screen's refresh rate.
static FRAME_MS: Mutex<Vec<f32>> = Mutex::new(Vec::new());
static RENDERING: AtomicBool = AtomicBool::new(false);
/// The start of a run, while the hand catches the dot, isn't counted (s).
const LEAD_IN_S: f64 = 0.6;
/// A screen's height of crosshair travel takes this much mouse travel (cm): about a game's
/// middling sensitivity.
const CM_PER_HEIGHT: f64 = 15.0;

/// Where the dot is `t` seconds into a run, on a screen `w` by `h`: a slow loop that turns back on
/// itself, the same as the page's.
fn dot_at(t: f64, seed: f64, w: f64, h: f64) -> (f64, f64) {
    use std::f64::consts::TAU;
    (w / 2.0 + w * 0.36 * (TAU * 0.23 * t + seed).sin(), h / 2.0 + h * 0.3 * (TAU * 0.37 * t + seed * 1.7).sin())
}

/// The arena the page set, or none: start drawing it, change it, or stop.
fn set_play(arena: Option<Arena>) {
    let Some(a) = arena else {
        if PLAY.lock().take().is_some() {
            crate::meter::set_arena(false);
        }
        return;
    };
    let mut play = PLAY.lock();
    match play.as_mut() {
        Some(p) => {
            if a.running && !p.arena.running {
                p.since = Instant::now();
                *RUN.lock() = (0.0, 0);
            }
            if !a.running && (a.seed != p.arena.seed || p.arena.running) {
                p.reset = true;
            }
            p.arena = a;
        }
        None => {
            if a.running {
                *RUN.lock() = (0.0, 0);
            }
            *play = Some(Play { arena: a, since: Instant::now(), reset: true });
            crate::meter::set_arena(true);
        }
    }
    drop(play);
    if !RENDERING.swap(true, Ordering::SeqCst) {
        thread::Builder::new().name("hrtz-arena".into()).spawn(render).expect("spawn arena thread");
    }
}

/// How closely the crosshair followed the dot in the run just done; the count starts again.
pub fn arena_take() -> ArenaRun {
    let (sum, frames) = std::mem::take(&mut *RUN.lock());
    let mut ms = FRAME_MS.lock().clone();
    ms.sort_by(f32::total_cmp);
    let refresh_hz = ms.get(ms.len() / 2).filter(|m| **m > 0.0).map(|m| (1000.0 / m).round() as u32);
    ArenaRun { miss: if frames > 0 { sum / frames as f64 } else { 0.0 }, frames, refresh_hz }
}

/// Draw the arena until it goes; should it come back just as this thread saw it go, carry on.
fn render() {
    loop {
        render_while_up();
        RENDERING.store(false, Ordering::SeqCst);
        if PLAY.lock().is_none() || RENDERING.swap(true, Ordering::SeqCst) {
            break;
        }
    }
}

/// What the arena is drawn with: Direct2D on the screen's window, presented in step with the
/// screen's refresh, a whole frame at a time, so a frame is never seen half drawn (drawn with GDI,
/// the crosshair went missing now and then and the dot's ring was cut where it moved: the user's
/// first runs). Text with DirectWrite.
struct Painter {
    target: ID2D1HwndRenderTarget,
    size: (u32, u32),
    faint: ID2D1SolidColorBrush,
    bright: ID2D1SolidColorBrush,
    ink: ID2D1SolidColorBrush,
    muted: ID2D1SolidColorBrush,
    label: IDWriteTextFormat,
    small: IDWriteTextFormat,
}

const fn color(c: COLORREF) -> D2D1_COLOR_F {
    let v = c.0;
    D2D1_COLOR_F {
        r: (v & 0xFF) as f32 / 255.0,
        g: ((v >> 8) & 0xFF) as f32 / 255.0,
        b: ((v >> 16) & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

impl Painter {
    /// In pixels (96 DPI to Direct2D), the text at `s` times 100%.
    fn new(d2d: &ID2D1Factory, dwrite: &IDWriteFactory, hwnd: HWND, size: (u32, u32), s: f32) -> Option<Self> {
        unsafe {
            let props = D2D1_RENDER_TARGET_PROPERTIES { dpiX: 96.0, dpiY: 96.0, ..Default::default() };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width: size.0, height: size.1 },
                // Waits for the screen's refresh to show each frame.
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let target = d2d.CreateHwndRenderTarget(&props, &hwnd_props).ok()?;
            let brush = |c: COLORREF| target.CreateSolidColorBrush(&color(c), None).ok();
            let text = |px: f32| {
                let f = dwrite
                    .CreateTextFormat(
                        w!("Segoe UI"),
                        None,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        px * s,
                        w!("en-us"),
                    )
                    .ok()?;
                let _ = f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                Some(f)
            };
            Some(Self {
                faint: brush(TRACK)?,
                bright: brush(FILL)?,
                ink: brush(TEXT)?,
                muted: brush(MUTED)?,
                label: text(16.0)?,
                small: text(14.0)?,
                target,
                size,
            })
        }
    }

    /// One frame: the line at the top, Esc at the bottom, the dot and the crosshair. Waits for the
    /// screen's refresh to show it. False if the target has to be made again.
    fn frame(&self, s: f32, label: &str, dot: (f64, f64), cross: (f64, f64), running: bool) -> bool {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let px = |v: f32| v * s;
        let text = |t: &str, format: &IDWriteTextFormat, y: f32| {
            let wide: Vec<u16> = t.encode_utf16().collect();
            let rect = D2D_RECT_F { left: 0.0, top: y - px(16.0), right: w, bottom: y + px(16.0) };
            unsafe {
                self.target.DrawText(
                    &wide,
                    format,
                    &rect,
                    &self.muted,
                    D2D1_DRAW_TEXT_OPTIONS_NONE,
                    DWRITE_MEASURING_MODE_NATURAL,
                )
            };
        };
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&color(BACKGROUND)));
            text(label, &self.label, px(112.0));
            text("Esc to stop", &self.small, h - px(64.0));
            // The dot: a ring with a point in it, faint until the run starts.
            let ring = if running { &self.bright } else { &self.faint };
            let ellipse = |r: f32| {
                let mut e = D2D1_ELLIPSE { radiusX: r, radiusY: r, ..Default::default() };
                e.point.X = dot.0 as f32;
                e.point.Y = dot.1 as f32;
                e
            };
            self.target.DrawEllipse(&ellipse(px(14.0)), ring, px(2.0), None);
            self.target.FillEllipse(&ellipse(px(2.0)), ring);
            // The crosshair: two bars.
            let (cx, cy, arm, half) = (cross.0 as f32, cross.1 as f32, px(10.0), px(1.0));
            self.target.FillRectangle(
                &D2D_RECT_F { left: cx - arm, top: cy - half, right: cx + arm, bottom: cy + half },
                &self.ink,
            );
            self.target.FillRectangle(
                &D2D_RECT_F { left: cx - half, top: cy - arm, right: cx + half, bottom: cy + arm },
                &self.ink,
            );
            self.target.EndDraw(None, None).is_ok()
        }
    }
}

/// Draw the dot and the crosshair, a whole frame at each refresh of the screen, until the arena
/// goes. Then the window is painted again as text.
fn render_while_up() {
    let mut last = Instant::now();
    let mut seen = crate::meter::arena_counts();
    let mut cross: Option<(f64, f64)> = None;
    let Ok(d2d) = (unsafe { D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }) else {
        return;
    };
    let Ok(dwrite) = (unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) }) else { return };
    let mut painter: Option<Painter> = None;
    let mut window = HWND::default();
    loop {
        let h = hwnd();
        let Some((arena, since, reset)) =
            PLAY.lock().as_mut().map(|p| (p.arena.clone(), p.since, std::mem::take(&mut p.reset)))
        else {
            break;
        };
        if h == 0 {
            break;
        }
        window = HWND(h as *mut _);
        let mut rc = RECT::default();
        let s = unsafe {
            let _ = GetClientRect(window, &mut rc);
            GetDpiForWindow(window).max(96) as f32 / 96.0
        };
        let (w, ht) = ((rc.right - rc.left) as f64, (rc.bottom - rc.top) as f64);
        if w <= 0.0 || ht <= 0.0 {
            thread::sleep(std::time::Duration::from_millis(5));
            continue;
        }
        let size = (w as u32, ht as u32);
        if painter.as_ref().is_none_or(|p| p.size != size) {
            painter = Painter::new(&d2d, &dwrite, window, size, s);
        }
        let Some(p) = &painter else { break };
        let now = Instant::now();
        let frame_ms = (now - last).as_secs_f32() * 1000.0;
        last = now;
        if arena.running && frame_ms > 0.0 && frame_ms < 100.0 {
            let mut ms = FRAME_MS.lock();
            ms.push(frame_ms);
            let n = ms.len();
            if n > 240 {
                ms.drain(..n - 240);
            }
        }
        let t = if arena.running { (now - since).as_secs_f64() } else { 0.0 };
        let dot = dot_at(t, arena.seed, w, ht);
        // The crosshair moves with the mouse's counts at its DPI, a screen's height for 15 cm.
        let counts = crate::meter::arena_counts();
        let moved = ((counts.0 - seen.0) as f64, (counts.1 - seen.1) as f64);
        seen = counts;
        if reset {
            cross = Some(dot);
        }
        let per_count = ht / CM_PER_HEIGHT * 2.54 / crate::meter::dpi_in_use().max(1) as f64;
        // Clear of the line at the top and Esc at the bottom.
        let edge = 160.0 * s as f64;
        let c = cross.get_or_insert(dot);
        c.0 = (c.0 + moved.0 * per_count).clamp(edge, w - edge);
        c.1 = (c.1 + moved.1 * per_count).clamp(edge, ht - edge);
        if arena.running && t >= LEAD_IN_S {
            let mut run = RUN.lock();
            run.0 += (c.0 - dot.0).hypot(c.1 - dot.1) / ht;
            run.1 += 1;
        }
        let cross_at = *c;
        // Waits for the refresh; a lost target is made again next time.
        if !p.frame(s, &arena.label, dot, cross_at, arena.running) {
            painter = None;
        }
    }
    drop(painter);
    // The screen's words come back, painted as before.
    if !window.is_invalid() && hwnd() == window.0 as isize {
        unsafe {
            let _ = InvalidateRect(Some(window), None, false);
        }
    }
}

/// Tell the page of a click or key.
fn input(kind: &'static str) {
    if let Some(o) = OPEN.lock().as_ref() {
        (o.on_input)(kind);
    }
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
    set_play(None);
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
        // For the blind test's answers: 1 or 2 (on the number row or the pad), Enter or Space.
        WM_KEYDOWN if matches!(wp.0, 0x31 | 0x61) => {
            input("one");
            LRESULT(0)
        }
        WM_KEYDOWN if matches!(wp.0, 0x32 | 0x62) => {
            input("two");
            LRESULT(0)
        }
        WM_KEYDOWN if matches!(wp.0, 0x0D | 0x20) => {
            input("next");
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            input("left");
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            input("right");
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
        // The arena is drawn by its own thread, a whole frame at each refresh.
        if screen.arena.is_some() && PLAY.lock().is_some() {
            let _ = EndPaint(hwnd, &ps);
            return;
        }
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

        // The blind test's arena: only its line at the top and Esc at the bottom are painted;
        // the dot and the crosshair are drawn each frame over this (`render`).
        if let Some(arena) = &screen.arena {
            line(dc, &arena.label, px(16.0), false, MUTED, w, px(112.0));
            line(dc, "Esc to stop", px(14.0), false, MUTED, w, h - px(64.0));
            let _ = GdiFlush();
            return;
        }

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
    let (face, weight) = if strong {
        (w!("Segoe UI Variable Display Semib"), FW_SEMIBOLD)
    } else {
        (w!("Segoe UI Variable Text"), FW_NORMAL)
    };
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
            arena: None,
        };
        for (w, h, s) in [(1920, 1080, 1.0), (3840, 2160, 1.5)] {
            let frame = Frame::new(None, w, h).expect("frame");
            draw(&frame, s, &screen);
            let pixels = unsafe { std::slice::from_raw_parts(frame.bits, (w * h) as usize) };
            let rgba = |p: u32| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255];
            let all: Vec<u8> = pixels.iter().flat_map(|p| rgba(*p)).collect();
            let name = format!("hrtz-game-screen-{}", (s * 100.0) as i32);
            let out = std::env::temp_dir();
            std::fs::write(out.join(format!("{name}.png")), crate::win::encode_png(w as u32, h as u32, &all).unwrap())
                .unwrap();
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
