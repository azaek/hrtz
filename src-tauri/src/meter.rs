//! Raw Input from the Zaopin mouse: the Lab page's tests (the rate, button and drag events, and
//! the connection and stroke analysis in `diag`), mousepad tracking, and double clicks over days
//! (`clicks`).
//!
//! Reports are read on a thread of their own, with a message-only window, as SDL and games with an
//! 8 kHz mode do. Read on the main window instead, they shared its thread with the web view's input
//! (Windows ties the input of a window and a child window from another process together), and with
//! the pointer over hrtz the user's 8,000 Hz arrived as under 1,000, merged.
//!
//! A moving pointer costs on its own: Windows moves the cursor and sends ordinary mouse messages
//! for every report, and the user's mouse that delivered over 7,000 a second with the pointer still
//! delivered about 2,500 while it moved over another app. Over hrtz's own page it's worse: Windows
//! marks every report as for an app in the background and hands over about 150 a second. So the
//! timing tests count in game mode (`capture`): a screen of hrtz's own takes the pointer as a game
//! does, and timing counts only while the pointer is held still there.
//!
//! Windows throttles and coalesces raw input for background listeners (since build 25272), so the
//! timing tests need hrtz in front. Pad tracking also listens in the background (RIDEV_INPUTSINK):
//! coalesced reports there still add up to the same movement, which is all the tracker needs. So
//! does the click log: every press and release still comes, in order.
//! `HRTZ_RAW_INPUT=window` reads on the main window, as before, should the thread ever misbehave.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL};
use windows::Win32::UI::Input::{
    GetRawInputBuffer, GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT,
    RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_DEVICENAME, RIM_TYPEMOUSE,
    RegisterRawInputDevices,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, CreateWindowExW, DispatchMessageW, GetClipCursor, GetCursorPos, HWND_MESSAGE, MSG, MWMO_INPUTAVAILABLE,
    MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, PostMessageW, QS_POSTMESSAGE, QS_RAWINPUT, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_INPUT,
};
use windows::core::{PCWSTR, w};

use crate::clicks::{self, ClickLog, Day};
use crate::diag::{Conn, ConnStats, Stroke, Strokes};
use crate::pad::{AppTag, PadData, PadView, Tracker};

const TICK: Duration = Duration::from_millis(100);
/// Gaps longer than this mean the mouse stopped; they are not part of the rate.
const MOVING_GAP: Duration = Duration::from_millis(12);
/// Pad data is written to disk at most this often while it changes.
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// RI_MOUSE_* down/up flags for left, right, middle, back, forward.
const BUTTON_FLAGS: [(u16, u16); 5] =
    [(0x0001, 0x0002), (0x0004, 0x0008), (0x0010, 0x0020), (0x0040, 0x0080), (0x0100, 0x0200)];
/// RI_MOUSE_WHEEL and RI_MOUSE_HWHEEL: the signed amount is in `usButtonData`.
const WHEEL: u16 = 0x0400;
const HWHEEL: u16 = 0x0800;
/// Asks the reader thread to register raw input again, for what's on now.
const WM_REGISTER: u32 = WM_APP + 1;
/// The pointer hold: free; settling (game mode's screen is up, and the pointer waits for Windows
/// to mark the reports as for hrtz in front); or held still.
const FREE: u8 = 0;
const SETTLING: u8 = 1;
const HOLDING: u8 = 2;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ButtonEvent {
    /// 0 left, 1 right, 2 middle, 3 back, 4 forward.
    pub button: u8,
    pub down: bool,
    /// Microseconds since the listener started.
    pub t_us: u64,
    /// On release: sensor counts moved while the button was held.
    pub dx: f64,
    pub dy: f64,
}

