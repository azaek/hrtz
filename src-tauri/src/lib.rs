pub mod battery;
pub mod battery_log;
pub mod clicks;
pub mod device;
pub mod diag;
pub mod games;
pub mod hud;
pub mod installed;
pub mod pad;
pub mod protocol;
pub mod settings;
pub mod store;
pub mod tray;

#[cfg(windows)]
mod capture;
#[cfg(windows)]
mod meter;
#[cfg(windows)]
pub mod usb;
#[cfg(windows)]
mod win;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use device::{DeviceEvent, DeviceState, Engine};
use settings::Settings;
use store::{AutoReason, BackupDevice, BackupMeta, Backups, Prefs, ProfileSnapshot};
use tray::{TrayLevel, TrayStyle};

const OSD_W: f64 = 280.0;
const OSD_H: f64 = 76.0;

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    foreground_exe: Option<String>,
    vendor_app_running: bool,
    /// The HUD's window is on screen now.
    hud_showing: bool,
}

struct Ctx {
    engine: Engine,
    prefs: RwLock<Prefs>,
    prefs_path: PathBuf,
    backups: Backups,
    status: RwLock<AppStatus>,
    osd_gen: AtomicU64,
    low_alerted: AtomicBool,
    /// The battery saver is changing the polling rate.
    saver_busy: AtomicBool,
    tray: Mutex<Option<(TrayIcon, Vec<CheckMenuItem<tauri::Wry>>)>>,
    /// The tray's menu, for the "Restart to update" item a downloaded update adds.
    tray_menu: Mutex<Option<Menu<tauri::Wry>>>,
    /// The update already announced this run, so the notification comes once.
    update_ready: Mutex<Option<String>>,
    /// Last rendered (level, style, light taskbar), so the icon is only redrawn when it changes.
    tray_level: Mutex<Option<(TrayLevel, TrayStyle, bool)>>,
    hud: hud::Hud,
    /// Models (`mid`) checked this run for a first backup, so it's looked into once.
    met: Mutex<HashSet<u8>>,
}

/// The app's status with the HUD's part filled in.
fn status_of(ctx: &Ctx) -> AppStatus {
    let mut s = ctx.status.read().clone();
    s.hud_showing = ctx.hud.showing();
    s
}

/// Save prefs the app changed on its own, and tell the HUD and the window.
fn commit_prefs(app: &AppHandle, ctx: &Ctx, prefs: Prefs) {
    let _ = prefs.save(&ctx.prefs_path);
    *ctx.prefs.write() = prefs.clone();
    ctx.hud.prefs(&prefs);
    let _ = app.emit("prefs", prefs);
}

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

async fn on_device<T: Send + 'static>(
    ctx: &Ctx,
    f: impl FnOnce(&mut device::Worker) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let engine = ctx.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.call(f)).await.map_err(|e| e.to_string())?.map_err(err)
}

// ---- commands -------------------------------------------------------------------------------

#[tauri::command]
fn get_state(ctx: State<'_, Arc<Ctx>>) -> DeviceState {
    ctx.engine.snapshot()
}

/// Windows' name and version, for a problem report.
#[tauri::command]
fn windows_version() -> String {
    win::windows_version()
}

/// A downloaded update waits for a restart. Say so once, in a notification and at the top of the
/// tray menu, so it reaches someone who keeps hrtz in the tray and never opens the window.
#[tauri::command]
fn update_ready(app: AppHandle, ctx: State<'_, Arc<Ctx>>, version: String) {
    let first = {
        let mut ready = ctx.update_ready.lock();
        if ready.as_deref() == Some(version.as_str()) {
            return;
        }
        ready.replace(version.clone()).is_none()
    };
    let _ = app
        .notification()
        .builder()
        .title(format!("hrtz {version} is ready"))
        .body("Choose Restart to update in the tray menu or the sidebar.")
        .show();
    if first
        && let Some(menu) = ctx.tray_menu.lock().as_ref()
        && let (Ok(item), Ok(sep)) = (
            MenuItem::with_id(&app, "update", "Restart to update", true, None::<&str>),
            PredefinedMenuItem::separator(&app),
        )
    {
        let _ = menu.insert(&item, 0);
        let _ = menu.insert(&sep, 1);
    }
}

/// Before the updater exits hrtz to install a new version: save what Quit would save. The rest of
/// Quit (the HUD, the goodbye to the mouse) ends with the process.
#[tauri::command]
fn prepare_update() {
    #[cfg(windows)]
    {
        meter::save_pad();
        meter::save_clicks();
    }
}

#[tauri::command]
async fn apply_settings(ctx: State<'_, Arc<Ctx>>, settings: Settings) -> CmdResult<Settings> {
    on_device(&ctx, move |w| w.apply_settings(&settings)).await
}

#[tauri::command]
async fn set_profile(ctx: State<'_, Arc<Ctx>>, index: u8) -> CmdResult<()> {
    on_device(&ctx, move |w| w.set_profile(index)).await
}

#[tauri::command]
async fn set_long_range(ctx: State<'_, Arc<Ctx>>, on: bool) -> CmdResult<()> {
    on_device(&ctx, move |w| w.set_long_range(on)).await
}

/// Pair a mouse with the receiver: up to 30 s, with `pairing` events counting down. True when one
/// paired.
#[tauri::command]
async fn pair_receiver(ctx: State<'_, Arc<Ctx>>) -> CmdResult<bool> {
    on_device(&ctx, |w| w.pair()).await
}

/// Factory settings, after saving the mouse as it is, so the setup can be got back from Backups.
#[tauri::command]
async fn factory_reset(app: AppHandle, ctx: State<'_, Arc<Ctx>>) -> CmdResult<()> {
    let backups = ctx.backups.clone();
    on_device(&ctx, move |w| {
        let current = w.read_full_image()?;
        backups.create_auto(&current, backup_device(&w.snapshot()), AutoReason::Factory)?;
        w.factory_reset()
    })
    .await?;
    let _ = app.emit("backups", ());
    Ok(())
}

