//! The portable copy: one exe, `hrtz-portable.exe`, for people who'd rather not install. It keeps
//! everything in an `hrtz-data` folder beside it, its web view's data too, and updates itself in
//! place from its own signed manifest on the newest release. Built with `--features portable`
//! (release.yml); the installed copy never takes these paths.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;

/// Built as the portable copy.
pub const PORTABLE: bool = cfg!(feature = "portable");

/// The folder beside the exe that holds the portable copy's data.
pub const DATA_FOLDER: &str = "hrtz-data";

/// The portable copy's update manifest, on the newest release beside the installer's `latest.json`
/// (made by `scripts/portable-manifest.mjs`).
pub const MANIFEST_URL: &str = "https://github.com/azaek/hrtz/releases/latest/download/latest-portable.json";

/// Its entry in the manifest's `platforms`, a name the installer's `latest.json` never has.
pub const TARGET: &str = "windows-x86_64-portable";

/// Where the update check looks. A dev build can be pointed at a test manifest with
/// `HRTZ_PORTABLE_MANIFEST` (plain http works there); the exe it names still needs hrtz's signature.
pub fn manifest_url() -> String {
    std::env::var("HRTZ_PORTABLE_MANIFEST")
        .ok()
        .filter(|_| cfg!(debug_assertions))
        .unwrap_or_else(|| MANIFEST_URL.to_string())
}

/// Given to the new version when an update restarts hrtz, with the old one's process ID.
const AFTER_UPDATE: &str = "--after-update";

/// Where the portable copy keeps its data: `hrtz-data` beside the exe, made if needed. None for the
/// installed copy, and for a portable one in a folder it can't write to, which then uses the
/// installed copy's folders rather than not starting.
pub fn data_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        if !PORTABLE {
            return None;
        }
        let dir = std::env::current_exe().ok()?.parent()?.join(DATA_FOLDER);
        match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(dir),
            Err(e) => {
                eprintln!("portable: can't make {}, using the installed copy's folders: {e}", dir.display());
                None
            }
        }
    })
    .as_deref()
}

/// The web view's data (what the page keeps in localStorage, its cache), in the portable copy's
/// folder rather than in `%LOCALAPPDATA%`.
pub fn webview_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("webview"))
}

/// `hrtz.exe` with a suffix: `hrtz.exe.old`.
fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Puts a new version in this copy's place. A running exe can't be overwritten, but it can be
/// renamed: it steps aside as `<name>.old` and the new one takes its name, so the file the user
/// starts, a shortcut to it and Start with Windows all reach the new version. A step that fails
/// undoes the ones before it.
pub fn swap_in(exe: &Path, new_exe: &[u8]) -> std::io::Result<()> {
    let staged = beside(exe, ".new");
    let old = beside(exe, ".old");
    // Left by an update whose clean-up didn't get to it.
    let _ = std::fs::remove_file(&old);
    std::fs::write(&staged, new_exe)?;
    if let Err(e) = std::fs::rename(exe, &old) {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&staged, exe) {
        let _ = std::fs::rename(&old, exe);
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    Ok(())
}

/// The version an update replaced, once nothing runs it. Tried again for a minute, for an
/// antivirus still looking at it.
pub fn remove_old() {
    if !PORTABLE {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let old = beside(&exe, ".old");
    std::thread::spawn(move || {
        for wait in [0, 2, 10, 45] {
            std::thread::sleep(std::time::Duration::from_secs(wait));
            match std::fs::remove_file(&old) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => continue,
                _ => return,
            }
        }
    });
}

/// Starts the new version, which waits for this process to end before it takes the one-copy lock
/// (`wait_for_old`). Hidden to the tray if this one is.
pub fn start_new(exe: &Path, minimized: bool) -> std::io::Result<()> {
    let mut command = std::process::Command::new(exe);
    command.arg(AFTER_UPDATE).arg(std::process::id().to_string());
    if minimized {
        command.arg("--minimized");
    }
    command.spawn().map(|_| ())
}

/// In a version an update started: wait, up to 20 s, for the one it replaced to end, so hrtz
/// isn't sent to the old copy as a second start, and the mice aren't talked to by both.
pub fn wait_for_old() {
    let args: Vec<String> = std::env::args().collect();
    let Some(pid) = args.iter().position(|a| a == AFTER_UPDATE).and_then(|i| args.get(i + 1)?.parse::<u32>().ok())
    else {
        return;
    };
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 20_000);
            let _ = CloseHandle(process);
        }
    }
}