/// One wheel report: 120 a notch, positive away from you (scrolling up) or to the right.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WheelEvent {
    pub delta: i32,
    pub horizontal: bool,
    /// Microseconds since the listener started.
    pub t_us: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MeterSample {
    /// Reports per second while moving during the last tick, or `None` if the mouse was still.
    pub hz: Option<f32>,
    /// Sensor counts per second over the same time. Below the polling rate, some polls have
    /// nothing to report, so the report rate is limited by movement speed rather than USB.
    pub counts_per_s: Option<f32>,
    /// Signed sensor counts moved during the tick.
    pub dx: f64,
    pub dy: f64,
    /// Movement reports during the tick, and the counts they carried (|dx| + |dy|).
    pub reports: u32,
    pub travel: f64,
    /// Gaps of 2 ms or more (and at least four polls) in movement fast enough to fill every poll,
    /// decided during the tick. A gap followed by a burst of reports is the app catching up on
    /// queued input, not the mouse, and isn't counted.
    pub stutters: u32,
    /// The connection test run so far (see `diag::Conn`), since the stream started or
    /// `reset_run`.
    pub conn: ConnStats,
    /// Strokes that ended during the tick.
    pub strokes: Vec<Stroke>,
    pub buttons: Vec<ButtonEvent>,
    pub wheel: Vec<WheelEvent>,
    /// The app's window is in front. Behind others, Windows hands it reports about 125 times a
    /// second, merged, so the rate, the connection run and strokes wait; clicks, the wheel and
    /// distances still count.
    pub foreground: bool,
    /// Game mode's screen holds the pointer still, as a game does, and timing counts.
    pub held: bool,
}

struct Counts {
    devices: HashMap<isize, bool>,
    last: Option<Instant>,
    moving_time: Duration,
    reports: u32,
    distance: f64,
    dx: f64,
    dy: f64,
    /// Every movement report in the tick, moving or not, and the counts they carried.
    tick_reports: u32,
    tick_travel: f64,
    /// Counts moved since the listener started, for drag distances.
    total: (f64, f64),
    pressed: [Option<(f64, f64)>; 5],
    buttons: Vec<ButtonEvent>,
    wheel: Vec<WheelEvent>,
    conn: Conn,
    strokes: Strokes,
}

impl Counts {
    /// Nothing counted yet, measured against the rate and DPI in use.
    fn new(devices: HashMap<isize, bool>) -> Self {
        let hz = RATE.load(Ordering::SeqCst);
        Self {
            devices,
            last: None,
            moving_time: Duration::ZERO,
            reports: 0,
            distance: 0.0,
            dx: 0.0,
            dy: 0.0,
            tick_reports: 0,
            tick_travel: 0.0,
            total: (0.0, 0.0),
            pressed: [None; 5],
            buttons: Vec::new(),
            wheel: Vec::new(),
            conn: Conn::new(hz),
            strokes: Strokes::new(dpi_or_default(), hz),
        }
    }
}

impl Counts {
    /// The window went behind another: timing stops counting until it's back.
    fn behind(&mut self) {
        self.conn.pause();
        self.strokes.drop_current();
        self.last = None;
    }
}

/// The DPI strokes are measured at, before the mouse has said.
fn dpi_or_default() -> u32 {
    match DPI.load(Ordering::SeqCst) {
        0 => 1600,
        dpi => dpi,
    }
}

/// Seconds since the listener started.
fn secs(now: Instant) -> f64 {
    EPOCH.get().map_or(0.0, |e| (now - *e).as_secs_f64())
}