#[tauri::command]
fn get_prefs(ctx: State<'_, Arc<Ctx>>) -> Prefs {
    ctx.prefs.read().clone()
}

#[tauri::command]
fn set_prefs(app: AppHandle, ctx: State<'_, Arc<Ctx>>, prefs: Prefs) -> CmdResult<()> {
    prefs.save(&ctx.prefs_path).map_err(err)?;
    #[cfg(windows)]
    if prefs.pad_tracking != ctx.prefs.read().pad_tracking {
        let on = prefs.pad_tracking;
        let _ = app.run_on_main_thread(move || meter::set_tracking(on));
    }
    #[cfg(windows)]
    if prefs.click_history != ctx.prefs.read().click_history {
        let on = prefs.click_history;
        let _ = app.run_on_main_thread(move || meter::set_clicks(on));
    }
    *ctx.prefs.write() = prefs.clone();
    if let Some((_, items)) = ctx.tray.lock().as_ref() {
        for (item, name) in items.iter().zip(&prefs.profile_names) {
            let _ = item.set_text(name);
        }
    }
    refresh_tray(&ctx, &ctx.engine.snapshot());
    ctx.hud.prefs(&prefs);
    let _ = app.emit("prefs", prefs);
    Ok(())
}

#[tauri::command]
fn list_backups(ctx: State<'_, Arc<Ctx>>) -> Vec<BackupMeta> {
    ctx.backups.list()
}

/// Who the mouse is, as a backup records it.
fn backup_device(s: &DeviceState) -> BackupDevice {
    BackupDevice {
        cid: s.cid,
        mid: s.mid,
        device_type: s.device_type,
        firmware: s.firmware.clone().unwrap_or_default(),
        profile_index: s.profile,
    }
}

#[tauri::command]
async fn create_backup(ctx: State<'_, Arc<Ctx>>, note: String) -> CmdResult<BackupMeta> {
    let (image, s) = on_device(&ctx, |w| {
        let image = w.read_full_image()?;
        Ok((image, w.snapshot()))
    })
    .await?;
    ctx.backups.create(&image, backup_device(&s), &note).map_err(err)
}

/// What a restore did: how many chunks it wrote (0 when the mouse already matched), and the
/// backup of the mouse as it was just before, which puts it back.
#[derive(Serialize)]
struct Restored {
    written: usize,
    saved: Option<String>,
}

#[tauri::command]
async fn restore_backup(ctx: State<'_, Arc<Ctx>>, id: String) -> CmdResult<Restored> {
    let (file, image) = ctx.backups.load(&id).map_err(err)?;
    let current = ctx.engine.snapshot();
    if current.connected && current.mid != file.device.mid {
        return Err(format!(
            "This backup is for a {}, but a {} is connected.",
            settings::model_name(file.device.mid),
            current.model.unwrap_or_default()
        ));
    }
    // One read serves both: the backup of the mouse as it is, and what to compare against, so
    // only the chunks that differ are written.
    let backups = ctx.backups.clone();
    let reason = AutoReason::Restore { note: file.note.clone(), created: file.created.clone() };
    on_device(&ctx, move |w| {
        let current = w.read_full_image()?;
        if current == image {
            return Ok(Restored { written: 0, saved: None });
        }
        let saved = backups.create_auto(&current, backup_device(&w.snapshot()), reason)?;
        let written = w.write_image(&current, &image)?;
        Ok(Restored { written, saved: Some(saved.id) })
    })
    .await
}

/// Puts back the mouse as it was before a restore, from the backup the restore saved, and drops
/// that backup: it has done its job.
#[tauri::command]
async fn undo_restore(ctx: State<'_, Arc<Ctx>>, id: String) -> CmdResult<usize> {
    let (file, image) = ctx.backups.load(&id).map_err(err)?;
    let written = on_device(&ctx, move |w| w.restore_image(&image)).await?;
    // Only while it's still hrtz's own: one given a name since is yours, and stays.
    if file.auto.is_some() {
        ctx.backups.delete(&id).map_err(err)?;
    }
    Ok(written)
}

#[tauri::command]
fn rename_backup(ctx: State<'_, Arc<Ctx>>, id: String, note: String) -> CmdResult<BackupMeta> {
    ctx.backups.rename(&id, &note).map_err(err)
}

#[tauri::command]
fn delete_backup(ctx: State<'_, Arc<Ctx>>, id: String) -> CmdResult<()> {
    ctx.backups.delete(&id).map_err(err)
}

#[tauri::command]
fn export_backup(ctx: State<'_, Arc<Ctx>>, id: String, path: String) -> CmdResult<()> {
    ctx.backups.export(&id, &PathBuf::from(path)).map_err(err)
}

#[tauri::command]
fn import_backup(ctx: State<'_, Arc<Ctx>>, path: String) -> CmdResult<BackupMeta> {
    ctx.backups.import(&PathBuf::from(path)).map_err(err)
}

#[tauri::command]
fn backup_settings(ctx: State<'_, Arc<Ctx>>, id: String) -> CmdResult<Settings> {
    ctx.backups.settings(&id).map_err(err)
}

#[tauri::command]
fn open_backups_folder(app: AppHandle, ctx: State<'_, Arc<Ctx>>) -> CmdResult<()> {
    std::fs::create_dir_all(ctx.backups.dir()).map_err(|e| e.to_string())?;
    app.opener().open_path(ctx.backups.dir().to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn running_apps() -> Vec<win::RunningApp> {
    #[cfg(windows)]
    {
        win::running_apps()
    }
}

/// Games installed through Steam, Epic and Riot, with the exe that runs each.
#[tauri::command]
async fn installed_games() -> Vec<installed::InstalledGame> {
    tauri::async_runtime::spawn_blocking(|| {
        #[cfg(windows)]
        let steam = win::steam_path();
        #[cfg(not(windows))]
        let steam = None;
        let program_data = PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()));
        #[allow(unused_mut)]
        let mut games = installed::installed_games(steam, &program_data);
        #[cfg(windows)]
        for g in games.iter_mut().filter(|g| g.name.is_empty()) {
            g.name = win::app_name(&g.path);
        }
        games
    })
    .await
    .unwrap_or_default()
}

