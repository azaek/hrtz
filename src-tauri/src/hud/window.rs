//! The HUD's window and the thread that owns it. Every Win32 call on the window happens here;
//! other threads queue an `Input` and post `WM_APP` to wake the loop.
//!
//! The window is a layered popup presented with `UpdateLayeredWindow` from a premultiplied BGRA
//! DIB. Fades change the window's constant alpha where they can, so a fade in or out of the
//! whole HUD reuses the same pixels; only a tab fading on its own, or a plate brightening, draws
//! again. A timer runs only while something animates, and one more wakes the thread when the
//! plan next changes.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromWindow, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowRect,
    KillTimer, PeekMessageW, PostMessageW, RegisterClassExW, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, HWND_TOPMOST, MA_NOACTIVATE, MSG, PM_NOREMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, ULW_ALPHA, WM_APP, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_MOUSEACTIVATE,
    WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use super::draw::{Look, Painter};
use super::model::{self, Facts, Field, Link, Moments, Plan, Scene, Tab, DIM};
use super::text::Gdi;
use super::{Input, Report, Shared};
use crate::device::DeviceState;
use crate::store::{HudShow, Prefs};

/// Something was queued for the HUD.
const WM_WAKE: u32 = WM_APP;
/// The displays, their scaling or the work area changed.
const WM_PLACE: u32 = WM_APP + 1;

const TIMER_FRAME: usize = 1;
const TIMER_WAKE: usize = 2;

const FRAME_MS: u32 = 16;
const FADE_IN_MS: f32 = 150.0;
const FADE_OUT_MS: f32 = 200.0;
const BRIGHT_FADE_MS: f32 = 300.0;
const DIM_FADE_MS: f32 = 400.0;
/// How often to look again for exclusive fullscreen while a game is in front: a game can switch
/// to it after it starts.
const EXCLUSIVE_EVERY_MS: u64 = 5000;

pub(super) fn run(rx: Receiver<Input>, shared: Arc<Shared>, report: Box<dyn Fn(Report) + Send>, prefs: Prefs, done: Sender<()>) {
    unsafe {
        // Physical pixels everywhere, so the HUD is placed and sized right on every monitor.
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Make the thread's queue before anyone can post to it.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        shared.thread.store(GetCurrentThreadId(), Ordering::SeqCst);
    }
    let Some(hwnd) = create_window() else {
        eprintln!("hud: couldn't create its window");
        let _ = done.send(());
        return;
    };
    let Some(gdi) = Gdi::new() else {
        eprintln!("hud: couldn't make a DC for its text");
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        let _ = done.send(());
        return;
    };
    shared.hwnd.store(hwnd.0 as isize, Ordering::SeqCst);
    let mut hud = State::new(hwnd, shared, report, prefs, gdi);
    if hud.drain(&rx) {
        hud.update(false);
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            match msg.message {
                WM_WAKE => {
                    if !hud.drain(&rx) {
                        break;
                    }
                    hud.update(false);
                }
                WM_PLACE => {
                    hud.place = None;
                    hud.update(false);
                }
                WM_TIMER if msg.hwnd == hwnd => hud.timer(msg.wParam.0),
                _ => unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                },
            }
        }
    }
    hud.close();
    let _ = done.send(());
}

fn create_window() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("hrtzHUD"),
            ..Default::default()
        };
        RegisterClassExW(&class);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("hrtzHUD"),
            w!("hrtz HUD"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()
    }
}

/// Only what Windows sends straight to the window lands here; the loop handles the rest. Display
/// changes are posted back to the loop, so the HUD's state is only ever touched there.
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
            let _ = unsafe { PostMessageW(Some(hwnd), WM_PLACE, WPARAM(0), LPARAM(0)) };
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// A value easing toward a target: Edge's CSS transitions.
#[derive(Clone, Copy, Debug)]
struct Anim {
    from: f32,
    to: f32,
    start: u64,
    ms: f32,
    /// Ease-in on the way down (a tab fading out), ease-out otherwise.
    ease_in_down: bool,
}

impl Anim {
    fn at(v: f32, ease_in_down: bool) -> Self {
        Self { from: v, to: v, start: 0, ms: 0.0, ease_in_down }
    }