static COUNTS: OnceLock<Mutex<Counts>> = OnceLock::new();
static PAD: OnceLock<Mutex<Tracker>> = OnceLock::new();
static PAD_PATH: OnceLock<PathBuf> = OnceLock::new();
static CLICK_LOG: OnceLock<Mutex<ClickLog>> = OnceLock::new();
static CLICKS_PATH: OnceLock<PathBuf> = OnceLock::new();
static EPOCH: OnceLock<Instant> = OnceLock::new();
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// The reader thread's message-only window, where raw input arrives.
static READER: AtomicIsize = AtomicIsize::new(0);
/// Raw input is read on the main window instead (`HRTZ_RAW_INPUT=window`).
static ON_MAIN: AtomicBool = AtomicBool::new(false);
/// A timing test is open: its timing counts only while game mode holds the pointer.
static HOLD_WANTED: AtomicBool = AtomicBool::new(false);
/// The pointer hold (FREE, SETTLING, HOLDING), and where it's held.
static HOLD: AtomicU8 = AtomicU8::new(FREE);
static HELD_X: AtomicI32 = AtomicI32::new(0);
static HELD_Y: AtomicI32 = AtomicI32::new(0);
/// Reports Windows marked as for hrtz in front, since settling began.
static FRONT_REPORTS: AtomicU32 = AtomicU32::new(0);
/// The test or a tool is on screen.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// The newest `set_running` and `set_hold` the page sent that were applied, by its numbering (see
/// `newest`).
static RUNNING_SEQ: AtomicU64 = AtomicU64::new(0);
static HOLD_SEQ: AtomicU64 = AtomicU64::new(0);
/// The main window is the one in front, checked every tick.
static FOREGROUND: AtomicBool = AtomicBool::new(true);
/// Pad tracking is on, in the background too.
static TRACKING: AtomicBool = AtomicBool::new(false);
/// Double clicks are counted over days, in the background too.
static CLICKS: AtomicBool = AtomicBool::new(false);
/// The polling rate stutters are measured against.
static RATE: AtomicU32 = AtomicU32::new(1000);
/// The DPI in use, to turn counts into centimetres for the pad.
static DPI: AtomicU32 = AtomicU32::new(0);
/// Lift-off is 1 mm or more, so the sensor tracks for a moment while the mouse rises.
static LIFT_OFF_HIGH: AtomicBool = AtomicBool::new(false);
/// The game in focus, if it's one the pad map records in.
static APP: Mutex<Option<AppTag>> = Mutex::new(None);

/// Start reading raw input and the 100 ms sampler. Must run on the main window's thread.
/// `pad_file` keeps pad data across restarts.
pub fn attach(hwnd: isize, pad_file: PathBuf, emit: impl Fn(MeterSample) + Send + 'static) {
    let _ = EPOCH.set(Instant::now());
    let _ = COUNTS.set(Mutex::new(Counts::new(HashMap::new())));
    let mut tracker = Tracker::new(PadData::load(&pad_file));
    tracker.blips = LIFT_OFF_HIGH.load(Ordering::SeqCst);
    tracker.set_app(APP.lock().clone());
    let _ = PAD.set(Mutex::new(tracker));
    let clicks_file = pad_file.with_file_name("clicks.json");
    let _ = CLICK_LOG.set(Mutex::new(ClickLog::load(&clicks_file)));
    let _ = CLICKS_PATH.set(clicks_file);
    let _ = PAD_PATH.set(pad_file);
    MAIN_HWND.store(hwnd, Ordering::SeqCst);
    if std::env::var("HRTZ_RAW_INPUT").is_ok_and(|v| v.eq_ignore_ascii_case("window")) {
        ON_MAIN.store(true, Ordering::SeqCst);
        unsafe {
            let _ = SetWindowSubclass(HWND(hwnd as *mut _), Some(subclass), 0x5A0F, 0);
        }
    } else {
        thread::Builder::new().name("hrtz-input".into()).spawn(reader).expect("spawn input thread");
    }
    thread::Builder::new()
        .name("hrtz-meter".into())
        .spawn(move || {
            let mut last_save = Instant::now();
            loop {
                thread::sleep(TICK);
                if TRACKING.load(Ordering::SeqCst)
                    && let Some(p) = PAD.get()
                {
                    p.lock().tick(Instant::now());
                }
                if last_save.elapsed() >= SAVE_EVERY {
                    save_pad();
                    save_clicks();
                    last_save = Instant::now();
                }
                if !RUNNING.load(Ordering::SeqCst) {
                    continue;
                }
                let fg = crate::win::foreground_window();
                let front = fg == MAIN_HWND.load(Ordering::SeqCst) || (fg != 0 && fg == crate::capture::hwnd());
                if FOREGROUND.swap(front, Ordering::SeqCst)
                    && !front
                    && let Some(c) = COUNTS.get()
                {
                    c.lock().behind();
                }
                let Some(c) = COUNTS.get() else { continue };
                let mut c = c.lock();
                let now = secs(Instant::now());
                c.conn.idle(now);
                c.strokes.idle(now);
                // Need a meaningful slice of movement before reporting a rate.
                let moving = c.moving_time >= Duration::from_millis(20);
                let moving_s = c.moving_time.as_secs_f32();
                let hz = moving.then(|| c.reports as f32 / moving_s);
                let counts_per_s = moving.then(|| c.distance as f32 / moving_s);
                let (dx, dy, reports, travel) = (c.dx, c.dy, c.tick_reports, c.tick_travel);
                let stutters = c.conn.take_stutters();
                let conn = c.conn.snapshot();
                let strokes = c.strokes.take();
                let buttons = std::mem::take(&mut c.buttons);
                let wheel = std::mem::take(&mut c.wheel);
                c.reports = 0;
                c.moving_time = Duration::ZERO;
                c.distance = 0.0;
                c.dx = 0.0;
                c.dy = 0.0;
                c.tick_reports = 0;
                c.tick_travel = 0.0;
                drop(c);

                // Never hold the pointer for something that isn't in front.
                if HOLD.load(Ordering::SeqCst) != FREE && !front {
                    let_go();
                }

                // The run's pattern is read here, without holding up the reports.
                let conn = conn.stats();
                let foreground = FOREGROUND.load(Ordering::SeqCst);
                let held = HOLD.load(Ordering::SeqCst) == HOLDING;
                emit(MeterSample {
                    hz,
                    counts_per_s,
                    dx,
                    dy,
                    reports,
                    travel,
                    stutters,
                    conn,
                    strokes,
                    buttons,
                    wheel,
                    foreground,
                    held,
                });
            }
        })
        .expect("spawn meter thread");
}