/// Name and icon of an exe picked from disk.
#[tauri::command]
fn app_info(path: String) -> Option<win::RunningApp> {
    #[cfg(windows)]
    {
        win::app_info(&path)
    }
}

#[tauri::command]
fn app_icon(path: String) -> Option<String> {
    #[cfg(windows)]
    {
        win::app_icon(&path)
    }
}

/// Both tray styles rendered for the current battery state, as PNG data URLs.
#[tauri::command]
fn tray_previews(ctx: State<'_, Arc<Ctx>>) -> std::collections::HashMap<&'static str, String> {
    use base64::Engine as _;
    let prefs = ctx.prefs.read().clone();
    let mut level = tray_level(&ctx.engine.snapshot(), &prefs);
    if level == TrayLevel::Disconnected {
        level = TrayLevel::Battery { level: 70, charging: false, low_at: prefs.low_battery_percent };
    }
    let mut out = std::collections::HashMap::new();
    out.insert("taskbar", if taskbar_light() { "light" } else { "dark" }.to_string());
    for (name, style) in [("percent", TrayStyle::Percent), ("battery", TrayStyle::Battery)] {
        let r = tray::render_at(level, style, Some(32));
        #[cfg(windows)]
        if let Some(png) = win::encode_png(r.width, r.height, &r.rgba) {
            out.insert(
                name,
                format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)),
            );
        }
    }
    out
}

#[tauri::command]
fn app_status(ctx: State<'_, Arc<Ctx>>) -> AppStatus {
    status_of(&ctx)
}

/// The HUD page's preview: the real HUD on screen while the page is open, with no game in front.
/// The page sends `true` again every minute while it's open and up, and the HUD ends the preview
/// by itself 5 minutes after the last one. A window that's hidden or minimised can't be looking
/// at it, so an ask from one turns the preview off instead: its page may not have heard that it
/// went.
#[tauri::command]
fn hud_test(app: AppHandle, ctx: State<'_, Arc<Ctx>>, on: bool) {
    ctx.hud.test(on && main_window_up(&app));
}

/// The main window is on screen: shown, and not minimised.
fn main_window_up(app: &AppHandle) -> bool {
    let Some(main) = app.get_webview_window("main") else { return false };
    #[cfg(windows)]
    let up = main.hwnd().is_ok_and(|h| win::window_up(h.0 as isize));
    #[cfg(not(windows))]
    let up = main.is_visible().unwrap_or(false) && !main.is_minimized().unwrap_or(false);
    up
}

/// The HUD shows this change itself: it's on (every mode but off shows changes), with that tab on.
fn hud_shows(h: &store::HudPrefs, e: &DeviceEvent) -> bool {
    h.show != store::HudShow::Off
        && match e {
            DeviceEvent::Dpi { .. } => h.fields.dpi,
            DeviceEvent::Polling { .. } => h.fields.polling,
            DeviceEvent::Profile { .. } => h.fields.profile,
            DeviceEvent::Progress { .. } | DeviceEvent::Pairing { .. } => false,
        }
}

/// What the HUD reports: a game it can't draw over, and whether it's on screen. This runs on the
/// HUD's thread, so anything that reads or writes files goes to a thread of its own.
fn on_hud_report(app: &AppHandle, r: hud::Report) {
    let Some(ctx) = app.try_state::<Arc<Ctx>>() else { return };
    match r {
        hud::Report::Exclusive { exe, path } => {
            let (app, ctx) = (app.clone(), ctx.inner().clone());
            thread::spawn(move || {
                #[cfg(windows)]
                let name = win::app_name(&path);
                #[cfg(not(windows))]
                let name = path;
                let mut prefs = ctx.prefs.read().clone();
                if prefs.hud.block(&exe, name, "exclusive") {
                    commit_prefs(&app, &ctx, prefs);
                }
            });
        }
        hud::Report::Showing(_) => {
            let _ = app.emit("app-status", status_of(&ctx));
        }
    }
}

/// `seq` only grows: a request older than one already applied is dropped (`meter::newest` says
/// why).
#[tauri::command]
fn meter(app: AppHandle, on: bool, rate: Option<u32>, seq: Option<u64>) {
    #[cfg(windows)]
    {
        if let Some(hz) = rate {
            meter::set_rate(hz);
        }
        // Raw input registration and the window subclass live on the main thread.
        let _ = app.run_on_main_thread(move || meter::set_running(on, seq));
    }
    #[cfg(not(windows))]
    let _ = (app, on, rate, seq);
}

/// A new connection test run: the Lab page's runs start from nothing.
#[tauri::command]
fn meter_reset() {
    #[cfg(windows)]
    meter::reset_run();
}

/// Game mode: a screen of hrtz's own that takes the pointer as a game does, showing `screen`
/// (or new text on one already up). The page hears `capture-closed` with why it closed.
#[tauri::command]
fn capture_open(app: AppHandle, screen: serde_json::Value) {
    #[cfg(windows)]
    if let Ok(screen) = serde_json::from_value::<capture::Screen>(screen) {
        let main = app.get_webview_window("main").and_then(|w| w.hwnd().ok()).map_or(0, |h| h.0 as isize);
        let handle = app.clone();
        capture::open(screen, main, move |why| {
            let _ = handle.emit_to("main", "capture-closed", why);
        });
    }
    #[cfg(not(windows))]
    let _ = (app, screen);
}