    fn value(&self, now: u64) -> f32 {
        if self.ms <= 0.0 {
            return self.to;
        }
        let p = ((now.saturating_sub(self.start)) as f32 / self.ms).clamp(0.0, 1.0);
        let e = if self.to < self.from && self.ease_in_down { p * p } else { 1.0 - (1.0 - p) * (1.0 - p) };
        self.from + (self.to - self.from) * e
    }

    fn done(&self, now: u64) -> bool {
        self.ms <= 0.0 || now >= self.start + self.ms as u64
    }

    /// Head for `to`, from wherever it is now, taking the share of the full time that's left.
    fn toward(&mut self, to: f32, now: u64, up_ms: f32, down_ms: f32) {
        if (to - self.to).abs() < 1e-6 {
            return;
        }
        let v = self.value(now);
        let full = if to > v { up_ms } else { down_ms };
        *self = Self { from: v, to, start: now, ms: full * (to - v).abs(), ease_in_down: self.ease_in_down };
    }
}

struct TabAnim {
    alpha: Anim,
    bright: Anim,
}

/// Where the HUD goes: the area it sits in and the monitor's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    area: RECT,
    scale: f32,
}

/// The DIB the window's pixels come from.
struct Surface {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Surface {
    fn new(w: i32, h: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let bi = BITMAPINFO {
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
            let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bmp.into());
            Some(Self { dc, bmp, old, bits: bits.cast(), w, h })
        }
    }