/// Reads raw input on a thread of its own, at the highest priority so report times stay close to
/// when the reports came: a message-only window, woken by raw input, which it then reads in
/// batches (Microsoft's buffered read, as SDL does).
fn reader() {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
        let window = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Message"),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("raw input window: {e}");
                return;
            }
        };
        READER.store(window.0 as isize, Ordering::SeqCst);
        register_on(window.0 as isize);
        // Blocks are 8-byte aligned; 64 of them at a time.
        let mut buf = vec![0u64; 64 * std::mem::size_of::<RAWINPUT>() / 8];
        let mut msg = MSG::default();
        loop {
            // Everything but raw input: raw input stays queued for the batch read.
            while PeekMessageW(&mut msg, None, 0, WM_INPUT - 1, PM_REMOVE).as_bool()
                || PeekMessageW(&mut msg, None, WM_INPUT + 1, 0xFFFF, PM_REMOVE).as_bool()
            {
                if msg.message == WM_REGISTER {
                    register_on(window.0 as isize);
                } else {
                    DispatchMessageW(&msg);
                }
            }
            drain(&mut buf);
            match HOLD.load(Ordering::Relaxed) {
                // Moved off hrtz's window: once Windows marks a report as for hrtz in front again,
                // hold the pointer still where it is, and count from there.
                SETTLING if FRONT_REPORTS.load(Ordering::Relaxed) > 0 => {
                    let mut c = POINT::default();
                    if GetCursorPos(&mut c).is_ok() && clip_at(c.x, c.y, 1) {
                        HELD_X.store(c.x, Ordering::SeqCst);
                        HELD_Y.store(c.y, Ordering::SeqCst);
                        if HOLD.compare_exchange(SETTLING, HOLDING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                            if let Some(counts) = COUNTS.get() {
                                counts.lock().conn.pause();
                            }
                        } else {
                            // It was let go meanwhile.
                            let _ = ClipCursor(None);
                        }
                    }
                }
                // Something else can undo the clip: put it back.
                HOLDING if !still_held() => {
                    clip_at(HELD_X.load(Ordering::Relaxed), HELD_Y.load(Ordering::Relaxed), 1);
                }
                _ => {}
            }
            // Wakes for raw input already waiting too, so none sits unread.
            MsgWaitForMultipleObjectsEx(None, 50, QS_RAWINPUT | QS_POSTMESSAGE, MWMO_INPUTAVAILABLE);
        }
    }
}

