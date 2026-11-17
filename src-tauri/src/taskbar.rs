//! The battery on the taskbar: a mouse and its level ("93%") left of the notification area's
//! arrow, for the mouse the tray follows. A tray icon is a 16 px square, too small for a number
//! and its unit, so this is a small window of its own (DECISIONS.md, "The battery on the
//! taskbar").
//!
//! It's a child of the taskbar's own window (`Shell_TrayWnd`), as TrafficMonitor's is, so it moves
//! and hides with the taskbar, auto-hide and full-screen games included. Windows has no API for
//! this, so its place is worked out again every second from the notification area's
//! (`TrayNotifyWnd`) and the taskbar buttons' (`ReBarWindow32`) rectangles, and it steps aside
//! while the buttons reach it. Windows 11 only: Windows 10's buttons fill the taskbar up to the
//! tray.
//!
//! Its thread owns its windows and never blocks: once the widget is the taskbar's child, the
//! taskbar's thread and this one share their input.

#[cfg(windows)]
use std::sync::Arc;

use crate::tray::TrayLevel;

/// What the widget shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Show {
    /// The switch in Settings.
    pub on: bool,
    pub level: TrayLevel,
    /// The tray's tooltip, shown on hover.
    pub tip: String,
}

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The text beside the mouse: its level, or none for an asleep mouse whose level isn't known.
pub fn label(level: TrayLevel) -> Option<String> {
    match level {
        TrayLevel::Disconnected | TrayLevel::Asleep { level: None } => None,
        TrayLevel::Asleep { level: Some(l) } | TrayLevel::Battery { level: l, .. } => Some(format!("{}%", l.min(100))),
    }
}

/// Where a widget `width` wide goes: left of the notification area, `clear` pixels from its
/// window's edge (Windows 11's ^ reaches past it), the taskbar's full height. None on a vertical
/// taskbar, or where the taskbar's buttons would be under it.
pub fn spot(taskbar: Rect, notify: Rect, buttons: Option<Rect>, width: i32, clear: i32) -> Option<Rect> {
    let (w, h) = (taskbar.right - taskbar.left, taskbar.bottom - taskbar.top);
    if w <= h || width <= 0 {
        return None;
    }
    let right = notify.left.min(taskbar.right) - clear;
    let at = Rect { left: right - width, top: taskbar.top, right, bottom: taskbar.bottom };
    if at.left < taskbar.left || buttons.is_some_and(|b| b.right > at.left && b.left < at.right) {
        return None;
    }
    Some(at)
}

/// The app's handle on the widget. Every call only stores what to show and wakes its thread.
pub struct Taskbar {
    #[cfg(windows)]
    shared: Option<Arc<imp::Shared>>,
}

impl Taskbar {
    /// Starts the widget's thread, on Windows 11. `open` runs when the widget is clicked.
    pub fn start(open: impl Fn() + Send + Sync + 'static) -> Self {
        #[cfg(windows)]
        {
            Self { shared: crate::win::is_windows_11().then(|| imp::start(Box::new(open))) }
        }
        #[cfg(not(windows))]
        {
            let _ = open;
            Self {}
        }
    }

    /// Whether this Windows can show it.
    pub fn supported(&self) -> bool {
        #[cfg(windows)]
        {
            self.shared.is_some()
        }
        #[cfg(not(windows))]
        false
    }

    pub fn show(&self, show: Show) {
        #[cfg(windows)]
        if let Some(shared) = &self.shared {
            shared.show(show);
        }
        #[cfg(not(windows))]
        let _ = show;
    }