    fn write(&mut self, bgra: &[u8]) {
        let n = (self.w * self.h * 4) as usize;
        if bgra.len() == n {
            unsafe { std::ptr::copy_nonoverlapping(bgra.as_ptr(), self.bits, n) };
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

struct State {
    hwnd: HWND,
    clock: Instant,
    shared: Arc<Shared>,
    report: Box<dyn Fn(Report) + Send>,
    own_exe: String,
    prefs: Prefs,
    device: DeviceState,
    vendor_app: bool,
    facts: Facts,
    moments: Moments,
    /// The app in front, and the game the HUD counts it as (exe in lower case).
    foreground: Option<String>,
    game: Option<String>,
    exclusive: bool,
    exclusive_checked: Option<u64>,
    exclusive_reported: bool,
    /// When the HUD page last asked for its preview (see `model::TEST_MS`).
    test_at: Option<u64>,
    painter: Painter<Gdi>,
    anims: HashMap<Field, TabAnim>,
    dim: Anim,
    place: Option<Place>,
    surface: Option<Surface>,
    /// What's on screen: shown at all, the position, the window's alpha, and what the pixels
    /// were drawn from (each tab's share of the alpha and its plate's brightness, in 1/255ths).
    shown: bool,
    pos: POINT,
    alpha: u8,
    drawn: Option<(Vec<u8>, Vec<u8>)>,
    animating: bool,
    /// What `Report::Showing` last said.
    reported: Option<bool>,
}

impl State {
    fn new(hwnd: HWND, shared: Arc<Shared>, report: Box<dyn Fn(Report) + Send>, prefs: Prefs, gdi: Gdi) -> Self {
        let own_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();
        let device = DeviceState::default();
        let facts = Facts::of(&device, &prefs, false, |_| None);
        Self {
            hwnd,
            clock: Instant::now(),
            shared,
            report,
            own_exe,
            prefs,
            device,
            vendor_app: false,
            facts,
            moments: Moments::default(),
            foreground: None,
            game: None,
            exclusive: false,
            exclusive_checked: None,
            exclusive_reported: false,
            test_at: None,
            painter: Painter::new(gdi),
            anims: HashMap::new(),
            dim: Anim::at(1.0, false),
            place: None,
            surface: None,
            shown: false,
            pos: POINT::default(),
            alpha: 0,
            drawn: None,
            animating: false,
            reported: None,
        }
    }

    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    /// Take in everything queued. Returns false once asked to quit.
    fn drain(&mut self, rx: &Receiver<Input>) -> bool {
        while let Ok(input) = rx.try_recv() {
            let now = self.now();
            match input {
                Input::Device(s) => {
                    self.device = *s;
                    self.refresh_facts(now);
                }
                Input::Prefs(p) => {
                    self.prefs = *p;
                    self.refresh_facts(now);
                    // Turning the HUD off ends the test, so turning it on again doesn't bring
                    // the test back.
                    if self.prefs.hud.show == HudShow::Off {
                        self.test_at = None;
                    }
                }
                Input::VendorApp(on) => {
                    self.vendor_app = on;
                    self.refresh_facts(now);
                }
                // Each ask starts the 5 minutes again. While the HUD is off the test does
                // nothing, and isn't kept for later.
                Input::Test(on) => self.test_at = (on && self.prefs.hud.show != HudShow::Off).then_some(now),
                Input::Foreground(path) => {
                    self.foreground = path;
                    // A new window in front goes over the HUD unless it's raised again.
                    if self.shown {
                        self.raise();
                    }
                }
                Input::Installed => {}
                Input::Quit => return false,
            }
        }
        true
    }

    fn refresh_facts(&mut self, now: u64) {
        let shared = self.shared.clone();
        let next = Facts::of(&self.device, &self.prefs, self.vendor_app, |hz| shared.life_h(hz));
        self.moments.compare(&self.facts, &next, now);
        self.facts = next;
    }

    /// A game hrtz knows: one `games.rs` names, one installed through a launcher, one on the pad
    /// map's list, or one with a profile rule.
    fn is_game(&self, exe: &str) -> bool {
        crate::games::find(exe).is_some()
            || self.shared.installed.read().contains(&exe.to_lowercase())
            || self.prefs.pad_game(exe).is_some()
            || self.prefs.rules.iter().any(|r| r.exe.eq_ignore_ascii_case(exe))
    }

    fn timer(&mut self, id: usize) {
        match id {
            TIMER_FRAME => self.update(true),
            TIMER_WAKE => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), TIMER_WAKE);
                }
                self.update(false);
            }
            _ => {}
        }
    }

    fn report_status(&mut self) {
        if self.reported != Some(self.shown) {
            self.reported = Some(self.shown);
            self.shared.showing.store(self.shown, Ordering::SeqCst);
            (self.report)(Report::Showing(self.shown));
        }
    }

    /// Work out what should be up now, get the window there, and set the timers.
    fn update(&mut self, frame: bool) {
        let now = self.now();
        let on = self.prefs.hud.show != HudShow::Off;

        // Is a game the HUD shows in at the front? Every tab shows for a moment when one comes
        // to the front.
        let exe = self.foreground.as_deref().map(crate::win::file_name).filter(|e| !e.eq_ignore_ascii_case(&self.own_exe));
        let game = exe.filter(|e| self.is_game(e)).map(|e| e.to_lowercase());
        if game != self.game {
            if game.is_some() {
                self.moments.game(now);
            }
            self.game = game;
            self.exclusive = false;
            self.exclusive_checked = None;
            self.exclusive_reported = false;
        }
        let in_game = on && self.game.as_deref().is_some_and(|g| !self.prefs.hud.hidden(g));
        let hud = &self.prefs.hud;
        // The HUD page's preview gives way to a game in front: there the HUD is the real one, or
        // none in a game it stays off in. It comes back when the game goes.
        let test_at = self.test_at.filter(|_| self.game.is_none());
        let scene = Scene { show: hud.show, oled_care: hud.oled_care, in_game, test_at, now };
        // The preview shows changes as a game would, so the old pop-up stays away then too.
        self.shared.in_game.store(in_game || scene.testing(), Ordering::SeqCst);

        // Windows reports "D3D fullscreen" for true exclusive fullscreen, where no other window can
        // draw, but likely also for games it runs borderless behind the scenes (fullscreen
        // optimisations), where the HUD draws fine. So the HUD still tries to show; the report
        // only lets the HUD page say it may not show there, and what to do if it doesn't.
        if in_game && self.exclusive_checked.is_none_or(|t| now >= t + EXCLUSIVE_EVERY_MS) {
            self.exclusive_checked = Some(now);
            self.exclusive = unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN);
            if self.exclusive && !self.exclusive_reported {
                self.exclusive_reported = true;
                if let (Some(exe), Some(path)) = (self.game_exe(), self.foreground.clone()) {
                    (self.report)(Report::Exclusive { exe, path });
                }
            }
        }

        // Without the receiver the HUD has nothing to say, so the page's preview shows a sample
        // mouse.
        let sample = (scene.testing() && self.facts.link == Link::NoReceiver).then(|| Facts::sample(&self.prefs));
        let tabs = model::tabs(sample.as_ref().unwrap_or(&self.facts), &self.prefs.hud.fields);
        let plan = model::plan(&tabs, &scene, &self.moments);
        self.present(now, &tabs, &plan, frame);

        // Wake again when the plan changes by itself, or to look for exclusive fullscreen.
        let check = self.exclusive_checked.filter(|_| in_game).map(|t| t + EXCLUSIVE_EVERY_MS);
        let next = [plan.next, check].into_iter().flatten().min();
        unsafe {
            match next {
                Some(t) => {
                    SetTimer(Some(self.hwnd), TIMER_WAKE, t.saturating_sub(now).clamp(10, u32::MAX as u64) as u32, None);
                }
                None => {
                    let _ = KillTimer(Some(self.hwnd), TIMER_WAKE);
                }
            }
            if self.animating {
                SetTimer(Some(self.hwnd), TIMER_FRAME, FRAME_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_FRAME);
            }
        }
    }

    /// The exe in front as it's spelled, for the HUD page.
    fn game_exe(&self) -> Option<String> {
        self.foreground.as_deref().map(crate::win::file_name)
    }

    fn present(&mut self, now: u64, tabs: &[Tab], plan: &Plan, frame: bool) {
        // Head every tab toward showing or not, and the whole HUD toward dimmed or not.
        let fields: Vec<Field> = tabs.iter().map(|t| t.field).collect();
        self.anims.retain(|f, _| fields.contains(f));
        for (i, f) in fields.iter().enumerate() {
            let a = self.anims.entry(*f).or_insert_with(|| TabAnim { alpha: Anim::at(0.0, true), bright: Anim::at(0.0, false) });
            a.alpha.toward(if plan.visible[i] { 1.0 } else { 0.0 }, now, FADE_IN_MS, FADE_OUT_MS);
            a.bright.toward(if plan.bright[i] { 1.0 } else { 0.0 }, now, BRIGHT_FADE_MS, BRIGHT_FADE_MS);
        }
        self.dim.toward(if plan.dim { DIM } else { 1.0 }, now, DIM_FADE_MS, DIM_FADE_MS);
        let alpha: Vec<f32> = fields.iter().map(|f| self.anims[f].alpha.value(now)).collect();
        let bright: Vec<f32> = fields.iter().map(|f| self.anims[f].bright.value(now)).collect();
        self.animating = self.anims.values().any(|a| !a.alpha.done(now) || !a.bright.done(now)) || !self.dim.done(now);

        let top = alpha.iter().copied().fold(0.0f32, f32::max);
        let wanted = plan.visible.iter().any(|v| *v);
        if top <= 0.0 && !wanted {
            self.hide();
            self.animating = false;
            return;
        }

        if !frame || self.place.is_none() || !self.shown {
            self.place = place_now().or(self.place);
        }
        // Nowhere to go or nothing to draw: wait for the next wake-up rather than spin frames.
        let Some(place) = self.place else {
            self.animating = false;
            return;
        };
        let look = Look::of(&self.prefs.hud);
        let redone = self.painter.set(tabs, place.scale, look);
        let Some(layout) = self.painter.layout.clone() else {
            self.animating = false;
            return;
        };
        if layout.width <= 0 || layout.height <= 0 {
            self.hide();
            self.animating = false;
            return;
        }

        // The window's alpha carries the fade the tabs share, and the dimming; the pixels carry
        // each tab's share of it.
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let share: Vec<u8> = alpha.iter().map(|a| if top > 0.0 { q(a / top) } else { 0 }).collect();
        let lit: Vec<u8> = bright.iter().map(|b| q(*b)).collect();
        let window_alpha = q(top * self.dim.value(now));

        let anchor = look.anchor;
        // The margin grows with the size, like everything else the HUD draws.
        let margin = layout.margin;
        let a = place.area;
        let x = if anchor.left() {
            a.left + margin
        } else if anchor.right() {
            a.right - margin - layout.inner_w
        } else {
            a.left + (a.right - a.left - layout.inner_w) / 2
        };
        let y = if anchor.top() {
            a.top + margin
        } else if anchor.bottom() {
            a.bottom - margin - layout.inner_h
        } else {
            a.top + (a.bottom - a.top - layout.inner_h) / 2
        };
        // OLED care moves the HUD in from its edges, by whole pixels of the monitor whatever the
        // size.
        let shift = (plan.shift as f32 * place.scale).round() as i32;
        let pos = POINT {
            x: x - layout.bleed + if anchor.right() { -shift } else { shift },
            y: y - layout.bleed + if anchor.bottom() { -shift } else { shift },
        };
        let size = SIZE { cx: layout.width, cy: layout.height };

        let resized = self.surface.as_ref().is_none_or(|s| s.w != size.cx || s.h != size.cy);
        let repaint = redone || resized || self.drawn.as_ref() != Some(&(share.clone(), lit.clone()));
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: window_alpha, AlphaFormat: AC_SRC_ALPHA as u8 };
        unsafe {
            if repaint {
                if resized {
                    self.surface = Surface::new(size.cx, size.cy);
                }
                let shares: Vec<f32> = share.iter().map(|s| *s as f32 / 255.0).collect();
                let lights: Vec<f32> = lit.iter().map(|b| *b as f32 / 255.0).collect();
                let (Some(surface), Some(bitmap)) = (self.surface.as_mut(), self.painter.compose(&shares, &lights)) else { return };
                surface.write(&bitmap.bgra());
                let src = POINT::default();
                let _ = UpdateLayeredWindow(
                    self.hwnd,
                    None,
                    Some(&pos as *const _),
                    Some(&size as *const _),
                    Some(surface.dc),
                    Some(&src as *const _),
                    COLORREF(0),
                    Some(&blend as *const _),
                    ULW_ALPHA,
                );
                self.drawn = Some((share, lit));
            } else if window_alpha != self.alpha || pos.x != self.pos.x || pos.y != self.pos.y {
                // Same pixels: only the fade or the position moves.
                let _ = UpdateLayeredWindow(self.hwnd, None, Some(&pos as *const _), None, None, None, COLORREF(0), Some(&blend as *const _), ULW_ALPHA);
            }
        }
        self.alpha = window_alpha;
        self.pos = pos;
        if !self.shown {
            self.shown = true;
            self.raise();
            self.report_status();
        }
    }

    /// Topmost again, over whatever just came to the front, without taking the focus.
    fn raise(&self) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
        }
    }

    fn hide(&mut self) {
        if self.shown {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.shown = false;
            self.drawn = None;
            self.alpha = 0;
            self.report_status();
        }
    }

    fn close(&mut self) {
        // Other threads stop posting to the window before it goes, so none can reach a handle
        // Windows has given to another window since.
        self.shared.hwnd.store(0, Ordering::SeqCst);
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
        self.shared.in_game.store(false, Ordering::SeqCst);
        self.shared.showing.store(false, Ordering::SeqCst);
    }
}