/// Read every raw input waiting, a batch at a time.
unsafe fn drain(buf: &mut Vec<u64>) {
    let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;
    loop {
        let mut size = (buf.len() * 8) as u32;
        let n = unsafe { GetRawInputBuffer(Some(buf.as_mut_ptr().cast()), &mut size, header) };
        if n == 0 {
            return;
        }
        if n == u32::MAX {
            // Too small for the next block.
            if buf.len() * 8 >= 1 << 20 {
                return;
            }
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        let now = Instant::now();
        let mut at = buf.as_ptr() as usize;
        for _ in 0..n {
            let raw = unsafe { &*(at as *const RAWINPUT) };
            // RIM_INPUT, not RIM_INPUTSINK: Windows counts hrtz as in front for this report.
            if raw.header.wParam.0 & 0xff == 0 {
                FRONT_REPORTS.fetch_add(1, Ordering::Relaxed);
            }
            handle(now, raw);
            // NEXTRAWINPUTBLOCK: the next block starts at the next 8-byte boundary.
            at = (at + raw.header.dwSize as usize + 7) & !7;
        }
    }
}

/// One raw input report, wherever it was read.
fn handle(now: Instant, raw: &RAWINPUT) {
    let running = RUNNING.load(Ordering::Relaxed);
    let tracking = TRACKING.load(Ordering::Relaxed);
    let counting = CLICKS.load(Ordering::Relaxed);
    if !(running || tracking || counting) || raw.header.dwType != RIM_TYPEMOUSE.0 {
        return;
    }
    let Some(c) = COUNTS.get() else { return };
    let mut c = c.lock();
    let key = raw.header.hDevice.0 as isize;
    if !*c.devices.entry(key).or_insert_with(|| is_zaopin(raw.header.hDevice)) {
        return;
    }
    let m = unsafe { raw.data.mouse };
    let (dx, dy) = if m.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 == 0 { (m.lLastX, m.lLastY) } else { (0, 0) };
    let flags = unsafe { m.Anonymous.Anonymous.usButtonFlags };
    let data = unsafe { m.Anonymous.Anonymous.usButtonData } as i16;
    if running {
        c.record(now, dx, dy, flags, data);
    }
    drop(c);
    if counting
        && flags & clicks::BUTTON_BITS != 0
        && let Some(log) = CLICK_LOG.get()
    {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        log.lock().report(&date, secs(now), flags);
    }
    if tracking && let Some(p) = PAD.get() {
        p.lock().report(now, dx, dy, DPI.load(Ordering::Relaxed));
    }
}

/// Clip the pointer to a box `size` pixels wide around `x`, `y`.
fn clip_at(x: i32, y: i32, size: i32) -> bool {
    let (left, top) = (x - size / 2, y - size / 2);
    unsafe { ClipCursor(Some(&RECT { left, top, right: left + size, bottom: top + size })).is_ok() }
}

/// The pointer is still held where `hold` put it.
fn still_held() -> bool {
    let mut r = RECT::default();
    unsafe { GetClipCursor(&mut r).is_ok() && r.right - r.left == 1 && r.bottom - r.top == 1 }
}

fn let_go() {
    HOLD.store(FREE, Ordering::SeqCst);
    unsafe {
        let _ = ClipCursor(None);
    }
}

/// Game mode's screen is up: a new run starts, so nothing carries over from a run stopped with Esc
/// or the moment after one finished, and the reader holds the pointer still once a report comes
/// marked as for hrtz in front.
pub fn capture_started() {
    reset_run();
    FRONT_REPORTS.store(0, Ordering::SeqCst);
    HOLD.store(SETTLING, Ordering::SeqCst);
}

/// Game mode's screen closed.
pub fn capture_ended() {
    if HOLD.load(Ordering::SeqCst) != FREE {
        let_go();
    }
}

/// Whether the page's request numbered `seq` is the newest yet. The page can send on, off and on in
/// a blink (React mounts a page twice in development), and requests can arrive out of order, so an
/// older one would undo a newer: the newest sent wins. hrtz's own requests carry no number and
/// always apply. Both callers run on the main thread.
fn newest(last: &AtomicU64, seq: Option<u64>) -> bool {
    seq.is_none_or(|s| last.fetch_max(s, Ordering::SeqCst) <= s)
}

/// A timing test is open (or closed): while it is, its timing counts only in game mode.
pub fn set_hold(on: bool, seq: Option<u64>) {
    if !newest(&HOLD_SEQ, seq) {
        return;
    }
    HOLD_WANTED.store(on, Ordering::SeqCst);
    if !on && HOLD.load(Ordering::SeqCst) != FREE {
        let_go();
    }
}

/// Register raw mouse input for what's on, on the reader thread (or the main window).
fn register() {
    if ON_MAIN.load(Ordering::SeqCst) {
        register_on(MAIN_HWND.load(Ordering::SeqCst));
        return;
    }
    // Registering is done by the thread whose window gets the input. Before it has a window, it
    // registers for what's on once it has.
    let reader = READER.load(Ordering::SeqCst);
    if reader != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(reader as *mut _)), WM_REGISTER, WPARAM(0), LPARAM(0));
        }
    }
}

