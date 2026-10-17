//! The in-game HUD: small "Label: value" tabs against an edge of the screen while a game is in
//! front (the Edge direction in DECISIONS.md, prototyped in the HUD lab, since archived).
//!
//! It's a separate native window, layered, click-through, no-activate, toolwindow and topmost,
//! owned by its own thread. Nothing is injected into the game, nothing is hooked, and no key is
//! taken: most keys a game might use are already bound, so the HUD has no hotkey.
//!
//! - `model`: what the tabs say and when they show. No Win32.
//! - `draw`: the layout and the drawing, into an RGBA bitmap.
//! - `text`: GDI text for `draw`.
//! - `window`: the window, its thread and message loop.

pub mod draw;
pub mod model;
#[cfg(windows)]
pub mod text;
#[cfg(windows)]
mod window;

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};

use crate::battery_log::{self, BatteryLog};
use crate::device::DeviceState;
use crate::store::{HudShow, Prefs};

/// The battery log's life per rate is worked out again at most this often.
const LIFE_EVERY: Duration = Duration::from_secs(5 * 60);
/// Installed games are listed again this often while the HUD is on.
const INSTALLED_EVERY: Duration = Duration::from_secs(30 * 60);

/// What the HUD tells the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Report {
    /// The game in front runs in exclusive fullscreen, where the HUD can't draw.
    Exclusive { exe: String, path: String },
    /// The HUD went on screen, or off it.
    Showing(bool),
}

enum Input {
    Device(Box<DeviceState>),
    Prefs(Box<Prefs>),
    /// The path of the app in front, sent when it or its window changes.
    Foreground(Option<String>),
    VendorApp(bool),
    /// The HUD page's preview: on (or still on), or off.
    Test(bool),
    /// The list of installed games was refreshed.
    Installed,
    Quit,
}

/// What the HUD's threads and the app's share.
struct Shared {
    /// The HUD window and its thread, for wake-ups; 0 until they exist.
    hwnd: AtomicIsize,
    thread: AtomicU32,
    /// The HUD is on and a game it shows in is in front, or the HUD page's preview is on,
    /// so the old DPI pop-up stays away.
    in_game: AtomicBool,
    /// The HUD is on at all, so installed games are worth listing.
    enabled: AtomicBool,
    /// What the last `Report::Showing` said, for the app's status before it could be told.
    showing: AtomicBool,
    /// Exes of games installed through Steam, Epic and Riot, in lower case.
    installed: RwLock<HashSet<String>>,
    log: Arc<Mutex<BatteryLog>>,
    /// When the battery log was last read, and the hours a charge lasts at each rate it knows.
    life: Mutex<(Option<Instant>, Vec<RateHours>)>,
}

/// A polling rate and the hours a full charge lasts at it.
type RateHours = (u32, f64);

impl Shared {
    /// Hours a full charge lasts at `hz`, from the battery log, worked out at most every few
    /// minutes.
    fn life_h(&self, hz: u32) -> Option<f64> {
        let mut life = self.life.lock();
        if life.0.is_none_or(|t| t.elapsed() > LIFE_EVERY) {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let summary = battery_log::summary(self.log.lock().entries(), now);
            *life = (Some(Instant::now()), summary.life.iter().map(|l| (l.hz, l.hours)).collect());
        }
        life.1.iter().find(|(h, _)| *h == hz).map(|(_, hours)| *hours)
    }
}

/// What `Hud::start` hands the threads: the HUD's input, the installed-games wake-up, the "closed"
/// signal and the prefs to start from.
type Parts = (Receiver<Input>, Receiver<()>, Sender<()>, Prefs);

/// The app's handle on the HUD. Every call only queues a message and wakes the HUD's thread.
pub struct Hud {
    tx: Sender<Input>,
    kick: Sender<()>,
    shared: Arc<Shared>,
    parts: Mutex<Option<Parts>>,
    done: Receiver<()>,
}

impl Hud {
    /// The handle, ready to queue input. Nothing runs until `start`.
    pub fn new(prefs: &Prefs, log: Arc<Mutex<BatteryLog>>) -> Self {
        let (tx, rx) = unbounded();
        let (kick, kicked) = unbounded();
        let (done_tx, done) = crossbeam_channel::bounded(1);
        let shared = Arc::new(Shared {
            hwnd: AtomicIsize::new(0),
            thread: AtomicU32::new(0),
            in_game: AtomicBool::new(false),
            enabled: AtomicBool::new(prefs.hud.show != HudShow::Off),
            showing: AtomicBool::new(false),
            installed: RwLock::new(HashSet::new()),
            log,
            life: Mutex::new((None, Vec::new())),
        });
        Self { tx, kick, shared, parts: Mutex::new(Some((rx, kicked, done_tx, prefs.clone()))), done }
    }