/// The portable copy has no installer to set up Microsoft's WebView2, which draws hrtz's window.
/// Windows 11 has it, as do most Windows 10 PCs; one without it hears why hrtz can't start and is
/// offered Microsoft's download page, instead of hrtz closing without a word. True when it's there.
pub fn webview_present() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::{
            IDOK, MB_ICONINFORMATION, MB_OKCANCEL, MessageBoxW, SW_SHOWNORMAL,
        };
        use windows::core::{HSTRING, PCWSTR, w};
        crate::i18n::set_language(None);
        let text = HSTRING::from(crate::i18n::tr(
            "hrtz needs Microsoft Edge WebView2 to show its window, and this PC doesn't have it. Choose OK to open Microsoft's download page, then start hrtz again.",
        ));
        if MessageBoxW(None, &text, w!("hrtz"), MB_OKCANCEL | MB_ICONINFORMATION) == IDOK {
            let url = HSTRING::from("https://developer.microsoft.com/microsoft-edge/webview2/");
            ShellExecuteW(None, w!("open"), &url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
    }
    false
}

/// Notifications on a PC without the installer. Windows shows a notification only from an app it
/// knows, which the installer's Start menu shortcut tells it; the portable copy tells it in the
/// registry instead (`HKCU\Software\Classes\AppUserModelId\<identifier>`), with its name and the
/// icon from its folder. Written each start, so a moved folder still has its icon. Left alone on a
/// PC where the installed copy's shortcut already does it.
pub fn register_for_notifications(identifier: &str) {
    let Some(dir) = data_dir() else { return };
    let installed = std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join(r"Microsoft\Windows\Start Menu\Programs\hrtz.lnk"))
        .is_some_and(|lnk| lnk.exists());
    if installed {
        return;
    }
    let icon = dir.join("hrtz.png");
    const PNG: &[u8] = include_bytes!("../icons/128x128.png");
    if std::fs::metadata(&icon).map(|m| m.len() != PNG.len() as u64).unwrap_or(true) {
        let _ = std::fs::write(&icon, PNG);
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
        use windows::core::HSTRING;
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{identifier}"));
        for (name, value) in [("DisplayName", "hrtz".to_string()), ("IconUri", icon.display().to_string())] {
            let data: Vec<u16> = value.encode_utf16().chain([0]).collect();
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &HSTRING::from(name),
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            );
        }
    }
}

/// A newer portable version: found by `check`, then its verified bytes once downloaded.
#[derive(Default)]
pub struct Pending {
    pub update: Mutex<Option<tauri_plugin_updater::Update>>,
    pub bytes: Mutex<Option<Vec<u8>>>,
}

/// The download's progress, shaped as the updater plugin's own events, so the page reads both alike.
#[derive(Serialize, Clone)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hrtz-portable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn swap_in_puts_the_new_version_in_place() {
        let dir = folder("swap");
        let exe = dir.join("hrtz-portable.exe");
        std::fs::write(&exe, b"0.6.0").unwrap();
        std::fs::write(beside(&exe, ".old"), b"0.5.0").unwrap();
        swap_in(&exe, b"0.6.1").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"0.6.1");
        assert_eq!(std::fs::read(beside(&exe, ".old")).unwrap(), b"0.6.0");
        assert!(!beside(&exe, ".new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_in_leaves_things_as_they_were_when_it_fails() {
        let dir = folder("fail");
        // No exe to step aside: the rename fails, and the staged copy goes.
        let exe = dir.join("hrtz-portable.exe");
        assert!(swap_in(&exe, b"0.6.1").is_err());
        assert!(!exe.exists());
        assert!(!beside(&exe, ".new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn download_events_read_as_the_plugins() {
        let json = |e: DownloadEvent| serde_json::to_string(&e).unwrap();
        assert_eq!(
            json(DownloadEvent::Started { content_length: Some(9) }),
            r#"{"event":"Started","data":{"contentLength":9}}"#
        );
        assert_eq!(
            json(DownloadEvent::Progress { chunk_length: 4 }),
            r#"{"event":"Progress","data":{"chunkLength":4}}"#
        );
        assert_eq!(json(DownloadEvent::Finished), r#"{"event":"Finished"}"#);
    }
}
