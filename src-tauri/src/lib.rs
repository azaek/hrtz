pub mod battery;
pub mod battery_log;
pub mod clicks;
pub mod diag;
pub mod games;
pub mod hud;
pub mod installed;
pub mod mice;
pub mod pad;
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
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use mice::model::Settings;
use mice::{DeviceEvent, DeviceState, Engine, Hub};
use store::{
    AutoReason, BackupDevice, BackupMeta, Backups, KnownMice, KnownMouse, Met, MousePrefs, Prefs, ProfileSnapshot,
};
use tray::{TrayLevel, TrayStyle};

const OSD_W: f64 = 280.0;
const OSD_H: f64 = 76.0;

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    foreground_exe: Option<String>,
    /// The mouse in your hand (its key): the tray, the HUD and the pop-ups follow it.
    in_use: Option<String>,
    /// The families whose maker's app is running (`compx` for Zaopin's, `hidpp` for G HUB).
    vendor_apps: Vec<&'static str>,
    /// The HUD's window is on screen now.
    hud_showing: bool,
}

struct Ctx {
    /// Every mouse connection, each on its own thread.
    hub: Arc<Hub>,
    /// Where each mouse's battery files are, for one that isn't connected.
    data_dir: PathBuf,
    prefs: RwLock<Prefs>,
    prefs_path: PathBuf,
    backups: Backups,
    status: RwLock<AppStatus>,
    osd_gen: AtomicU64,
    /// Mice the low battery alert has told about, until they charge.
    low_alerted: Mutex<HashSet<String>>,
    /// The battery saver is changing the polling rate.
    saver_busy: AtomicBool,
    tray: Mutex<Option<(TrayIcon, Vec<CheckMenuItem<tauri::Wry>>)>>,
    /// The tray's menu, for the "Restart to update" item a downloaded update adds.
    tray_menu: Mutex<Option<Menu<tauri::Wry>>>,
    /// The update already announced this run, so the notification comes once.
    update_ready: Mutex<Option<String>>,
    /// Profile switches to make, for `profile_switcher`: per mouse, the newest one wins.
    profiles: std::sync::mpsc::Sender<(String, u8)>,
    /// When the pop-up last said a profile, so the DPI and polling rate that came with it don't
    /// pop up after it.
    profile_osd_at: Mutex<Option<Instant>>,
    /// Last rendered (level, style, light taskbar), so the icon is only redrawn when it changes.
    tray_level: Mutex<Option<(TrayLevel, TrayStyle, bool)>>,
    hud: hud::Hud,
    /// Models (`mid`) checked this run for a first backup, so it's looked into once.
    /// The mice (keys) checked for their first backup this run.
    met: Mutex<HashSet<String>>,
    /// Mice (keys) with profile names renamed here and not on the mouse yet, and those being written.
    names_pending: Mutex<HashSet<String>>,
    names_writing: Mutex<HashSet<String>>,
    /// The mice hrtz has met, with what it keeps for each (`mice.json`).
    known: RwLock<KnownMice>,
    /// The mice connected now, so when one goes it's noted as last seen then.
    connected: Mutex<HashSet<String>>,
}