    /// Start the HUD's thread, and the one that lists installed games. `report` is called on the
    /// HUD's thread.
    pub fn start(&self, report: impl Fn(Report) + Send + 'static) {
        let Some((rx, kicked, done_tx, prefs)) = self.parts.lock().take() else { return };
        #[cfg(windows)]
        {
            let shared = self.shared.clone();
            let _ = std::thread::Builder::new()
                .name("hrtz-hud".into())
                .spawn(move || window::run(rx, shared, Box::new(report), prefs, done_tx));
            let (shared, tx) = (self.shared.clone(), self.tx.clone());
            let _ = std::thread::Builder::new().name("hrtz-hud-games".into()).spawn(move || list_installed(shared, kicked, tx));
        }
        #[cfg(not(windows))]
        let _ = (rx, kicked, done_tx, prefs, report);
    }

    /// The HUD's window is on screen.
    pub fn showing(&self) -> bool {
        self.shared.showing.load(Ordering::SeqCst)
    }

    fn send(&self, input: Input) {
        let _ = self.tx.send(input);
        wake(&self.shared);
    }

    pub fn device(&self, s: &DeviceState) {
        self.send(Input::Device(Box::new(s.clone())));
    }

    pub fn prefs(&self, p: &Prefs) {
        let on = p.hud.show != HudShow::Off;
        if on && !self.shared.enabled.swap(on, Ordering::SeqCst) {
            let _ = self.kick.send(());
        }
        self.shared.enabled.store(on, Ordering::SeqCst);
        self.send(Input::Prefs(Box::new(p.clone())));
    }

    pub fn foreground(&self, path: Option<String>) {
        self.send(Input::Foreground(path));
    }

    pub fn vendor_app(&self, running: bool) {
        self.send(Input::VendorApp(running));
    }

    /// The HUD page's preview: the real HUD with every chosen tab, over whatever is in front,
    /// with no game needed (and a sample mouse with no receiver). It gives way to a game in front.
    /// The page asks again every minute while it's open and up; the preview ends by itself 5
    /// minutes after the last ask, and does nothing while the HUD is off.
    pub fn test(&self, on: bool) {
        self.send(Input::Test(on));
    }

    /// While the HUD is on and a game is in front (or the preview is on), the old DPI and
    /// profile pop-up stays away so the two never show together.
    pub fn suppresses_osd(&self) -> bool {
        self.shared.in_game.load(Ordering::SeqCst)
    }

    /// Close the window, waiting a moment for that to happen. A preview ends with it.
    pub fn stop(&self) {
        if self.parts.lock().is_some() {
            return;
        }
        self.send(Input::Quit);
        let _ = self.done.recv_timeout(Duration::from_millis(300));
    }
}