/// The monitor the app in front is on: all of it when that app covers it (a fullscreen game),
/// else its work area, so the HUD stays off the taskbar.
fn place_now() -> Option<Place> {
    unsafe {
        let fg = GetForegroundWindow();
        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return None;
        }
        let m = mi.rcMonitor;
        let mut r = RECT::default();
        let covers = !fg.is_invalid()
            && GetWindowRect(fg, &mut r).is_ok()
            && r.left <= m.left
            && r.top <= m.top
            && r.right >= m.right
            && r.bottom >= m.bottom;
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        Some(Place { area: if covers { m } else { mi.rcWork }, scale: dx.max(48) as f32 / 96.0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Puts the real HUD on screen for a moment, as if Counter-Strike 2 were in front, checks the
    /// window shows, fades in fully and hides again, and saves what the screen showed. It does
    /// appear on this PC's screen. `cargo test --lib shows_the_window -- --ignored`; `HRTZ_OUT`
    /// names the folder for `native-on-screen.png`.
    #[test]
    #[ignore = "shows the HUD on screen"]
    fn shows_the_window_fades_and_hides() {
        use windows::Win32::Graphics::Gdi::{BitBlt, GetDC, GetDIBits, ReleaseDC, CAPTUREBLT, SRCCOPY};
        use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, PM_REMOVE};

        fn pump(hud: &mut State, ms: u64) {
            let until = Instant::now() + std::time::Duration::from_millis(ms);
            let mut msg = MSG::default();
            while Instant::now() < until {
                while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
                    if msg.message == WM_TIMER && msg.hwnd == hud.hwnd {
                        hud.timer(msg.wParam.0);
                    } else {
                        unsafe { DispatchMessageW(&msg) };
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }

        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let hwnd = create_window().unwrap();
        let shared = Arc::new(Shared {
            hwnd: Default::default(),
            thread: Default::default(),
            in_game: Default::default(),
            enabled: Default::default(),
            showing: Default::default(),
            installed: Default::default(),
            log: Default::default(),
            life: parking_lot::Mutex::new((None, Vec::new())),
        });
        let reports = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let seen = reports.clone();
        let mut prefs = Prefs::default();
        prefs.hud.show = HudShow::Always;
        let mut hud = State::new(hwnd, shared.clone(), Box::new(move |r| seen.lock().push(r)), prefs.clone(), Gdi::new().unwrap());
        hud.facts = Facts::sample(&prefs);
        hud.foreground = Some(r"C:\Games\cs2.exe".into());
        hud.update(false);
        assert!(hud.shown && unsafe { IsWindowVisible(hwnd) }.as_bool());
        assert!(shared.in_game.load(Ordering::SeqCst), "the old pop-up stays away");
        pump(&mut hud, 400);
        assert_eq!(hud.alpha, 255, "faded in");
        assert!(!hud.animating);

        // What's on screen where the window is.
        let mut r = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut r).unwrap() };
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let surface = Surface::new(w, h).unwrap();
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            let screen = GetDC(None);
            BitBlt(surface.dc, 0, 0, w, h, Some(screen), r.left, r.top, SRCCOPY | CAPTUREBLT).unwrap();
            ReleaseDC(None, screen);
            let mut bi = BITMAPINFO {
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
            GetDIBits(surface.dc, surface.bmp, 0, h as u32, Some(px.as_mut_ptr().cast()), &mut bi, DIB_RGB_COLORS);
        }
        for p in px.chunks_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        let dir = std::env::var("HRTZ_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        std::fs::write(dir.join("native-on-screen.png"), crate::win::encode_png(w as u32, h as u32, &px).unwrap()).unwrap();
        eprintln!("window {w}x{h} at {},{} ({:?}); saved {}", r.left, r.top, hud.place, dir.join("native-on-screen.png").display());

        // The game goes away: the HUD fades out and hides.
        hud.foreground = Some(r"C:\Windows\explorer.exe".into());
        hud.update(false);
        assert!(!shared.in_game.load(Ordering::SeqCst));
        pump(&mut hud, 400);
        assert!(!hud.shown && !unsafe { IsWindowVisible(hwnd) }.as_bool());

        // The HUD page's preview brings it back with no game in front, and off again.
        hud.test_at = Some(hud.now());
        hud.update(false);
        assert!(hud.shown && shared.in_game.load(Ordering::SeqCst));
        pump(&mut hud, 400);
        assert_eq!(hud.alpha, 255, "not dimmed");
        hud.test_at = None;
        hud.update(false);
        pump(&mut hud, 400);
        assert!(!hud.shown && !unsafe { IsWindowVisible(hwnd) }.as_bool());

        let said = reports.lock().clone();
        assert!(said.contains(&Report::Showing(true)), "{said:?}");
        assert_eq!(said.last(), Some(&Report::Showing(false)));
        hud.close();
    }

    #[test]
    fn fades_ease_from_where_they_are() {
        let mut a = Anim::at(0.0, true);
        a.toward(1.0, 1000, FADE_IN_MS, FADE_OUT_MS);
        assert_eq!(a.value(1000), 0.0);
        assert!(a.value(1075) > 0.5, "ease-out: most of the way at half time");
        assert_eq!(a.value(1150), 1.0);
        assert!(a.done(1150) && !a.done(1100));
        // Turned back half way: down from there, in the share of the fade-out that's left.
        let mut b = Anim::at(0.0, true);
        b.toward(1.0, 0, FADE_IN_MS, FADE_OUT_MS);
        let mid = b.value(75);
        b.toward(0.0, 75, FADE_IN_MS, FADE_OUT_MS);
        assert!((b.value(75) - mid).abs() < 1e-6, "no jump");
        assert!(b.done(75 + (FADE_OUT_MS * mid) as u64 + 1));
        assert_eq!(b.value(1000), 0.0);
        // Asking for the same target again changes nothing.
        let before = b.start;
        b.toward(0.0, 2000, FADE_IN_MS, FADE_OUT_MS);
        assert_eq!(b.start, before);
    }
}