/// Register raw mouse input to `hwnd` for what's on: background for pad tracking and the click
/// log, foreground for the tests, or nothing.
fn register_on(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    let on = TRACKING.load(Ordering::SeqCst) || RUNNING.load(Ordering::SeqCst) || CLICKS.load(Ordering::SeqCst);
    let dev = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        // INPUTSINK: pad tracking and the click log listen in the background too. In front,
        // reports come in full.
        dwFlags: if on { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if on { HWND(hwnd as *mut _) } else { HWND::default() },
    };
    unsafe {
        let _ = RegisterRawInputDevices(&[dev], std::mem::size_of::<RAWINPUTDEVICE>() as u32);
    }
}

/// Start or stop the test and tools stream. Must run on the main window's thread.
pub fn set_running(on: bool, seq: Option<u64>) {
    if !newest(&RUNNING_SEQ, seq) {
        return;
    }
    if !on && HOLD.load(Ordering::SeqCst) != FREE {
        let_go();
    }
    if RUNNING.swap(on, Ordering::SeqCst) == on {
        return;
    }
    if let Some(c) = COUNTS.get() {
        let mut c = c.lock();
        let devices = std::mem::take(&mut c.devices);
        *c = Counts::new(devices);
    }
    register();
}

/// Start a new connection test run, and forget the stroke in progress. The stream carries on. A
/// new polling rate (`set_rate`) starts a new run too.
pub fn reset_run() {
    if let Some(c) = COUNTS.get() {
        let mut c = c.lock();
        c.conn.reset();
        c.strokes.clear();
    }
}

/// Turn background pad tracking on or off. Must run on the window's thread.
pub fn set_tracking(on: bool) {
    if TRACKING.swap(on, Ordering::SeqCst) != on {
        register();
    }
    if !on {
        save_pad();
    }
}

pub fn set_rate(hz: u32) {
    RATE.store(hz.max(1), Ordering::SeqCst);
    if let Some(c) = COUNTS.get() {
        let mut c = c.lock();
        c.conn.set_rate(hz);
        c.strokes.set(dpi_or_default(), hz);
    }
}

pub fn set_dpi(dpi: u32) {
    DPI.store(dpi, Ordering::SeqCst);
    if let Some(c) = COUNTS.get() {
        c.lock().strokes.set(dpi_or_default(), RATE.load(Ordering::SeqCst));
    }
}

/// The game in focus; `None` when what's in focus isn't one the pad map records in.
pub fn set_app(app: Option<AppTag>) {
    *APP.lock() = app.clone();
    if let Some(p) = PAD.get() {
        p.lock().set_app(app);
    }
}

pub fn set_lift_off(high: bool) {
    LIFT_OFF_HIGH.store(high, Ordering::SeqCst);
    if let Some(p) = PAD.get() {
        p.lock().blips = high;
    }
}

pub fn pad_view(app: Option<&str>) -> Option<PadView> {
    Some(PAD.get()?.lock().view(TRACKING.load(Ordering::SeqCst), app))
}

pub fn pad_calibrate(cal: crate::pad::Calibration) {
    if let Some(p) = PAD.get() {
        p.lock().calibrate(cal);
    }
    save_pad();
}

/// Count double clicks over days, or stop. Must run on the window's thread.
pub fn set_clicks(on: bool) {
    if CLICKS.swap(on, Ordering::SeqCst) != on {
        register();
    }
    if !on {
        save_clicks();
    }
}

/// The click log's days, oldest first.
pub fn click_days() -> Vec<Day> {
    CLICK_LOG.get().map(|l| l.lock().days()).unwrap_or_default()
}