#[tauri::command]
fn capture_update(screen: serde_json::Value) {
    #[cfg(windows)]
    if let Ok(screen) = serde_json::from_value::<capture::Screen>(screen) {
        capture::update(screen);
    }
    #[cfg(not(windows))]
    let _ = screen;
}

#[tauri::command]
fn capture_close() {
    #[cfg(windows)]
    capture::close();
}

/// A timing test is open (`true`) or closed: while it's open, its timing counts only in game mode.
/// `seq` as for `meter`.
#[tauri::command]
fn meter_hold(on: bool, seq: Option<u64>) {
    #[cfg(windows)]
    meter::set_hold(on, seq);
    #[cfg(not(windows))]
    let _ = (on, seq);
}

/// Double clicks counted over the last 30 days, oldest first.
#[tauri::command]
fn click_history() -> Vec<clicks::Day> {
    #[cfg(windows)]
    return meter::click_days();
    #[cfg(not(windows))]
    Vec::new()
}

#[tauri::command]
fn pad_state(app: Option<String>) -> Option<pad::PadView> {
    #[cfg(windows)]
    return meter::pad_view(app.as_deref());
    #[cfg(not(windows))]
    {
        let _ = app;
        None
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// What the battery log says about battery life and health.
#[tauri::command]
fn battery_summary(ctx: State<'_, Arc<Ctx>>) -> battery_log::Summary {
    battery_log::summary(ctx.engine.log.lock().entries(), unix_now())
}

/// Battery log rows from the last `days`, thinned for a chart.
#[tauri::command]
fn battery_history(ctx: State<'_, Arc<Ctx>>, days: u32) -> Vec<battery_log::Entry> {
    ctx.engine.log.lock().history(unix_now().saturating_sub(days as u64 * 86_400), 720)
}

/// Lower the polling rate while the battery is low, and put back the rate you chose once it's
/// charging (or the saver is turned off). Each change is written to the mouse's memory, so this
/// switches about twice a charge, never back and forth.
fn battery_saver(app: &AppHandle, ctx: &Arc<Ctx>, s: &DeviceState, prefs: &Prefs) {
    let (Some(b), Some(st), Some(caps)) = (&s.battery, &s.settings, &s.capabilities) else { return };
    if !s.online || caps.wired || ctx.saver_busy.load(Ordering::SeqCst) {
        return;
    }
    let Some((target, restore)) = prefs.saver_step(b.level, b.charging, st.polling_hz) else { return };
    let lowering = restore.is_some() && target.is_some();
    let (from, level, model) = (st.polling_hz, b.level, s.model.clone().unwrap_or_default());
    let mut next = st.clone();
    let (app, ctx) = (app.clone(), ctx.clone());
    ctx.saver_busy.store(true, Ordering::SeqCst);
    thread::spawn(move || {
        let applied = match target {
            Some(hz) => {
                next.polling_hz = hz;
                ctx.engine.call(move |w| w.apply_settings(&next)).map(|_| ())
            }
            None => Ok(()),
        };
        match applied {
            Ok(()) => {
                let mut p = ctx.prefs.read().clone();
                p.battery_saver_restore = restore;
                commit_prefs(&app, &ctx, p);
                if lowering {
                    notify_saver(&app, level, target.unwrap_or(0), from, &model);
                }
            }
            Err(e) => eprintln!("battery saver: {e:#}"),
        }
        ctx.saver_busy.store(false, Ordering::SeqCst);
    });
}

fn notify_low_battery(app: &AppHandle, level: u8, model: &str) {
    let _ = app
        .notification()
        .builder()
        .title(format!("Mouse battery at {level}%"))
        .body(format!("Charge your {model} soon."))
        .show();
}

fn notify_saver(app: &AppHandle, level: u8, to: u32, from: u32, model: &str) {
    let _ = app
        .notification()
        .builder()
        .title(format!("Battery saver lowered polling to {} Hz", fmt_hz(to)))
        .body(format!("Battery at {level}%. Your {model} returns to {} Hz when charged.", fmt_hz(from)))
        .show();
}

/// Show one of the app's pop-ups as it really appears, with the mouse's current values where it
/// has them. For checking how each looks.
#[tauri::command]
fn test_popup(app: AppHandle, ctx: State<'_, Arc<Ctx>>, kind: String) -> CmdResult<()> {
    let prefs = ctx.prefs.read().clone();
    let s = ctx.engine.snapshot();
    let st = s.settings.as_ref();
    let model = s.model.clone().unwrap_or_else(|| "mouse".into());
    let level = s.battery.as_ref().map_or(15, |b| b.level);
    let hz = st.map_or(8000, |x| x.polling_hz);
    let event = match kind.as_str() {
        "dpi" => Some(DeviceEvent::Dpi {
            stage: st.map_or(0, |x| x.dpi_stage),
            dpi: st.and_then(|x| x.stages.get(x.dpi_stage as usize)).map_or(1600, |x| x.dpi),
        }),
        "polling" => Some(DeviceEvent::Polling { hz }),
        "profile" => Some(DeviceEvent::Profile { index: s.profile.unwrap_or(0) }),
        "low-battery" => {
            notify_low_battery(&app, level.min(prefs.low_battery_percent), &model);
            None
        }
        "battery-saver" => {
            notify_saver(
                &app,
                level.min(prefs.battery_saver_below),
                prefs.battery_saver_hz,
                hz.max(prefs.battery_saver_hz * 2),
                &model,
            );
            None
        }
        other => return Err(format!("No pop-up called {other}")),
    };
    if let Some(e) = event {
        show_osd(&app, &ctx, &e, &prefs);
    }
    Ok(())
}

/// 8000 → "8,000".
fn fmt_hz(hz: u32) -> String {
    let s = hz.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A game's mouse sensitivity, read from its own settings file.
#[tauri::command]
fn game_sensitivity(exe: String) -> Option<games::Sensitivity> {
    games::sensitivity(&exe)
}

#[tauri::command]
fn pad_calibrate(calibration: pad::Calibration) {
    #[cfg(windows)]
    meter::pad_calibrate(calibration);
}

/// Windows' pointer acceleration and speed, and whether Raw Accel is installed.
#[tauri::command]
fn pointer_settings() -> Option<serde_json::Value> {
    #[cfg(windows)]
    return serde_json::to_value(win::pointer_settings()).ok();
    #[cfg(not(windows))]
    None
}

#[tauri::command]
fn fix_pointer(acceleration: bool, speed: bool) -> CmdResult<Option<serde_json::Value>> {
    #[cfg(windows)]
    {
        win::set_pointer(acceleration, speed).map_err(|e| e.to_string())?;
        Ok(serde_json::to_value(win::pointer_settings()).ok())
    }
    #[cfg(not(windows))]
    {
        let _ = (acceleration, speed);
        Ok(None)
    }
}

#[tauri::command]
fn pad_clear() {
    #[cfg(windows)]
    meter::pad_clear();
}

// ---- device notifications ------------------------------------------------------------------

fn tray_level(s: &DeviceState, prefs: &Prefs) -> TrayLevel {
    match (&s.battery, s.connected, s.online) {
        (_, false, _) => TrayLevel::Disconnected,
        (Some(b), true, true) => {
            TrayLevel::Battery { level: b.level, charging: b.charging, low_at: prefs.low_battery_percent }
        }
        (b, _, _) => TrayLevel::Asleep { level: b.as_ref().map(|b| b.level) },
    }
}

fn tray_image(level: TrayLevel, style: TrayStyle) -> tauri::image::Image<'static> {
    let r = tray::render(level, style);
    tauri::image::Image::new_owned(r.rgba, r.width, r.height)
}

fn taskbar_light() -> bool {
    #[cfg(windows)]
    {
        win::taskbar_is_light()
    }
    #[cfg(not(windows))]
    false
}

/// Redraw the tray icon, tooltip and profile checks for `s`; the icon only when something visible changed.
fn refresh_tray(ctx: &Ctx, s: &DeviceState) {
    let prefs = ctx.prefs.read().clone();
    let key = (tray_level(s, &prefs), prefs.tray_style, taskbar_light());
    if let Some((tray, items)) = ctx.tray.lock().as_ref() {
        let mut last = ctx.tray_level.lock();
        if *last != Some(key) {
            let _ = tray.set_icon(Some(tray_image(key.0, key.1)));
            *last = Some(key);
        }
        let _ = tray.set_tooltip(Some(tooltip(s)));
        for (i, item) in items.iter().enumerate() {
            let _ = item.set_checked(s.profile == Some(i as u8));
            let _ = item.set_enabled(s.online);
        }
    }
}

fn tooltip(s: &DeviceState) -> String {
    let model = s.model.clone().unwrap_or_else(|| "Zaopin mouse".into());
    match (&s.battery, s.connected, s.online) {
        (_, false, _) => "hrtz: no mouse connected".into(),
        (Some(b), true, true) => {
            let hz = s.settings.as_ref().map(|x| format!(" · {} Hz", x.polling_hz)).unwrap_or_default();
            let charging = if b.charging { " charging" } else { "" };
            format!("{model}: {}%{charging}{hz}", b.level)
        }
        _ => format!("{model}: asleep, move it to wake"),
    }
}

fn on_device_change(app: &AppHandle, s: &DeviceState, event: Option<DeviceEvent>) {
    let _ = app.emit("device-state", s);
    let Some(ctx) = app.try_state::<Arc<Ctx>>() else { return };
    let prefs = ctx.prefs.read().clone();

    // Pad tracking turns sensor counts into centimetres at the DPI in use, and reads pauses
    // differently when the sensor keeps tracking as the mouse rises.
    #[cfg(windows)]
    if let Some(st) = &s.settings {
        meter::set_dpi(st.stages.get(st.dpi_stage as usize).map_or(0, |x| x.dpi));
        meter::set_lift_off(st.lod != settings::Lod::Low);
    }

    refresh_tray(&ctx, s);
    ctx.hud.device(s);
    first_meeting(app, &ctx, s);

    // Remember what the profile in use holds: the mouse reports only that one, and the Profiles
    // page shows the rest from these.
    if let (true, Some(i), Some(st), Some(caps)) = (s.online, s.profile, &s.settings, &s.capabilities) {
        let snap = ProfileSnapshot::of(s.mid, st, &caps.buttons);
        if prefs.profile_snapshots.get(i as usize).is_some_and(|x| x.as_ref() != Some(&snap)) {
            let mut next = ctx.prefs.read().clone();
            if next.note_profile(i, snap) {
                commit_prefs(app, &ctx, next);
            }
        }
    }

    if let Some(b) = &s.battery {
        let low = b.level <= prefs.low_battery_percent && !b.charging;
        if low && prefs.battery_alerts && !ctx.low_alerted.swap(true, Ordering::SeqCst) {
            notify_low_battery(app, b.level, &s.model.clone().unwrap_or_default());
        }
        if b.charging || b.level > prefs.low_battery_percent + 5 {
            ctx.low_alerted.store(false, Ordering::SeqCst);
        }
    }
    battery_saver(app, &ctx, s, &prefs);

    match event {
        Some(DeviceEvent::Progress { done, total }) => {
            let _ = app.emit("device-progress", (done, total));
        }
        Some(DeviceEvent::Pairing { seconds_left }) => {
            let _ = app.emit("pairing", seconds_left);
        }
        Some(e) if prefs.osd => show_osd(app, &ctx, &e, &prefs),
        _ => {}
    }
}

/// The first time hrtz meets a mouse (no backup of its model yet), it saves one as it found it,
/// before anything here changes it. Checked once per model per run, while it's awake and the
/// vendor app isn't talking to it too. The read is quiet: nothing the user started.
fn first_meeting(app: &AppHandle, ctx: &Arc<Ctx>, s: &DeviceState) {
    if !s.online || s.settings.is_none() || ctx.status.read().vendor_app_running || !ctx.met.lock().insert(s.mid) {
        return;
    }
    let model = settings::model_name(s.mid);
    if ctx.backups.list().iter().any(|b| b.model == model) {
        return;
    }
    let (app, ctx, mid) = (app.clone(), ctx.clone(), s.mid);
    tauri::async_runtime::spawn(async move {
        let backups = ctx.backups.clone();
        let saved = on_device(&ctx, move |w| {
            let image = w.read_full_image_quietly()?;
            backups.create_auto(&image, backup_device(&w.snapshot()), AutoReason::First)
        })
        .await;
        match saved {
            Ok(_) => {
                let _ = app.emit("backups", ());
            }
            // Try again on the next connection.
            Err(_) => {
                ctx.met.lock().remove(&mid);
            }
        }
    });
}

fn show_osd(app: &AppHandle, ctx: &Ctx, e: &DeviceEvent, prefs: &Prefs) {
    // In a game, the HUD says it instead when it will (its mode shows changes and that tab is
    // on), so the two never show together and a change is never left unshown.
    if ctx.hud.suppresses_osd() && hud_shows(&prefs.hud, e) {
        return;
    }
    let Some(osd) = app.get_webview_window("osd") else { return };
    #[derive(Serialize, Clone)]
    struct Osd {
        kind: &'static str,
        title: String,
        detail: String,
    }
    let payload = match e {
        DeviceEvent::Dpi { stage, dpi } => {
            Osd { kind: "dpi", title: format!("{} DPI", fmt_hz(*dpi)), detail: format!("Stage {}", stage + 1) }
        }
        DeviceEvent::Polling { hz } => {
            Osd { kind: "polling", title: format!("{} Hz", fmt_hz(*hz)), detail: "Polling rate".into() }
        }
        DeviceEvent::Profile { index } => Osd {
            kind: "profile",
            title: prefs
                .profile_names
                .get(*index as usize)
                .cloned()
                .unwrap_or_else(|| format!("Profile {}", index + 1)),
            detail: format!("Profile {}", index + 1),
        },
        DeviceEvent::Progress { .. } | DeviceEvent::Pairing { .. } => return,
    };
    if let Ok(Some(m)) = osd.primary_monitor() {
        let scale = m.scale_factor();
        let (mw, mh) = (m.size().width as f64, m.size().height as f64);
        let x = m.position().x as f64 + (mw - OSD_W * scale) / 2.0;
        let y = m.position().y as f64 + mh - (OSD_H + 120.0) * scale;
        let _ = osd.set_position(PhysicalPosition::new(x as i32, y as i32));
    }
    let _ = osd.emit_to("osd", "osd", payload);
    #[cfg(windows)]
    if let Ok(h) = osd.hwnd() {
        win::show_no_activate(h.0 as isize);
    }
    let _ = osd.set_always_on_top(true);
    let generation = ctx.osd_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    // On screen 1.2 s, then a 0.2 s fade in the page; hidden once that's done. A newer value
    // restarts it.
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1450));
        if let Some(ctx) = app.try_state::<Arc<Ctx>>()
            && ctx.osd_gen.load(Ordering::SeqCst) == generation
            && let Some(w) = app.get_webview_window("osd")
        {
            #[cfg(windows)]
            if let Ok(h) = w.hwnd() {
                win::hide(h.0 as isize);
            }
            #[cfg(not(windows))]
            let _ = w.hide();
        }
    });
}