/// The app's status with the HUD's part filled in.
fn status_of(ctx: &Ctx) -> AppStatus {
    let mut s = ctx.status.read().clone();
    s.hud_showing = ctx.hud.showing();
    s.in_use = ctx.hub.primary_state().key;
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

/// The connected mouse's prefs: its profile names, rules and battery saver. The defaults when no
/// mouse is connected.
fn mouse_prefs(ctx: &Ctx, s: &DeviceState) -> MousePrefs {
    s.key.as_deref().map(|k| ctx.known.read().prefs(k)).unwrap_or_default()
}

/// Save the known mice after a change, and tell the window, the HUD and the tray.
fn commit_mice(app: &AppHandle, ctx: &Ctx) {
    let s = ctx.hub.primary_state();
    {
        let known = ctx.known.read();
        let _ = known.save();
        let _ = app.emit("mice", known.list().to_vec());
        let prefs = s.key.as_deref().map(|k| known.prefs(k)).unwrap_or_default();
        ctx.hud.mouse(&prefs, known.rule_exes());
    }
    refresh_tray(ctx, &s);
}

/// Change a mouse's prefs from the app's side: a profile snapshot, the battery saver.
fn commit_mouse_prefs(app: &AppHandle, ctx: &Ctx, key: &str, prefs: MousePrefs) {
    if ctx.known.write().set_prefs(key, prefs) {
        commit_mice(app, ctx);
    }
}

/// Note a mouse that connected, or the time one went.
fn meet(app: &AppHandle, ctx: &Ctx, s: &DeviceState) {
    let Some(key) = &s.key else { return };
    let now = unix_now();
    {
        let mut connected = ctx.connected.lock();
        if s.connected == connected.contains(key) {
            return;
        }
        if s.connected {
            connected.insert(key.clone());
            // A mouse never met opens the onboarding's second half; the Zaopin taking over what
            // was kept before several mice isn't new to anyone.
            if ctx.known.write().meet(key, s.model.as_deref().unwrap_or_default(), now) == Met::New {
                let _ = app.emit("mouse-new", key.clone());
            }
        } else {
            connected.remove(key);
            ctx.known.write().seen(key, now);
        }
    }
    commit_mice(app, ctx);
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// The engine running `mouse` (its key), or the primary mouse's when that's left out.
fn engine_of(ctx: &Ctx, mouse: Option<&str>) -> CmdResult<Engine> {
    match mouse {
        Some(key) => {
            ctx.hub.engine(key).ok_or_else(|| "That mouse isn't connected. Plug in its receiver or cable.".into())
        }
        None => ctx.hub.primary().ok_or_else(|| "No mouse connected. Plug in its receiver or cable.".into()),
    }
}

/// Run `f` on `mouse`'s thread (the primary mouse's when left out), off the command's own.
async fn on_device<T: Send + 'static>(
    ctx: &Ctx,
    mouse: Option<&str>,
    f: impl FnOnce(&mut mice::Worker) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let engine = engine_of(ctx, mouse)?;
    tauri::async_runtime::spawn_blocking(move || engine.call(f)).await.map_err(|e| e.to_string())?.map_err(err)
}

// ---- commands -------------------------------------------------------------------------------

/// The primary mouse's state.
#[tauri::command]
fn get_state(ctx: State<'_, Arc<Ctx>>) -> DeviceState {
    ctx.hub.primary_state()
}

/// Every connection's state.
#[tauri::command]
fn get_states(ctx: State<'_, Arc<Ctx>>) -> Vec<DeviceState> {
    ctx.hub.states()
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
async fn apply_settings(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, settings: Settings) -> CmdResult<Settings> {
    on_device(&ctx, mouse.as_deref(), move |w| w.apply_settings(&settings)).await
}

#[tauri::command]
async fn set_profile(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, index: u8) -> CmdResult<()> {
    on_device(&ctx, mouse.as_deref(), move |w| w.set_profile(index)).await
}

#[tauri::command]
async fn set_long_range(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, on: bool) -> CmdResult<()> {
    on_device(&ctx, mouse.as_deref(), move |w| w.set_long_range(on)).await
}

/// Pair a mouse with the receiver: up to 30 s, with `pairing` events counting down. True when one
/// paired.
#[tauri::command]
async fn pair_receiver(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>) -> CmdResult<bool> {
    on_device(&ctx, mouse.as_deref(), |w| w.pair()).await
}

/// Factory settings, after saving the mouse as it is, so the setup can be got back from Backups.
#[tauri::command]
async fn factory_reset(app: AppHandle, ctx: State<'_, Arc<Ctx>>, mouse: Option<String>) -> CmdResult<()> {
    let backups = ctx.backups.clone();
    on_device(&ctx, mouse.as_deref(), move |w| {
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

/// The mice hrtz has met, each with its prefs.
#[tauri::command]
fn get_mice(ctx: State<'_, Arc<Ctx>>) -> Vec<KnownMouse> {
    ctx.known.read().list().to_vec()
}

/// Every mouse hrtz can run, by family, and whether each was tried on the real one.
#[tauri::command]
fn supported_mice() -> Vec<mice::model::Supported> {
    let mut all = mice::compx::models::supported();
    all.extend(mice::hidpp::models::supported());
    all
}

/// Remove mouse `key`, which isn't connected, from hrtz: what's kept for it (its prefs, its
/// battery history) goes, and it's met as new if it comes back. Returns what was kept, for Undo.
/// Its battery files are set aside until hrtz next starts.
#[tauri::command]
fn forget_mouse(app: AppHandle, ctx: State<'_, Arc<Ctx>>, key: String) -> CmdResult<KnownMouse> {
    if ctx.hub.engine(&key).is_some_and(|e| e.snapshot().connected) {
        return Err("Unplug it first: a connected mouse comes straight back.".into());
    }
    let gone = ctx.known.write().forget(&key).ok_or("hrtz doesn't know this mouse.")?;
    set_aside_battery(&ctx.data_dir, &key, true);
    commit_mice(&app, &ctx);
    Ok(gone)
}

/// Put back a mouse just removed (Undo), with its battery history.
#[tauri::command]
fn restore_mouse(app: AppHandle, ctx: State<'_, Arc<Ctx>>, mouse: KnownMouse) -> CmdResult<()> {
    let key = mouse.key.clone();
    if ctx.known.write().restore(mouse) {
        set_aside_battery(&ctx.data_dir, &key, false);
        commit_mice(&app, &ctx);
    }
    Ok(())
}

/// Mouse `key`'s battery files, set aside as `.removed` or put back.
fn set_aside_battery(data_dir: &std::path::Path, key: &str, aside: bool) {
    let (estimate, log) = mice::engine::battery_files(data_dir, key);
    for f in [estimate, log] {
        let removed = PathBuf::from(format!("{}.removed", f.display()));
        let (from, to) = if aside { (&f, &removed) } else { (&removed, &f) };
        if from.exists() {
            let _ = std::fs::rename(from, to);
        }
    }
}

/// What removed mice left set aside: past Undo once hrtz starts again.
fn clear_removed(data_dir: &std::path::Path) {
    let Ok(dir) = std::fs::read_dir(data_dir.join("battery")) else { return };
    for f in dir.flatten() {
        if f.file_name().to_string_lossy().ends_with(".removed") {
            let _ = std::fs::remove_file(f.path());
        }
    }
}

#[tauri::command]
fn set_mouse_prefs(app: AppHandle, ctx: State<'_, Arc<Ctx>>, key: String, prefs: MousePrefs) -> CmdResult<()> {
    let renamed = ctx.known.read().prefs(&key).profile_names != prefs.profile_names;
    if ctx.known.write().set_prefs(&key, prefs) {
        commit_mice(&app, &ctx);
    }
    // A mouse that keeps names gets them too, now or once it's awake.
    if renamed && mice::family_of(&key) == Some("hidpp") {
        ctx.names_pending.lock().insert(key.clone());
        push_names(&ctx, &key);
    }
    Ok(())
}

/// Write mouse `key`'s names from hrtz to the mouse, if it's awake and they're waiting to go.
fn push_names(ctx: &Arc<Ctx>, key: &str) {
    let awake = ctx.hub.engine(key).is_some_and(|e| e.snapshot().online);
    if !awake || !ctx.names_pending.lock().contains(key) || !ctx.names_writing.lock().insert(key.to_string()) {
        return;
    }
    let names = ctx.known.read().prefs(key).profile_names;
    let (ctx, key) = (ctx.clone(), key.to_string());
    tauri::async_runtime::spawn(async move {
        let done = on_device(&ctx, Some(&key), move |w| w.set_profile_names(&names)).await;
        ctx.names_writing.lock().remove(&key);
        match done {
            Ok(()) => {
                ctx.names_pending.lock().remove(&key);
            }
            // Kept waiting: tried again when the mouse next reports in.
            Err(e) => eprintln!("profile names not saved on the mouse yet: {e}"),
        }
    });
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
    refresh_tray(&ctx, &ctx.hub.primary_state());
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
        key: s.key.clone(),
        model: s.model.clone(),
        wired: s.capabilities.as_ref().is_some_and(|c| c.wired),
    }
}

#[tauri::command]
async fn create_backup(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, note: String) -> CmdResult<BackupMeta> {
    let (image, s) = on_device(&ctx, mouse.as_deref(), |w| {
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
async fn restore_backup(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, id: String) -> CmdResult<Restored> {
    let (file, image) = ctx.backups.load(&id).map_err(err)?;
    let current = engine_of(&ctx, mouse.as_deref())?.snapshot();
    // The same model: by key, or for a backup from before keys, a Zaopin of its model.
    let same = match (file.device.model_key(), current.key.as_deref()) {
        (Some(theirs), Some(ours)) => theirs == store::model_key(ours),
        (None, Some(ours)) => mice::family_of(ours) == Some("compx") && current.mid == file.device.mid,
        (_, None) => true,
    };
    if current.connected && !same {
        let theirs =
            file.device.model.clone().unwrap_or_else(|| mice::compx::models::model_name(file.device.mid).into());
        return Err(format!(
            "This backup is for a {theirs}, but a {} is connected.",
            current.model.unwrap_or_default()
        ));
    }
    // One read serves both: the backup of the mouse as it is, and what to compare against, so
    // only the chunks that differ are written.
    let backups = ctx.backups.clone();
    let reason = AutoReason::Restore { note: file.note.clone(), created: file.created.clone() };
    on_device(&ctx, mouse.as_deref(), move |w| {
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
async fn undo_restore(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, id: String) -> CmdResult<usize> {
    let (file, image) = ctx.backups.load(&id).map_err(err)?;
    let written = on_device(&ctx, mouse.as_deref(), move |w| w.restore_image(&image)).await?;
    // Only while it's still hrtz's own: one given a name since is yours, and stays.
    if file.auto.is_some() {
        ctx.backups.delete(&id).map_err(err)?;
    }
    Ok(written)
}

/// A test report's reading of mouse `mouse`: its memory and what its family adds (`Worker::report`).
#[tauri::command]
async fn mouse_report(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>) -> CmdResult<serde_json::Value> {
    on_device(&ctx, mouse.as_deref(), |w| w.report()).await
}

/// Write a test report where the tester picked.
#[tauri::command]
fn save_report(path: String, text: String) -> CmdResult<()> {
    std::fs::write(&path, text).map_err(|e| e.to_string())
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
    let mut level = tray_level(&ctx.hub.primary_state(), &prefs);
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
fn battery_summary(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>) -> battery_log::Summary {
    battery_log::summary(battery_log_of(&ctx, mouse.as_deref()).lock().entries(), unix_now())
}

/// Battery log rows from the last `days`, thinned for a chart.
#[tauri::command]
fn battery_history(ctx: State<'_, Arc<Ctx>>, mouse: Option<String>, days: u32) -> Vec<battery_log::Entry> {
    battery_log_of(&ctx, mouse.as_deref()).lock().history(unix_now().saturating_sub(days as u64 * 86_400), 720)
}

/// Lower the polling rate while the battery is low, and put back the rate you chose once it's
/// charging (or the saver is turned off). Each change is written to the mouse's memory, so this
/// switches about twice a charge, never back and forth.
/// `mouse`'s battery log (the primary mouse's when left out, else the last one seen): live while
/// it's connected, read from its file when it isn't.
fn battery_log_of(ctx: &Ctx, mouse: Option<&str>) -> Arc<Mutex<battery_log::BatteryLog>> {
    let engine = match mouse {
        Some(key) => ctx.hub.engine(key),
        None => ctx.hub.primary().filter(|e| e.snapshot().key.is_some()),
    };
    if let Some(e) = engine {
        return e.log.clone();
    }
    let key = mouse.map(str::to_string).or_else(|| ctx.known.read().last_seen().map(|m| m.key.clone()));
    let path = key.map(|k| mice::engine::battery_files(&ctx.data_dir, &k).1);
    Arc::new(Mutex::new(battery_log::BatteryLog::load(path)))
}

fn battery_saver(app: &AppHandle, ctx: &Arc<Ctx>, s: &DeviceState, prefs: &MousePrefs) {
    let (Some(key), Some(b), Some(hz), Some(caps)) =
        (&s.key, &s.battery, s.settings.as_ref().and_then(|x| x.polling_hz), &s.capabilities)
    else {
        return;
    };
    if !s.online || caps.wired || caps.polling.is_none() || ctx.saver_busy.load(Ordering::SeqCst) {
        return;
    }
    let Some((target, restore)) = prefs.saver_step(b.level, b.charging, hz) else { return };
    let lowering = restore.is_some() && target.is_some();
    let (from, level, model) = (hz, b.level, s.model.clone().unwrap_or_default());
    let (app, ctx, key) = (app.clone(), ctx.clone(), key.clone());
    ctx.saver_busy.store(true, Ordering::SeqCst);
    thread::spawn(move || {
        let applied = match target {
            Some(hz) => {
                // Only the rate: the rest stays as the mouse has it.
                let next = Settings { polling_hz: Some(hz), ..Default::default() };
                match ctx.hub.engine(&key) {
                    Some(e) => e.call(move |w| w.apply_settings(&next)).map(|_| ()),
                    None => Ok(()),
                }
            }
            None => Ok(()),
        };
        match applied {
            Ok(()) => {
                let mut p = ctx.known.read().prefs(&key);
                p.battery_saver_restore = restore;
                commit_mouse_prefs(&app, &ctx, &key, p);
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
    let s = ctx.hub.primary_state();
    let st = s.settings.as_ref();
    let model = s.model.clone().unwrap_or_else(|| "mouse".into());
    let level = s.battery.as_ref().map_or(15, |b| b.level);
    let hz = st.and_then(|x| x.polling_hz).unwrap_or(8000);
    let mouse = mouse_prefs(&ctx, &s);
    let event = match kind.as_str() {
        "dpi" => Some(DeviceEvent::Dpi {
            stage: st.and_then(|x| x.dpi_stage).unwrap_or(0),
            dpi: st.and_then(|x| x.dpi()).unwrap_or(1600),
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
                level.min(mouse.battery_saver_below),
                mouse.battery_saver_hz,
                hz.max(mouse.battery_saver_hz * 2),
                &model,
            );
            None
        }
        other => return Err(format!("No pop-up called {other}")),
    };
    if let Some(e) = event {
        show_osd(&app, &ctx, &e, &prefs, &mouse);
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
    if let Some((tray, items)) = ctx.tray.lock().as_mut() {
        let mut last = ctx.tray_level.lock();
        if *last != Some(key) {
            let _ = tray.set_icon(Some(tray_image(key.0, key.1)));
            *last = Some(key);
        }
        let _ = tray.set_tooltip(Some(tooltip(s)));
        if let Some(n) = s.capabilities.as_ref().and_then(|c| c.profiles) {
            fit_tray_profiles(ctx, tray, items, (n as usize).clamp(1, 8));
        }
        let mouse = mouse_prefs(ctx, s);
        for (i, item) in items.iter().enumerate() {
            let _ = item.set_text(mouse.profile_name(i as u8));
            let _ = item.set_checked(s.profile == Some(i as u8));
            let _ = item.set_enabled(s.online);
        }
    }
}

/// The tray lists as many profiles as the mouse shown has: items past them go, new ones go after
/// the last.
fn fit_tray_profiles(ctx: &Ctx, tray: &TrayIcon, items: &mut Vec<CheckMenuItem<tauri::Wry>>, n: usize) {
    let Some(menu) = ctx.tray_menu.lock().clone() else { return };
    while items.len() > n {
        if let Some(item) = items.pop() {
            let _ = menu.remove(&item);
        }
    }
    while items.len() < n {
        let i = items.len();
        let Ok(item) = CheckMenuItem::with_id(
            tray.app_handle(),
            format!("profile-{i}"),
            format!("Profile {}", i + 1),
            false,
            false,
            None::<&str>,
        ) else {
            return;
        };
        let after = items.last().map(|l| l.id().clone());
        let at = after.and_then(|id| menu.items().ok()?.iter().position(|x| *x.id() == id)).map_or(2, |p| p + 1);
        if menu.insert(&item, at).is_err() {
            return;
        }
        items.push(item);
    }
}

fn tooltip(s: &DeviceState) -> String {
    let model = s.model.clone().unwrap_or_else(|| "Mouse".into());
    match (&s.battery, s.connected, s.online) {
        (_, false, _) => "hrtz: no mouse connected".into(),
        (Some(b), true, true) => {
            let hz = s.settings.as_ref().and_then(|x| x.polling_hz).map(|hz| format!(" · {hz} Hz")).unwrap_or_default();
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

    meet(app, &ctx, s);
    // What it holds, for its pages while it's away: saved when it goes.
    if let (true, Some(key), Some(_)) = (s.connected, &s.key, &s.settings) {
        ctx.known.write().note_last(key, s);
    }
    follow_primary(app, &ctx);
    first_meeting(app, &ctx, s);
    let mut mouse = mouse_prefs(&ctx, s);
    // A name and a snapshot for each of its profiles.
    if let (Some(key), Some(n)) = (&s.key, s.capabilities.as_ref().and_then(|c| c.profiles)) {
        let mut next = mouse.clone();
        if next.fit_profiles(n) {
            commit_mouse_prefs(app, &ctx, key, next.clone());
            mouse = next;
        }
    }
    // Names the mouse keeps (set in G HUB, say) become hrtz's, unless a rename here is on its way.
    if let (true, Some(key), Some(names)) = (s.online, &s.key, &s.profile_names) {
        if ctx.names_pending.lock().contains(key) {
            push_names(&ctx, key);
        } else {
            let mut next = mouse.clone();
            if next.take_names(names) {
                commit_mouse_prefs(app, &ctx, key, next.clone());
                mouse = next;
            }
        }
    }

    // Remember what the profile in use holds: the mouse reports only that one, and the Profiles
    // page shows the rest from these.
    if let (true, Some(key), Some(i), Some(st), Some(caps)) =
        (s.online, &s.key, s.profile, &s.settings, &s.capabilities)
        && let Some(snap) = ProfileSnapshot::of(s.mid, st, caps.buttons.as_deref().unwrap_or(&[]))
        && mouse.profile_snapshots.get(i as usize).is_some_and(|x| x.as_ref() != Some(&snap))
    {
        let mut next = mouse.clone();
        if next.note_profile(i, snap) {
            commit_mouse_prefs(app, &ctx, key, next);
        }
    }

    if let (Some(b), Some(key)) = (&s.battery, &s.key) {
        let low = b.level <= prefs.low_battery_percent && !b.charging;
        if low && prefs.battery_alerts && ctx.low_alerted.lock().insert(key.clone()) {
            notify_low_battery(app, b.level, &s.model.clone().unwrap_or_default());
        }
        if b.charging || b.level > prefs.low_battery_percent + 5 {
            ctx.low_alerted.lock().remove(key);
        }
    }
    battery_saver(app, &ctx, s, &mouse);

    match event {
        Some(DeviceEvent::Progress { done, total }) => {
            let _ = app.emit("device-progress", (done, total));
        }
        Some(DeviceEvent::Pairing { seconds_left }) => {
            let _ = app.emit("pairing", seconds_left);
        }
        Some(e) if prefs.osd => show_osd(app, &ctx, &e, &prefs, &mouse),
        _ => {}
    }
}

/// The tray, the HUD and pad tracking follow the primary mouse (the one in use). The meter is told
/// where every connected mouse is, so input can be put down to one, and follows input while two or
/// more are connected.
fn follow_primary(app: &AppHandle, ctx: &Ctx) {
    #[cfg(windows)]
    {
        let states = ctx.hub.states();
        let at: Vec<(String, String)> = states
            .iter()
            .filter(|s| s.connected)
            .filter_map(|s| Some((s.port.as_ref()?.path.clone(), s.key.clone()?)))
            .collect();
        // Two mice to tell apart, or one that's told awake by being used.
        let watch = at.len() >= 2
            || states.iter().any(|s| s.connected && s.capabilities.as_ref().is_some_and(|c| c.wake_by_input));
        meter::set_mice(at);
        let _ = app.run_on_main_thread(move || meter::set_watch(watch));
    }
    let Some(primary) = ctx.hub.primary() else { return };
    let p = primary.snapshot();
    // Pad tracking turns sensor counts into centimetres at the DPI in use, and reads pauses
    // differently when the sensor keeps tracking as the mouse rises.
    #[cfg(windows)]
    if let Some(st) = &p.settings {
        meter::set_dpi(st.dpi().unwrap_or(0));
        if let Some(lod) = st.lod {
            meter::set_lift_off(lod != mice::model::Lod::Low);
        }
    }
    refresh_tray(ctx, &p);
    ctx.hud.device(&p);
    ctx.hud.log(primary.log.clone());
    let known = ctx.known.read();
    let prefs = p.key.as_deref().map(|k| known.prefs(k)).unwrap_or_default();
    ctx.hud.mouse(&prefs, known.rule_exes());
}

/// The first time hrtz meets a mouse (no backup of its model yet), it saves one as it found it,
/// before anything here changes it. Checked once per model per run, while it's awake and the
/// vendor app isn't talking to it too. The read is quiet: nothing the user started.
fn first_meeting(app: &AppHandle, ctx: &Arc<Ctx>, s: &DeviceState) {
    let backups = s.capabilities.as_ref().is_some_and(|c| c.backups);
    if !backups
        || !s.online
        || s.settings.is_none()
        || s.key.as_deref().and_then(mice::family_of).is_some_and(|f| ctx.status.read().vendor_apps.contains(&f))
    {
        return;
    }
    let Some(key) = s.key.clone() else { return };
    if !ctx.met.lock().insert(key.clone()) {
        return;
    }
    // Its own, or (from before several mice, with no key) one of its model's.
    let model = s.model.clone().unwrap_or_default();
    if ctx.backups.list().iter().any(|b| b.key.as_deref().map_or(b.model == model, |k| k == key)) {
        return;
    }
    let (app, ctx) = (app.clone(), ctx.clone());
    tauri::async_runtime::spawn(async move {
        let backups = ctx.backups.clone();
        let saved = on_device(&ctx, Some(&key), move |w| {
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
                ctx.met.lock().remove(&key);
            }
        }
    });
}

fn show_osd(app: &AppHandle, ctx: &Ctx, e: &DeviceEvent, prefs: &Prefs, mouse: &MousePrefs) {
    // A profile switch brings its own DPI and polling rate: the pop-up says the profile, not all
    // three in a row.
    match e {
        DeviceEvent::Profile { .. } => *ctx.profile_osd_at.lock() = Some(Instant::now()),
        DeviceEvent::Dpi { .. } | DeviceEvent::Polling { .. }
            if ctx.profile_osd_at.lock().is_some_and(|t| t.elapsed() < Duration::from_millis(1500)) =>
        {
            return;
        }
        _ => {}
    }
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
        DeviceEvent::Profile { index } => {
            Osd { kind: "profile", title: mouse.profile_name(*index), detail: format!("Profile {}", index + 1) }
        }
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
            let mut focus = FocusSettle::default();
            let mut ticks = 0u32;
            loop {
                // Four looks a second at the front window; the rest runs on multiples of this.
                thread::sleep(Duration::from_millis(250));
                let Some(ctx) = app.try_state::<Arc<Ctx>>() else { continue };
                ticks += 1;
                if ticks.is_multiple_of(12) {
                    refresh_tray(&ctx, &ctx.hub.primary_state());
                }
                if ticks.is_multiple_of(8)
                    && let Some(h) = app.get_webview_window("main").and_then(|w| w.hwnd().ok())
                {
                    win::strip_webview_scroll_styles(h.0 as isize);
                }
                if ticks % 40 == 1 {
                    let running = win::vendor_apps_running();
                    let changed = ctx.status.read().vendor_apps != running;
                    if changed {
                        // The HUD's note is about Zaopin's app, which makes its values late.
                        ctx.hud.vendor_app(running.contains(&"compx"));
                        ctx.status.write().vendor_apps = running;
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
                    if let (Some(p), Some(e), false) = (&path, &exe, ours)
                        && ctx.known.write().fill_rule(e, &win::app_name(p), p)
                    {
                        commit_mice(&app, &ctx);
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
                if exe.is_none() || ours {
                    continue;
                }
                if exe != last_exe {
                    last_exe = exe.clone();
                    ctx.status.write().foreground_exe = exe.clone();
                    let _ = app.emit("app-status", status_of(&ctx));
                }
                // Only an app that stays in front switches the profile, and only once: alt-tabbing
                // through windows switches nothing.
                let Some(settled) = focus.see(exe.as_deref(), Instant::now()) else { continue };
                // Each connected mouse switches by its own rules.
                for s in ctx.hub.states() {
                    let mouse = mouse_prefs(&ctx, &s);
                    if let (true, Some(key), true) = (s.connected, &s.key, mouse.rules_enabled)
                        && let Some(want) = mouse.profile_for(&settled)
                    {
                        let _ = ctx.profiles.send((key.clone(), want));
                    }
                }
            }
        })
        .expect("spawn foreground watcher");
}

/// How long an app has to stay in front before its profile applies.
const SETTLE: Duration = Duration::from_millis(200);

/// Which app's profile to apply as focus moves. An app counts once it has been in front for
/// `SETTLE`, so alt-tabbing through windows (or past the Alt+Tab switcher) switches nothing, and
/// coming back to the app already applied switches nothing either.
#[derive(Default)]
struct FocusSettle {
    seen: Option<(String, Instant)>,
    applied: Option<String>,
}

impl FocusSettle {
    /// The app in front now; returns it once it has settled and isn't the one applied already.
    fn see(&mut self, exe: Option<&str>, now: Instant) -> Option<String> {
        let exe = exe?;
        match &self.seen {
            Some((e, _)) if e == exe => {}
            _ => {
                self.seen = Some((exe.to_string(), now));
                return None;
            }
        }
        let since = self.seen.as_ref()?.1;
        if self.applied.as_deref() == Some(exe) || now.duration_since(since) < SETTLE {
            return None;
        }
        self.applied = Some(exe.to_string());
        Some(exe.to_string())
    }
}

/// Makes profile switches one at a time. A switch holds the mouse for about a second while the new
/// profile's settings load, so asks that arrive meanwhile replace each other: only the newest runs.
fn profile_switcher(app: AppHandle, asks: std::sync::mpsc::Receiver<(String, u8)>) {
    thread::Builder::new()
        .name("hrtz-profiles".into())
        .spawn(move || {
            while let Ok(first) = asks.recv() {
                // The newest ask for each mouse.
                let mut wants = vec![first];
                while let Ok(newer) = asks.try_recv() {
                    wants.retain(|(k, _)| *k != newer.0);
                    wants.push(newer);
                }
                let Some(ctx) = app.try_state::<Arc<Ctx>>() else { continue };
                for (key, want) in wants {
                    let Some(engine) = ctx.hub.engine(&key) else { continue };
                    let s = engine.snapshot();
                    if !s.online || s.profile.is_none() || s.profile == Some(want) {
                        continue;
                    }
                    if let Err(e) = engine.call(move |w| w.set_profile(want)) {
                        eprintln!("profile switch failed: {e:#}");
                    }
                }
            }
        })
        .expect("spawn profile switcher");
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
    // The last mouse's names until one connects; `refresh_tray` keeps them current after.
    let names = ctx.known.read().last_seen().map(|m| m.prefs.clone()).unwrap_or_default();
    let profiles: Vec<CheckMenuItem<tauri::Wry>> = (0..4u8)
        .map(|i| CheckMenuItem::with_id(app, format!("profile-{i}"), names.profile_name(i), false, false, None::<&str>))
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
                if let Some(i) = id.strip_prefix("profile-").and_then(|n| n.parse::<u8>().ok())
                    && let Some(ctx) = app.try_state::<Arc<Ctx>>()
                    && let Some(key) = ctx.hub.primary_state().key
                {
                    let _ = ctx.profiles.send((key, i));
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
        for e in ctx.hub.engines() {
            let _ = e.call(|w| {
                w.goodbye();
                Ok(())
            });
        }
        // What each connected mouse holds, for its pages next time.
        let _ = ctx.known.read().save();
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
            clear_removed(&data_dir);
            // What a prefs file from before several mice held for its mouse is kept for the first
            // Zaopin to connect, before the prefs are saved without it.
            let (prefs, legacy) = Prefs::load_with_legacy(&prefs_path);
            let mut known = KnownMice::load(Some(config_dir.join("mice.json")));
            if let Some(legacy) = legacy {
                if known.hold_legacy(legacy) {
                    known.save()?;
                }
                // The file as it was, once, in case it's wanted by an older hrtz.
                let before = config_dir.join("prefs.before-mice.json");
                if !before.exists() {
                    let _ = std::fs::copy(&prefs_path, before);
                }
                prefs.save(&prefs_path)?;
            }
            let hub = Hub::new();
            let hud = hud::Hud::new(&prefs);
            let (profiles, profile_asks) = std::sync::mpsc::channel();
            let ctx = Arc::new(Ctx {
                hub: hub.clone(),
                data_dir: data_dir.clone(),
                prefs: RwLock::new(prefs),
                prefs_path,
                backups: Backups::new(data_dir.join("backups")),
                status: RwLock::new(AppStatus::default()),
                osd_gen: AtomicU64::new(0),
                low_alerted: Mutex::new(HashSet::new()),
                saver_busy: AtomicBool::new(false),
                tray: Mutex::new(None),
                tray_menu: Mutex::new(None),
                update_ready: Mutex::new(None),
                profiles,
                profile_osd_at: Mutex::new(None),
                tray_level: Mutex::new(None),
                hud,
                met: Mutex::new(HashSet::new()),
                names_pending: Mutex::new(HashSet::new()),
                names_writing: Mutex::new(HashSet::new()),
                known: RwLock::new(known),
                connected: Mutex::new(HashSet::new()),
            });
            build_tray(&handle, &ctx)?;
            app.manage(ctx.clone());
            profile_switcher(handle.clone(), profile_asks);
            // Its reports need the state managed above.
            let hud_handle = handle.clone();
            ctx.hud.start(move |r| on_hud_report(&hud_handle, r));
            ctx.hud.device(&ctx.hub.primary_state());
            {
                let known = ctx.known.read();
                let last = known.last_seen().map(|m| m.prefs.clone()).unwrap_or_default();
                ctx.hud.mouse(&last, known.rule_exes());
            }

            // Mice are looked for once everything that hears about them is ready.
            #[allow(unused_mut)]
            let mut families: Vec<Box<dyn mice::Family>> =
                vec![Box::new(mice::compx::Compx), Box::new(mice::hidpp::Logitech)];
            #[cfg(debug_assertions)]
            {
                if let Some(sim) = mice::compx::sim::SimFamily::from_env() {
                    families.push(Box::new(sim));
                }
                if let Some(sim) = mice::hidpp::sim::SimFamily::from_env() {
                    families.push(Box::new(sim));
                }
            }
            hub.run(families, Arc::new(move |s, e| on_device_change(&notify_handle, s, e)), Some(data_dir.clone()));
            // The mouse moved last is the one in use: the tray, the HUD and the pop-ups go to it.
            #[cfg(windows)]
            {
                let in_use_handle = handle.clone();
                meter::on_in_use(move |key| {
                    let app = in_use_handle.clone();
                    thread::spawn(move || {
                        let Some(ctx) = app.try_state::<Arc<Ctx>>() else { return };
                        if ctx.hub.set_in_use(&key) {
                            follow_primary(&app, &ctx);
                            let _ = app.emit("app-status", status_of(&ctx));
                        }
                    });
                });
            }

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
            get_states,
            windows_version,
            update_ready,
            prepare_update,
            apply_settings,
            set_profile,
            set_long_range,
            factory_reset,
            get_prefs,
            get_mice,
            supported_mice,
            forget_mouse,
            restore_mouse,
            mouse_report,
            save_report,
            set_mouse_prefs,
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

#[cfg(test)]
mod focus {
    use super::FocusSettle;
    use std::time::{Duration, Instant};

    /// Looks at the front window every 250 ms, as the watcher does: one app per look.
    fn run(apps: &[&str]) -> Vec<String> {
        let start = Instant::now();
        let mut f = FocusSettle::default();
        apps.iter()
            .enumerate()
            .filter_map(|(i, a)| f.see(Some(a), start + Duration::from_millis(250 * i as u64)))
            .collect()
    }

    #[test]
    fn an_app_that_stays_in_front_switches_once() {
        assert_eq!(run(&["game.exe"; 8]), ["game.exe"]);
    }

    #[test]
    fn alt_tabbing_back_and_forth_switches_nothing_more() {
        // Settled on the game, then quick tabs to Discord and back, for two seconds.
        let mut looks = vec!["game.exe"; 3];
        for _ in 0..4 {
            looks.extend(["discord.exe", "game.exe"]);
        }
        assert_eq!(run(&looks), ["game.exe"]);
    }

    #[test]
    fn a_detour_that_settles_switches_and_comes_back() {
        let looks = [
            "game.exe",
            "game.exe",
            "game.exe",
            "discord.exe",
            "discord.exe",
            "discord.exe",
            "game.exe",
            "game.exe",
            "game.exe",
        ];
        assert_eq!(run(&looks), ["game.exe", "discord.exe", "game.exe"]);
    }
}

#[cfg(all(test, windows))]
mod lab {
    use std::path::PathBuf;

    use crate::mice::compx::models;
    use crate::mice::{self, Battery, DeviceState};
    use crate::store::{Backups, KnownMice, Prefs};
    use crate::unix_now;

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

        // The battery's last reading, from the log the Battery page reads: the Zaopin's own, or
        // the one from before several mice.
        let (_, own_log) = mice::engine::battery_files(&data, "compx/7-4");
        let log = std::fs::read_to_string(&own_log)
            .or_else(|_| std::fs::read_to_string(data.join("battery_log.jsonl")))
            .unwrap();
        let last: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
        let level = last["level"].as_u64().unwrap() as u8;
        let state = DeviceState {
            rev: 1,
            key: Some("compx/7-4".into()),
            connected: true,
            online: true,
            model: Some(models::model_name(4).into()),
            cid: 7,
            mid: 4,
            device_type: 5,
            variant: None,
            firmware: Some("2.17".into()),
            battery: Some(Battery {
                level,
                raw_level: level,
                charging: false,
                millivolts: last["mv"].as_u64().unwrap() as u16,
            }),
            profile: Some(0),
            profile_names: None,
            long_range: Some(false),
            host_mode: false,
            settings: Some(newest),
            capabilities: Some(models::capabilities(4, 5)),
            // The stand-in backend says where the receiver is.
            port: None,
        };
        write("device.json", serde_json::to_value(&state).unwrap());

        let (prefs, legacy) = Prefs::load_with_legacy(&data.join("prefs.json"));
        write("prefs.json", serde_json::to_value(&prefs).unwrap());
        // The known mice; on a PC that hasn't run this version yet, the Zaopin with the prefs it
        // would take over.
        let mut known = KnownMice::load(Some(data.join("mice.json")));
        if known.list().is_empty() {
            known.hold_legacy(legacy.unwrap_or_default());
            known.meet("compx/7-4", models::model_name(4), unix_now());
        }
        write("mice.json", serde_json::to_value(known.list()).unwrap());
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