/// Write the click log if it changed since the last save.
pub fn save_clicks() {
    let (Some(log), Some(path)) = (CLICK_LOG.get(), CLICKS_PATH.get()) else { return };
    let mut log = log.lock();
    if log.dirty
        && let Err(e) = log.save(path)
    {
        eprintln!("click log not saved: {e}");
    }
}

pub fn pad_clear() {
    if let Some(p) = PAD.get() {
        p.lock().clear();
    }
    save_pad();
}

/// Write pad data if it changed since the last save.
pub fn save_pad() {
    let (Some(p), Some(path)) = (PAD.get(), PAD_PATH.get()) else { return };
    let data = {
        let mut p = p.lock();
        if !p.dirty {
            return;
        }
        p.dirty = false;
        p.data.clone()
    };
    if let Err(e) = data.save(path) {
        eprintln!("saving pad data failed: {e:#}");
    }
}

fn is_zaopin(handle: HANDLE) -> bool {
    let mut len = 0u32;
    unsafe {
        GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut len);
        let mut buf = vec![0u16; len as usize + 1];
        if GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, Some(buf.as_mut_ptr().cast()), &mut len) == u32::MAX {
            return false;
        }
        String::from_utf16_lossy(&buf).to_ascii_lowercase().contains("vid_3554")
    }
}

impl Counts {
    fn record(&mut self, now: Instant, dx: i32, dy: i32, flags: u16, data: i16) {
        if flags != 0 {
            let t_us = EPOCH.get().map_or(0, |e| (now - *e).as_micros() as u64);
            if flags & (WHEEL | HWHEEL) != 0 {
                self.wheel.push(WheelEvent { delta: data as i32, horizontal: flags & HWHEEL != 0, t_us });
            }
            for (i, (down, up)) in BUTTON_FLAGS.into_iter().enumerate() {
                if flags & down != 0 {
                    self.pressed[i] = Some(self.total);
                    self.buttons.push(ButtonEvent { button: i as u8, down: true, t_us, dx: 0.0, dy: 0.0 });
                }
                if flags & up != 0 {
                    let (sx, sy) = self.pressed[i].take().unwrap_or(self.total);
                    let (ex, ey) = (self.total.0 + dx as f64, self.total.1 + dy as f64);
                    self.buttons.push(ButtonEvent { button: i as u8, down: false, t_us, dx: ex - sx, dy: ey - sy });
                }
            }
        }
        if dx == 0 && dy == 0 {
            return;
        }
        let counts = (dx.abs() + dy.abs()) as f64;
        self.total.0 += dx as f64;
        self.total.1 += dy as f64;
        self.dx += dx as f64;
        self.dy += dy as f64;
        self.tick_reports += 1;
        self.tick_travel += counts;
        if !FOREGROUND.load(Ordering::Relaxed) {
            return;
        }
        let t = secs(now);
        // When a timing test is open, its timing counts only while game mode holds the pointer:
        // elsewhere the reports come merged, or throttled.
        if !HOLD_WANTED.load(Ordering::Relaxed) || HOLD.load(Ordering::Relaxed) == HOLDING {
            self.conn.push(t, counts);
            self.strokes.push(t, dx as f64, dy as f64);
        }
        if let Some(prev) = self.last {
            let gap = now - prev;
            if gap <= MOVING_GAP {
                self.moving_time += gap;
                self.reports += 1;
                // A poll produces a report when either axis moved, so |dx| + |dy| bounds the report count.
                self.distance += counts;
            }
        }
        self.last = Some(now);
    }
}

/// The main window's raw input, when it reads it (`HRTZ_RAW_INPUT=window`).
unsafe extern "system" fn subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_INPUT
        && (RUNNING.load(Ordering::Relaxed) || TRACKING.load(Ordering::Relaxed) || CLICKS.load(Ordering::Relaxed))
    {
        let mut raw = RAWINPUT::default();
        let mut size = std::mem::size_of::<RAWINPUT>() as u32;
        let got = unsafe {
            GetRawInputData(
                HRAWINPUT(lp.0 as *mut _),
                RID_INPUT,
                Some((&mut raw as *mut RAWINPUT).cast()),
                &mut size,
                std::mem::size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        if got != u32::MAX {
            handle(Instant::now(), &raw);
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}