    /// Takes the widget off the taskbar, before the app quits.
    pub fn stop(&self) {
        #[cfg(windows)]
        if let Some(shared) = &self.shared {
            shared.stop();
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicIsize, Ordering};

    use parking_lot::Mutex;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, MapWindowPoints, SelectObject,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        TOOLTIPS_CLASSW, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_SETMAXTIPWIDTH, TTM_UPDATETIPTEXTW,
        TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW, WM_MOUSELEAVE,
    };
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, GetWindowDpiAwarenessContext,
        SetThreadDpiAwarenessContext,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowExW, FindWindowW,
        GW_CHILD, GetMessageW, GetWindow, GetWindowRect, HWND_MESSAGE, HWND_TOP, IDC_ARROW, IsWindow, LoadCursorW,
        MA_NOACTIVATE, MSG, PostMessageW, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SendMessageW, SetTimer, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, ULW_ALPHA,
        UpdateLayeredWindow, WINDOW_STYLE, WM_APP, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_TIMER, WNDCLASSEXW,
        WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };
    use windows::core::{PCWSTR, PWSTR, w};

    use super::{Rect, Show, label, spot};
    use crate::hud::draw::{Face, Fonts};
    use crate::hud::text::Gdi;
    use crate::tray::TrayLevel;

    /// Something new to show.
    const WM_WAKE: u32 = WM_APP;
    /// Take the widget down and end the thread.
    const WM_STOP: u32 = WM_APP + 1;
    const TIMER_PLACE: usize = 1;
    /// How often its place is looked at again: the tray grows and shrinks as icons come and go.
    const PLACE_MS: u32 = 1000;
    /// Segoe Fluent Icons' mouse.
    const MOUSE: char = '\u{E962}';
    /// Sizes at 100%, in pixels: the glyph as a tray icon, the text as the clock's.
    const GLYPH: f32 = 16.0;
    const TEXT: f32 = 12.0;
    const PAD: f32 = 8.0;
    const GAP: f32 = 5.0;
    /// The hover wash, as the taskbar's buttons have it: inset from the taskbar's edges, rounded.
    const WASH_INSET: f32 = 4.0;
    const WASH_RADIUS: f32 = 4.0;
    /// How far Windows 11's ^ reaches left of the notification area's window, at 100%.
    const CHEVRON: f32 = 8.0;
    /// Text through the same curve as the HUD's, a little lighter: the clock beside it is thin.
    const TEXT_GAMMA: f32 = 1.6;

    pub struct Shared {
        host: AtomicIsize,
        want: Mutex<Option<Show>>,
        open: Box<dyn Fn() + Send + Sync>,
    }

    impl Shared {
        pub fn show(&self, show: Show) {
            let mut want = self.want.lock();
            if want.as_ref() != Some(&show) {
                *want = Some(show);
                self.post(WM_WAKE);
            }
        }

        pub fn stop(&self) {
            self.post(WM_STOP);
        }

        fn post(&self, msg: u32) {
            let host = self.host.load(Ordering::SeqCst);
            if host != 0 {
                let _ = unsafe { PostMessageW(Some(HWND(host as *mut _)), msg, WPARAM(0), LPARAM(0)) };
            }
        }
    }

    pub fn start(open: Box<dyn Fn() + Send + Sync>) -> Arc<Shared> {
        let shared = Arc::new(Shared { host: AtomicIsize::new(0), want: Mutex::new(None), open });
        let thread_shared = shared.clone();
        std::thread::Builder::new().name("taskbar".into()).spawn(move || run(thread_shared)).ok();
        shared
    }

    fn run(shared: Arc<Shared>) {
        unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let (Some(host), Some(gdi)) = (create_host(), Gdi::new()) else {
            eprintln!("taskbar: couldn't make its window");
            return;
        };
        shared.host.store(host.0 as isize, Ordering::SeqCst);
        let mut state = State {
            shared,
            gdi,
            widget: None,
            taskbar: None,
            tip: None,
            tip_text: String::new(),
            hover: false,
            pixels: None,
            placed: None,
            presented: false,
        };
        unsafe {
            SetTimer(Some(host), TIMER_PLACE, PLACE_MS, None);
        }
        state.update();
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
            let on_widget = state.widget.is_some_and(|w| w == msg.hwnd);
            match msg.message {
                WM_STOP => break,
                WM_WAKE => state.update(),
                WM_TIMER if msg.hwnd == host => state.update(),
                WM_MOUSEMOVE if on_widget && !state.hover => {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: msg.hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut track) };
                    state.hover = true;
                    state.update();
                }
                WM_MOUSELEAVE if on_widget => {
                    state.hover = false;
                    state.update();
                }
                WM_LBUTTONUP if on_widget => (state.shared.open)(),
                _ => {}
            }
            // The tooltip watches the widget's mouse messages too, so they go on to it.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        state.remove();
        state.shared.host.store(0, Ordering::SeqCst);
        unsafe {
            let _ = DestroyWindow(host);
        }
    }

    fn create_host() -> Option<HWND> {
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let host = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(host_proc),
                hInstance: instance.into(),
                lpszClassName: w!("hrtzTaskbarHost"),
                ..Default::default()
            };
            RegisterClassExW(&host);
            let widget = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(widget_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("hrtzTaskbar"),
                ..Default::default()
            };
            RegisterClassExW(&widget);
            CreateWindowExW(
                Default::default(),
                w!("hrtzTaskbarHost"),
                w!("hrtz taskbar"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )
            .ok()
        }
    }

    unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    /// Only what Windows sends straight to the widget lands here; the loop handles the rest.
    unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            // Clicking it doesn't take the focus from the window in front.
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }

    /// What was drawn, and with what.
    #[derive(Clone, PartialEq, Debug)]
    struct Look {
        level: TrayLevel,
        light: bool,
        dpi: u32,
        height: i32,
        hover: bool,
    }

    struct Pixels {
        look: Look,
        width: i32,
        bgra: Vec<u8>,
    }

    struct State {
        shared: Arc<Shared>,
        gdi: Gdi,
        widget: Option<HWND>,
        /// The taskbar the widget is in: a new one after Explorer restarts.
        taskbar: Option<HWND>,
        tip: Option<HWND>,
        tip_text: String,
        hover: bool,
        pixels: Option<Pixels>,
        /// Where it is, in the taskbar's client pixels, and whether it's showing.
        placed: Option<(i32, i32, i32, i32)>,
        /// The pixels last drawn are the ones on screen.
        presented: bool,
    }

    impl State {
        fn update(&mut self) {
            let want = self.shared.want.lock().clone();
            let Some(show) = want.filter(|s| s.on && !matches!(s.level, TrayLevel::Disconnected)) else {
                self.remove();
                return;
            };
            let Some((taskbar, bar, notify, buttons)) = find_taskbar() else {
                self.remove();
                return;
            };
            if self.taskbar != Some(taskbar) || !self.widget.is_some_and(|w| unsafe { IsWindow(Some(w)).as_bool() }) {
                self.remove();
            }
            let dpi = unsafe { GetDpiForWindow(taskbar) }.max(96);
            let look = Look {
                level: show.level,
                light: crate::win::taskbar_is_light(),
                dpi,
                height: bar.bottom - bar.top,
                hover: self.hover,
            };
            if self.pixels.as_ref().is_none_or(|p| p.look != look) {
                let (width, bgra) = paint(&look, &mut self.gdi);
                self.pixels = Some(Pixels { look, width, bgra });
                self.presented = false;
            }
            let width = self.pixels.as_ref().map_or(0, |p| p.width);
            let clear = (CHEVRON * dpi as f32 / 96.0).round() as i32;
            let Some(at) = spot(bar, notify, buttons, width, clear) else {
                self.hide();
                return;
            };
            let Some(widget) = self.widget.or_else(|| self.create(taskbar)) else { return };
            // The taskbar's client pixels: on Windows 11 its window is the bar itself.
            let mut corner = [POINT { x: at.left, y: at.top }];
            unsafe { MapWindowPoints(None, Some(taskbar), &mut corner) };
            let placed = (corner[0].x, corner[0].y, at.right - at.left, at.bottom - at.top);
            unsafe {
                if self.placed != Some(placed) {
                    let _ = SetWindowPos(
                        widget,
                        Some(HWND_TOP),
                        placed.0,
                        placed.1,
                        placed.2,
                        placed.3,
                        SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    self.presented = false;
                    self.placed = Some(placed);
                } else if GetWindow(taskbar, GW_CHILD).ok() != Some(widget) {
                    // Above the taskbar's own content, which it may have raised.
                    let _ = SetWindowPos(widget, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE);
                }
            }
            if !self.presented
                && let Some(p) = &self.pixels
            {
                present(widget, p.width, placed.3, &p.bgra);
                self.presented = true;
            }
            if self.tip_text != show.tip {
                self.tip_text = show.tip.clone();
                self.set_tip();
            }
        }

        fn create(&mut self, taskbar: HWND) -> Option<HWND> {
            let widget = unsafe {
                let instance = GetModuleHandleW(None).ok()?;
                // Windows makes a child only in its parent's DPI mode: the taskbar's, here.
                let mode = GetWindowDpiAwarenessContext(taskbar);
                SetThreadDpiAwarenessContext(mode);
                CreateWindowExW(
                    WS_EX_LAYERED,
                    w!("hrtzTaskbar"),
                    w!("hrtz battery"),
                    WS_CHILD | WS_CLIPSIBLINGS,
                    0,
                    0,
                    1,
                    1,
                    Some(taskbar),
                    None,
                    Some(instance.into()),
                    None,
                )
                .inspect_err(|e| eprintln!("taskbar: couldn't make the widget: {e}"))
                .ok()?
            };
            self.widget = Some(widget);
            self.taskbar = Some(taskbar);
            self.placed = None;
            self.presented = false;
            self.tip = create_tip(widget);
            self.tip_text.clear();
            Some(widget)
        }

        fn set_tip(&mut self) {
            let (Some(tip), Some(widget)) = (self.tip, self.widget) else { return };
            let mut text: Vec<u16> = self.tip_text.encode_utf16().chain(std::iter::once(0)).collect();
            let info = tool(widget, PWSTR(text.as_mut_ptr()));
            unsafe {
                SendMessageW(tip, TTM_UPDATETIPTEXTW, Some(WPARAM(0)), Some(LPARAM(&info as *const _ as isize)));
                // What a screen reader says for it.
                let _ = SetWindowTextW(widget, PCWSTR(text.as_ptr()));
            }
        }

        fn hide(&mut self) {
            if let Some(widget) = self.widget
                && self.placed.is_some()
            {
                unsafe {
                    let _ = ShowWindow(widget, SW_HIDE);
                }
                self.placed = None;
            }
        }

        fn remove(&mut self) {
            unsafe {
                if let Some(tip) = self.tip.take() {
                    let _ = DestroyWindow(tip);
                }
                if let Some(widget) = self.widget.take() {
                    let _ = DestroyWindow(widget);
                }
            }
            self.taskbar = None;
            self.placed = None;
            self.presented = false;
            self.hover = false;
        }
    }

    /// The primary taskbar and the rectangles that place the widget: the bar, its notification
    /// area and its buttons.
    fn find_taskbar() -> Option<(HWND, Rect, Rect, Option<Rect>)> {
        unsafe {
            let taskbar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
            let notify = FindWindowExW(Some(taskbar), None, w!("TrayNotifyWnd"), PCWSTR::null()).ok()?;
            let buttons = FindWindowExW(Some(taskbar), None, w!("ReBarWindow32"), PCWSTR::null()).ok();
            Some((taskbar, rect_of(taskbar)?, rect_of(notify)?, buttons.and_then(rect_of)))
        }
    }

    fn rect_of(hwnd: HWND) -> Option<Rect> {
        let mut r = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut r) }.ok()?;
        Some(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom })
    }

    fn tool(widget: HWND, text: PWSTR) -> TTTOOLINFOW {
        TTTOOLINFOW {
            // The size before `lpReserved`, which every version of the control takes.
            cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: widget,
            uId: widget.0 as usize,
            lpszText: text,
            ..Default::default()
        }
    }

    /// A tooltip for the widget, with the tray's text.
    fn create_tip(widget: HWND) -> Option<HWND> {
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let tip = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            let mut empty = [0u16];
            let info = tool(widget, PWSTR(empty.as_mut_ptr()));
            SendMessageW(tip, TTM_ADDTOOLW, Some(WPARAM(0)), Some(LPARAM(&info as *const _ as isize)));
            // A width makes it break lines at the text's newlines, one mouse to a line.
            SendMessageW(tip, TTM_SETMAXTIPWIDTH, Some(WPARAM(0)), Some(LPARAM(800)));
            Some(tip)
        }
    }

    /// Put `bgra` (premultiplied, `w` x `h`) on the widget.
    fn present(widget: HWND, w: i32, h: i32, bgra: &[u8]) {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return;
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
            let bmp: HBITMAP = match CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return;
                }
            };
            let n = (w * h * 4) as usize;
            if bgra.len() == n {
                std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), n);
            }
            let old: HGDIOBJ = SelectObject(dc, bmp.into());
            let size = SIZE { cx: w, cy: h };
            let src = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                widget,
                None::<HDC>,
                None,
                Some(&size as *const _),
                Some(dc),
                Some(&src as *const _),
                COLORREF(0),
                Some(&blend as *const _),
                ULW_ALPHA,
            );
            SelectObject(dc, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(dc);
        }
    }

    /// The widget's pixels, premultiplied BGRA, and its width: the hover wash, the mouse, the
    /// level.
    fn paint(look: &Look, gdi: &mut Gdi) -> (i32, Vec<u8>) {
        let s = look.dpi as f32 / 96.0;
        let px = |v: f32| (v * s).round() as i32;
        let h = look.height.max(1);
        let mono = if look.light { [0, 0, 0] } else { [255, 255, 255] };
        let (_, tone, strength) = crate::tray::logo_state(look.level, mono);
        let (glyph, text_px, pad, gap) = (px(GLYPH), px(TEXT), px(PAD), px(GAP));
        let text = label(look.level);
        let text_w = text.as_deref().map_or(0, |t| gdi.width(t, text_px, Face::Regular).ceil() as i32);
        let w = pad + glyph + if text_w > 0 { gap + text_w } else { 0 } + pad;
        let mut rgba = vec![0.0f32; (w * h * 4) as usize];

        if look.hover {
            let wash = if look.light { 0.05 } else { 0.06 };
            let (inset, r) = (WASH_INSET * s, WASH_RADIUS * s);
            for y in 0..h {
                for x in 0..w {
                    let a = rounded_cover(x as f32 + 0.5, y as f32 + 0.5, 0.0, inset, w as f32, h as f32 - inset, r);
                    over(&mut rgba, (y * w + x) as usize, mono, a * wash);
                }
            }
        }
        if let Some(alpha) = crate::win::glyph_alpha(MOUSE, glyph) {
            let (x0, y0) = (pad, (h - glyph) / 2);
            for gy in 0..glyph {
                for gx in 0..glyph {
                    let a = alpha[(gy * glyph + gx) as usize] as f32 / 255.0;
                    let (x, y) = (x0 + gx, y0 + gy);
                    if a > 0.0 && (0..w).contains(&x) && (0..h).contains(&y) {
                        over(&mut rgba, (y * w + x) as usize, tone, a * strength);
                    }
                }
            }
        }
        if let Some(text) = text {
            let (ascent, descent) = gdi.metrics(text_px, Face::Regular);
            let baseline = (h - (ascent + descent)) / 2 + ascent;
            let cov = gdi.coverage(&text, text_px, Face::Regular, text_w, h, 0.0, baseline);
            let x0 = pad + glyph + gap;
            for y in 0..h {
                for tx in 0..text_w {
                    let c = cov[(y * text_w + tx) as usize] as f32 / 255.0;
                    if c > 0.0 {
                        over(&mut rgba, (y * w + x0 + tx) as usize, tone, c.powf(1.0 / TEXT_GAMMA) * strength);
                    }
                }
            }
        }
        let bgra = rgba
            .chunks(4)
            .flat_map(|p| {
                let a = p[3];
                let ch = |c: f32| (c * a).round().clamp(0.0, 255.0) as u8;
                [ch(p[2]), ch(p[1]), ch(p[0]), (a * 255.0).round() as u8]
            })
            .collect();
        (w, bgra)
    }

    /// How much of the pixel centred at (`x`, `y`) a rounded rectangle covers.
    fn rounded_cover(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, r: f32) -> f32 {
        let cx = x.clamp(left + r, right - r);
        let cy = y.clamp(top + r, bottom - r);
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        (r + 0.5 - d).clamp(0.0, 1.0)
    }

    /// Lay colour `c` at `alpha` over pixel `i`, straight alpha, colour in 0-255.
    fn over(rgba: &mut [f32], i: usize, c: [u8; 3], alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let px = &mut rgba[i * 4..i * 4 + 4];
        let below = px[3];
        let a = alpha + below * (1.0 - alpha);
        for k in 0..3 {
            px[k] = (c[k] as f32 * alpha + px[k] * below * (1.0 - alpha)) / a;
        }
        px[3] = a;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAR: Rect = Rect { left: 0, top: 2111, right: 3840, bottom: 2160 };
    const NOTIFY: Rect = Rect { left: 3501, top: 2111, right: 3840, bottom: 2160 };

    #[test]
    fn the_label_is_the_level() {
        assert_eq!(label(TrayLevel::Battery { level: 93, charging: false, low_at: 15 }).as_deref(), Some("93%"));
        assert_eq!(label(TrayLevel::Asleep { level: Some(40) }).as_deref(), Some("40%"));
        assert_eq!(label(TrayLevel::Asleep { level: None }), None);
        assert_eq!(label(TrayLevel::Disconnected), None);
    }

    #[test]
    fn it_sits_against_the_notification_area() {
        let buttons = Rect { left: 1113, top: 2111, right: 2496, bottom: 2160 };
        let at = spot(BAR, NOTIFY, Some(buttons), 80, 8).unwrap();
        assert_eq!(at, Rect { left: 3413, top: 2111, right: 3493, bottom: 2160 });
    }

    #[test]
    fn it_steps_aside_for_the_buttons_and_a_vertical_taskbar() {
        // Buttons reaching the tray, as with many apps open on a left-aligned taskbar.
        let full = Rect { left: 0, top: 2111, right: 3450, bottom: 2160 };
        assert_eq!(spot(BAR, NOTIFY, Some(full), 80, 8), None);
        let vertical = Rect { left: 0, top: 0, right: 62, bottom: 2160 };
        assert_eq!(spot(vertical, Rect { left: 0, top: 1800, right: 62, bottom: 2160 }, None, 80, 8), None);
    }
}