// ---- background watchers -------------------------------------------------------------------

#[cfg(windows)]
fn watch_foreground(app: AppHandle) {
    thread::Builder::new()
        .name("hrtz-foreground".into())
        .spawn(move || {
            let own = std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default();
            let mut last_exe: Option<String> = None;
            let mut last_path: Option<String> = None;
            let mut last_recording: Option<String> = None;
            let mut last_hud: Option<(Option<String>, isize, isize)> = None;
            let mut ticks = 0u32;
            loop {
                thread::sleep(Duration::from_millis(500));
                let Some(ctx) = app.try_state::<Arc<Ctx>>() else { continue };
                ticks += 1;
                if ticks.is_multiple_of(6) {
                    refresh_tray(&ctx, &ctx.engine.snapshot());
                }
                if ticks.is_multiple_of(4)
                    && let Some(h) = app.get_webview_window("main").and_then(|w| w.hwnd().ok())
                {
                    win::strip_webview_scroll_styles(h.0 as isize);
                }
                if ticks % 20 == 1 {
                    let running = win::vendor_app_running();
                    let changed = ctx.status.read().vendor_app_running != running;
                    if changed {
                        ctx.status.write().vendor_app_running = running;
                        ctx.hud.vendor_app(running);
                        let _ = app.emit("app-status", status_of(&ctx));
                    }
                }
                let path = win::foreground_path();
                let exe = path.as_deref().map(win::file_name);
                let ours = exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&own));
                // The HUD shows only over games, and goes back on top whenever another window
                // comes to the front. It sits on the front window's monitor, so it follows that
                // window to another one: hrtz's own, dragged across during the HUD page's test.
                let fg = win::foreground_window();
                let front = (path.clone(), fg, win::monitor_of(fg));
                if last_hud.as_ref() != Some(&front) {
                    ctx.hud.foreground(front.0.clone());
                    last_hud = Some(front);
                }
                // The pad map only records in the games on the list. A known game joins it the first
                // time it comes into focus.
                if path != last_path {
                    last_path = path.clone();
                    // A rule typed in by hand gets its app's name and icon once the app shows up.
                    if let (Some(p), Some(e), false) = (&path, &exe, ours) {
                        let mut prefs = ctx.prefs.read().clone();
                        if prefs.fill_rule(e, win::app_name(p), p) {
                            commit_prefs(&app, &ctx, prefs);
                        }
                    }
                    if let (Some(p), Some(e), false) = (&path, &exe, ours)
                        && games::find(e).is_some()
                    {
                        let mut prefs = ctx.prefs.read().clone();
                        if prefs.join_pad_game(e, win::app_name(p), p.clone()) {
                            commit_prefs(&app, &ctx, prefs);
                        }
                    }
                }
                // Checked every tick, so adding or removing a game takes effect while it's in focus.
                let game = exe.as_deref().filter(|_| !ours).and_then(|e| ctx.prefs.read().pad_game(e).cloned());
                let recording = game.as_ref().map(|g| g.exe.to_lowercase());
                if recording != last_recording {
                    last_recording = recording;
                    meter::set_app(game.map(|g| pad::AppTag {
                        exe: g.exe.to_lowercase().into(),
                        name: g.name,
                        path: g.path,
                    }));
                }
                if exe.is_none() || exe == last_exe || ours {
                    continue;
                }
                last_exe = exe.clone();
                ctx.status.write().foreground_exe = exe.clone();
                let _ = app.emit("app-status", status_of(&ctx));
                let prefs = ctx.prefs.read().clone();
                if !prefs.rules_enabled {
                    continue;
                }
                let Some(want) = exe.as_deref().and_then(|e| prefs.profile_for(e)) else { continue };
                let s = ctx.engine.snapshot();
                if s.online
                    && s.profile.is_some()
                    && s.profile != Some(want)
                    && let Err(e) = ctx.engine.call(move |w| w.set_profile(want))
                {
                    eprintln!("rule switch failed: {e:#}");
                }
            }
        })
        .expect("spawn foreground watcher");
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle, ctx: &Ctx) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open hrtz", true, None::<&str>)?;
    let names = ctx.prefs.read().profile_names.clone();
    let profiles: Vec<CheckMenuItem<tauri::Wry>> = (0..4)
        .map(|i| CheckMenuItem::with_id(app, format!("profile-{i}"), &names[i], false, false, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let mut items: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = vec![&open, &sep1];
    for p in &profiles {
        items.push(p);
    }
    items.push(&sep2);
    items.push(&quit_item);
    let menu = Menu::with_items(app, &items)?;
    let tray = TrayIconBuilder::with_id("main")
        .icon(tray_image(TrayLevel::Disconnected, ctx.prefs.read().tray_style))
        .tooltip("hrtz")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => quit(app),
            // The window holds the downloaded update (src/update.ts), and installs it.
            "update" => {
                let _ = app.emit_to("main", "update-install", ());
            }
            id => {
                if let Some(i) = id.strip_prefix("profile-").and_then(|n| n.parse::<u8>().ok()) {
                    let app = app.clone();
                    thread::spawn(move || {
                        if let Some(ctx) = app.try_state::<Arc<Ctx>>()
                            && let Err(e) = ctx.engine.call(move |w| w.set_profile(i))
                        {
                            eprintln!("profile switch failed: {e:#}");
                        }
                    });
                }
            }
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    *ctx.tray.lock() = Some((tray, profiles));
    *ctx.tray_menu.lock() = Some(menu);
    Ok(())
}

fn quit(app: &AppHandle) {
    if let Some(ctx) = app.try_state::<Arc<Ctx>>() {
        ctx.hud.stop();
        #[cfg(windows)]
        {
            meter::set_running(false, None);
            meter::save_pad();
            meter::save_clicks();
        }
        let _ = ctx.engine.call(|w| {
            w.goodbye();
            Ok(())
        });
    }
    app.exit(0);
}

pub fn run() {
    let context = tauri::generate_context!();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        // Updates: a signed manifest on the GitHub release, then a restart into the new version.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(move |app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let prefs_path = config_dir.join("prefs.json");
            let notify_handle = handle.clone();
            std::fs::create_dir_all(&data_dir)?;
            let engine = Engine::start(
                Arc::new(move |s, e| on_device_change(&notify_handle, s, e)),
                Some(data_dir.join("battery.json")),
                Some(data_dir.join("battery_log.jsonl")),
            );
            let prefs = Prefs::load(&prefs_path);
            let hud = hud::Hud::new(&prefs, engine.log.clone());
            let ctx = Arc::new(Ctx {
                engine,
                prefs: RwLock::new(prefs),
                prefs_path,
                backups: Backups::new(data_dir.join("backups")),
                status: RwLock::new(AppStatus::default()),
                osd_gen: AtomicU64::new(0),
                low_alerted: AtomicBool::new(false),
                saver_busy: AtomicBool::new(false),
                tray: Mutex::new(None),
                tray_menu: Mutex::new(None),
                update_ready: Mutex::new(None),
                tray_level: Mutex::new(None),
                hud,
                met: Mutex::new(HashSet::new()),
            });
            build_tray(&handle, &ctx)?;
            app.manage(ctx.clone());
            // Its reports need the state managed above.
            let hud_handle = handle.clone();
            ctx.hud.start(move |r| on_hud_report(&hud_handle, r));
            ctx.hud.device(&ctx.engine.snapshot());

            WebviewWindowBuilder::new(app, "osd", WebviewUrl::App("index.html".into()))
                .title("hrtz OSD")
                .inner_size(OSD_W, OSD_H)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .build()?
                .set_ignore_cursor_events(true)?;

            #[cfg(windows)]
            {
                if let Some(main) = app.get_webview_window("main") {
                    let meter_handle = handle.clone();
                    meter::attach(main.hwnd()?.0 as isize, data_dir.join("pad.json"), move |sample| {
                        let _ = meter_handle.emit_to("main", "meter", sample);
                    });
                    if let Some(ctx) = app.try_state::<Arc<Ctx>>() {
                        meter::set_tracking(ctx.prefs.read().pad_tracking);
                        meter::set_clicks(ctx.prefs.read().click_history);
                    }
                }
                watch_foreground(handle.clone());
            }

            if !std::env::args().any(|a| a == "--minimized") {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            #[cfg(windows)]
            if let WindowEvent::Resized(_) = event
                && window.label() == "main"
                && let Ok(h) = window.hwnd()
            {
                win::strip_webview_scroll_styles(h.0 as isize);
                // Minimised: the HUD page's preview ends, as it does when the
                // page hears it went. WebView2 doesn't always tell the page.
                if !win::window_up(h.0 as isize)
                    && let Some(ctx) = window.app_handle().try_state::<Arc<Ctx>>()
                {
                    ctx.hud.test(false);
                }
            }
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
                #[cfg(windows)]
                meter::set_running(false, None);
                // Hidden to the tray, so nobody's looking at the HUD page's test.
                if let Some(ctx) = window.app_handle().try_state::<Arc<Ctx>>() {
                    ctx.hud.test(false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            windows_version,
            update_ready,
            prepare_update,
            apply_settings,
            set_profile,
            set_long_range,
            factory_reset,
            get_prefs,
            set_prefs,
            list_backups,
            create_backup,
            restore_backup,
            undo_restore,
            rename_backup,
            delete_backup,
            export_backup,
            import_backup,
            backup_settings,
            open_backups_folder,
            running_apps,
            app_info,
            installed_games,
            app_icon,
            tray_previews,
            app_status,
            hud_test,
            meter,
            meter_reset,
            meter_hold,
            capture_open,
            capture_update,
            capture_close,
            pad_state,
            click_history,
            pair_receiver,
            pad_calibrate,
            pad_clear,
            game_sensitivity,
            battery_summary,
            battery_history,
            pointer_settings,
            fix_pointer,
            test_popup,
        ])
        .run(context)
        .expect("error while running hrtz");
}

#[cfg(all(test, windows))]
mod lab {
    use std::path::PathBuf;

    use crate::device::{Battery, DeviceState};
    use crate::settings;
    use crate::store::Backups;

    /// Real data for the page labs (`lab-labpage.html`, `lab-backups.html`): this PC's backups
    /// decoded, a device state built from the newest of them, the pointer settings, and the games'
    /// sensitivity. Writes `src/lab/data/*.json`. Goes with the labs.
    /// `cargo test --lib lab::dump_lab_data -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_lab_data() {
        let data = PathBuf::from(std::env::var("APPDATA").unwrap()).join("com.azaek.hrtz");
        let out = PathBuf::from("../src/lab/data");
        std::fs::create_dir_all(&out).unwrap();
        let write = |name: &str, v: serde_json::Value| {
            std::fs::write(out.join(name), format!("{}\n", serde_json::to_string_pretty(&v).unwrap())).unwrap()
        };

        // The app's own backups, then the first one, made by scripts/backup.py before any change.
        let backups = Backups::new(data.join("backups"));
        let first = Backups::new(PathBuf::from("../../backups"));
        let list: Vec<serde_json::Value> = [&backups, &first]
            .iter()
            .flat_map(|b| {
                b.list().into_iter().map(|m| serde_json::json!({ "meta": m, "settings": b.settings(&m.id).unwrap() }))
            })
            .collect();
        let newest = backups.list().first().map(|m| backups.settings(&m.id).unwrap()).expect("a backup");
        write("backups.json", serde_json::Value::Array(list));

        // The battery's last reading, from the log the Battery page reads.
        let log = std::fs::read_to_string(data.join("battery_log.jsonl")).unwrap();
        let last: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        let level = last["level"].as_u64().unwrap() as u8;
        let state = DeviceState {
            rev: 1,
            connected: true,
            online: true,
            model: Some(settings::model_name(4).into()),
            cid: 7,
            mid: 4,
            device_type: 5,
            firmware: Some("2.17".into()),
            battery: Some(Battery {
                level,
                raw_level: level,
                charging: false,
                millivolts: last["mv"].as_u64().unwrap() as u16,
            }),
            profile: Some(0),
            long_range: Some(false),
            settings: Some(newest),
            capabilities: Some(settings::capabilities(4, 5)),
            // The stand-in backend says where the receiver is.
            port: None,
        };
        write("device.json", serde_json::to_value(&state).unwrap());

        let prefs: serde_json::Value =
            serde_json::from_slice(&std::fs::read(data.join("prefs.json")).unwrap()).unwrap();
        write("prefs.json", prefs);
        write("pointer.json", serde_json::to_value(crate::win::pointer_settings()).unwrap());
        let games: serde_json::Map<String, serde_json::Value> = ["VALORANT-Win64-Shipping.exe", "r5apex_dx12.exe"]
            .iter()
            .map(|exe| (exe.to_lowercase(), serde_json::to_value(crate::games::sensitivity(exe)).unwrap()))
            .collect();
        write("games.json", serde_json::Value::Object(games));
        // Installed games with their own icons, for the pad's game list.
        let program_data = PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()));
        let apps: Vec<serde_json::Value> = crate::installed::installed_games(crate::win::steam_path(), &program_data)
            .into_iter()
            .map(|g| {
                let name = if g.name.is_empty() { crate::win::app_name(&g.path) } else { g.name.clone() };
                serde_json::json!({ "exe": g.exe, "name": name, "path": g.path, "icon": crate::win::app_icon(&g.path) })
            })
            .collect();
        write("apps.json", serde_json::Value::Array(apps));
        eprintln!("wrote {}", out.canonicalize().unwrap().display());
    }
}