#[cfg(windows)]
fn wake(shared: &Shared) {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, PostThreadMessageW, WM_APP};
    let hwnd = shared.hwnd.load(Ordering::SeqCst);
    unsafe {
        if hwnd != 0 {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_APP, WPARAM(0), LPARAM(0));
        } else if let t @ 1.. = shared.thread.load(Ordering::SeqCst) {
            // Before the window exists. If the thread has no queue yet either, it reads the
            // channel as soon as it does.
            let _ = PostThreadMessageW(t, WM_APP, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(not(windows))]
fn wake(_: &Shared) {}

/// List installed games now and every 30 minutes while the HUD is on, on this thread so the
/// HUD's never waits on the disk.
#[cfg(windows)]
fn list_installed(shared: Arc<Shared>, kicked: Receiver<()>, tx: Sender<Input>) {
    let program_data = std::path::PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()));
    loop {
        if shared.enabled.load(Ordering::SeqCst) {
            let games = crate::installed::installed_games(crate::win::steam_path(), &program_data);
            *shared.installed.write() = games.iter().map(|g| g.exe.to_lowercase()).collect();
            if tx.send(Input::Installed).is_err() {
                return;
            }
            wake(&shared);
        }
        match kicked.recv_timeout(INSTALLED_EVERY) {
            Ok(()) | Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::draw::{render, Look};
    use super::model::{tabs, Facts, Link};
    use super::text::Gdi;
    use crate::store::{HudAnchor, HudFields, HudSize, HudValueColor, Prefs};

    /// The sample mouse (what the page's preview shows with no receiver) at 100%, with this PC's
    /// fonts.
    #[test]
    fn draws_the_sample_mouse_at_its_real_sizes() {
        let prefs = Prefs::default();
        let sample = Facts::sample(&prefs);
        let draw = |hud: &crate::store::HudPrefs, facts: &Facts| render(&tabs(facts, &hud.fields), 1.0, Look::of(hud), Gdi::new().unwrap());
        let b = draw(&prefs.hud, &sample).unwrap();
        // The mark and three tabs in a row, 24 px tall plus the room for shadows.
        assert_eq!(b.height, 24 + 2 * 4);
        assert!(b.width > 330 && b.width < 530, "{}", b.width);
        let with_note = draw(&prefs.hud, &Facts { vendor_app: true, ..sample.clone() }).unwrap();
        assert!(with_note.width > b.width + 250, "the note about Zaopin's app");
        let stacked = crate::store::HudPrefs { anchor: HudAnchor::Right, ..prefs.hud.clone() };
        assert_eq!(draw(&stacked, &sample).unwrap().height, 4 * 24 + 3 * 4 + 2 * 4);
        // At the chosen size: 20 px tabs small, 29 px large, with the bleed rounded up.
        let sized = |size| crate::store::HudPrefs { size, ..prefs.hud.clone() };
        let small = draw(&sized(HudSize::Small), &sample).unwrap();
        let large = draw(&sized(HudSize::Large), &sample).unwrap();
        assert_eq!((small.height, large.height), (20 + 2 * 4, 29 + 2 * 5));
        assert!(small.width < b.width && b.width < large.width, "{} {} {}", small.width, b.width, large.width);
        // Without labels the row is the values alone, a good deal narrower.
        let bare = crate::store::HudPrefs { fields: HudFields { labels: false, ..HudFields::default() }, ..prefs.hud.clone() };
        let bare = draw(&bare, &sample).unwrap();
        assert!(bare.width + 80 < b.width, "{} {}", bare.width, b.width);
        // The mark alone leads nothing.
        let nothing = crate::store::HudPrefs {
            fields: HudFields { battery: false, time_left: false, polling: false, dpi: false, profile: false, ..HudFields::default() },
            ..prefs.hud.clone()
        };
        assert!(draw(&nothing, &sample).is_none(), "nothing to draw");
    }

    /// Writes the native HUD for several states as PNGs, to compare with the lab's
    /// `shots/lab/hud/edge*.png`. Run with `cargo test --lib dump_hud -- --ignored`; `HRTZ_OUT`
    /// names another folder.
    #[test]
    #[ignore = "writes PNGs into shots/lab/hud"]
    fn dump_hud() {
        let dir = std::env::var("HRTZ_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(r"..\..\shots\lab\hud")
        });
        std::fs::create_dir_all(&dir).unwrap();
        let prefs = Prefs::default();
        let yours = Facts::sample(&prefs);
        let stages = Facts { dpi: Some((800, 1, 4)), battery: Some((82, false)), time_left_h: Some(10.7), profile: Some("Valorant".into()), ..yours.clone() };
        let look = |anchor, plate| Look { anchor, size: HudSize::Medium, plate, value_color: HudValueColor::Teal };
        let row = look(HudAnchor::TopRight, 90);
        let fields = HudFields::default();
        let bare = HudFields { labels: false, ..fields };
        let away = Facts { link: Link::Away, ..yours.clone() };
        let cases: Vec<(&str, Facts, Look, f32, HudFields)> = vec![
            ("yours", yours.clone(), row, 1.0, fields),
            ("no-labels", yours.clone(), row, 1.0, bare),
            ("no-labels-stages", stages.clone(), row, 1.0, HudFields { profile: true, ..bare }),
            ("no-labels-saver", Facts { battery: Some((14, false)), polling_hz: Some(2000), saver: true, time_left_h: Some(1.8), ..yours.clone() }, row, 1.0, bare),
            ("no-labels-away-plate0", away.clone(), look(HudAnchor::TopRight, 0), 1.0, bare),
            ("no-labels-stacked-right", stages.clone(), look(HudAnchor::Right, 90), 1.0, HudFields { profile: true, ..bare }),
            ("stages", stages.clone(), row, 1.0, fields),
            ("low", Facts { battery: Some((14, false)), time_left_h: Some(1.8), ..yours.clone() }, row, 1.0, fields),
            ("charging", Facts { battery: Some((62, true)), time_left_h: None, ..yours.clone() }, row, 1.0, fields),
            ("away", away.clone(), row, 1.0, fields),
            ("away-plate0", away.clone(), look(HudAnchor::TopRight, 0), 1.0, fields),
            ("away-stacked-right", away.clone(), look(HudAnchor::Right, 90), 1.0, fields),
            ("away-large-150", away.clone(), Look { size: HudSize::Large, ..row }, 1.5, fields),
            ("saver", Facts { battery: Some((48, false)), polling_hz: Some(2000), saver: true, time_left_h: None, ..yours.clone() }, row, 1.0, fields),
            ("plate0", yours.clone(), look(HudAnchor::TopRight, 0), 1.0, fields),
            ("stacked-right", stages.clone(), look(HudAnchor::Right, 90), 1.0, HudFields { profile: true, ..fields }),
            ("vendor", Facts { vendor_app: true, ..yours.clone() }, row, 1.0, fields),
            ("white", stages.clone(), Look { value_color: HudValueColor::White, ..row }, 1.0, fields),
            ("cable-long-range", Facts { link: Link::Cable, long_range: true, battery: Some((71, true)), ..yours.clone() }, row, 1.0, fields),
            ("yours-150", yours.clone(), row, 1.5, fields),
            ("small", stages.clone(), Look { size: HudSize::Small, ..row }, 1.0, fields),
            ("large", stages.clone(), Look { size: HudSize::Large, ..row }, 1.0, fields),
        ];
        for (name, facts, look, scale, fields) in cases {
            let b = render(&tabs(&facts, &fields), scale, look, Gdi::new().unwrap()).unwrap();
            let png = crate::win::encode_png(b.width, b.height, &b.straight()).unwrap();
            std::fs::write(dir.join(format!("native-{name}.png")), png).unwrap();
        }
    }
}
