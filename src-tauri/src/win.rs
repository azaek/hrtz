//! Small Win32 helpers: foreground process, running-app list with names and icons, vendor-app
//! detection, and showing a window without focus.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use base64::Engine as _;
use parking_lot::Mutex;
use serde::Serialize;
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumChildWindows, EnumWindows, GetForegroundWindow, GetIconInfo, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, ShowWindow, GWL_EXSTYLE,
    GW_OWNER, ICONINFO, SW_SHOWNOACTIVATE, WS_EX_TOOLWINDOW,
};

const VENDOR_EXE: &str = "mouse drive beta.exe";
/// Hosts whose windows belong to another process (Store/UWP apps run inside it).
const FRAME_HOST: &str = "applicationframehost.exe";

fn path_of_pid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Executable path behind a top-level window, looking through ApplicationFrameHost to the app it hosts.
fn path_of_window(hwnd: HWND) -> Option<String> {
    let host = pid_of(hwnd);
    let path = path_of_pid(host)?;
    if !file_name(&path).eq_ignore_ascii_case(FRAME_HOST) {
        return Some(path);
    }
    unsafe extern "system" fn child(hwnd: HWND, lp: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lp.0 as *mut (u32, u32)) };
        let pid = pid_of(hwnd);
        if pid != 0 && pid != found.0 {
            found.1 = pid;
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut found = (host, 0u32);
    unsafe {
        let _ = EnumChildWindows(Some(hwnd), Some(child), LPARAM(&mut found as *mut _ as isize));
    }
    (found.1 != 0).then(|| path_of_pid(found.1)).flatten()
}

/// The window in front, as a number that can cross threads.
pub fn foreground_window() -> isize {
    unsafe { GetForegroundWindow() }.0 as isize
}

/// The monitor a window is mostly on, as a number that can cross threads.
pub fn monitor_of(hwnd: isize) -> isize {
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    unsafe { MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST) }.0 as isize
}

/// Full path of the executable whose window has focus.
pub fn foreground_path() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then(|| path_of_window(hwnd)).flatten()
}

/// What to call an app: the game's name, the exe's product name, or the exe name without `.exe`.
pub fn app_name(path: &str) -> String {
    let exe = file_name(path);
    if let Some(game) = crate::games::find(&exe) {
        return game.name.to_string();
    }
    file_description(path).unwrap_or_else(|| exe.strip_suffix(".exe").or(exe.strip_suffix(".EXE")).unwrap_or(&exe).to_string())
}

/// An app the user would recognise in the taskbar: visible, titled, not cloaked, not a tool window.
fn is_app_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
            return false;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut cloaked = 0u32;
        let ok = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), 4).is_ok();
        !(ok && cloaked != 0)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct RunningApp {
    pub exe: String,
    /// Product name from the exe's version info, or the window title.
    pub name: String,
    pub title: String,
    pub path: String,
    /// `data:image/png;base64,…`
    pub icon: Option<String>,
}

/// Apps with a taskbar-style window, one entry per executable, sorted by name.
pub fn running_apps() -> Vec<RunningApp> {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lp.0 as *mut Vec<(HWND, String)>) };
        if is_app_window(hwnd) {
            let len = unsafe { GetWindowTextLengthW(hwnd) };
            let mut buf = vec![0u16; len as usize + 1];
            let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
            out.push((hwnd, String::from_utf16_lossy(&buf[..n as usize])));
        }
        BOOL(1)
    }
    let mut windows: Vec<(HWND, String)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut windows as *mut _ as isize));
    }
    let own = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let mut map = BTreeMap::<String, RunningApp>::new();
    for (hwnd, title) in windows {
        let Some(path) = path_of_window(hwnd) else { continue };
        let exe = file_name(&path);
        let key = exe.to_lowercase();
        if key == FRAME_HOST || own.as_deref() == Some(&path.to_lowercase()) || map.contains_key(&key) {
            continue;
        }
        let name = file_description(&path).unwrap_or_else(|| title.clone());
        map.insert(key, RunningApp { icon: app_icon(&path), exe, name, title, path });
    }
    let mut apps: Vec<RunningApp> = map.into_values().collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// An exe picked from disk, described like a running app: its name, path and icon.
pub fn app_info(path: &str) -> Option<RunningApp> {
    let p = std::path::Path::new(path);
    let is_exe = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe || !p.is_file() {
        return None;
    }
    Some(RunningApp { exe: file_name(path), name: app_name(path), title: String::new(), path: path.to_string(), icon: app_icon(path) })
}

/// `FileDescription` from the exe's version resource ("Brave Browser" for brave.exe).
fn file_description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let q: Vec<u16> = "\\VarFileInfo\\Translation".encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(q.as_ptr()), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", pair[0], pair[1]);
        let q: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(q.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let s = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

/// The exe's 32 px shell icon as a PNG data URL, cached by path.
pub fn app_icon(path: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().get(path) {
        return v.clone();
    }
    let v = icon_rgba(path).and_then(|(w, h, rgba)| encode_png(w, h, &rgba)).map(|png| {
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
    });
    cache.lock().insert(path.to_string(), v.clone());
    v
}

fn icon_rgba(path: &str) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let mut info = SHFILEINFOW::default();
        let got = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if got == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let mut ii = ICONINFO::default();
        let result = (|| {
            GetIconInfo(info.hIcon, &mut ii).ok()?;
            let mut bm = BITMAP::default();
            if GetObjectW(ii.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut bm as *mut BITMAP).cast()))
                == 0
            {
                return None;
            }
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(buf.as_mut_ptr().cast()), &mut bi, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            if lines == 0 {
                return None;
            }
            let has_alpha = buf.chunks(4).any(|p| p[3] != 0);
            for p in buf.chunks_mut(4) {
                p.swap(0, 2); // BGRA → RGBA
                if !has_alpha {
                    p[3] = 255;
                }
            }
            Some((w as u32, h as u32, buf))
        })();
        if !ii.hbmColor.is_invalid() {
            let _ = DeleteObject(ii.hbmColor.into());
        }
        if !ii.hbmMask.is_invalid() {
            let _ = DeleteObject(ii.hbmMask.into());
        }
        let _ = DestroyIcon(info.hIcon);
        result
    }
}

pub fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The vendor app fights over the same channel (it disables status reports when minimised).
pub fn vendor_app_running() -> bool {
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut found = false;
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let end = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
                if String::from_utf16_lossy(&e.szExeFile[..end]).eq_ignore_ascii_case(VENDOR_EXE) {
                    found = true;
                    break;
                }
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
        found
    }
}

/// Show a window without taking focus away from the game or app in front.
pub fn show_no_activate(hwnd: isize) {
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_SHOWNOACTIVATE);
    }
}

/// Hide a window shown with `show_no_activate`. The window library never saw that show, so its
/// own hide thinks the window is already hidden and does nothing.
pub fn hide(hwnd: isize) {
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    unsafe {
        let _ = ShowWindow(HWND(hwnd as *mut _), SW_HIDE);
    }
}

/// The window is on screen: shown, and not minimised.
pub fn window_up(hwnd: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsIconic;
    let h = HWND(hwnd as *mut _);
    unsafe { IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() }
}

/// Tray icon edge in pixels for the system DPI (16 at 100 %, 20 at 125 %, 24 at 150 %, 32 at 200 %).
pub fn small_icon_size() -> i32 {
    use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
    use windows::Win32::UI::WindowsAndMessaging::SM_CXSMICON;
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    if size > 0 { size } else { 16 }
}

/// Windows' own pointer settings. Games that read raw input ignore them; the desktop and some
/// games don't.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PointerSettings {
    /// "Enhance pointer precision", Windows' pointer acceleration.
    pub acceleration: bool,
    /// Pointer speed, 1 to 20. 10 is the middle of the Settings slider (6 of 11), where one count
    /// moves the pointer one pixel.
    pub speed: u32,
    /// Raw Accel's driver sits in the mouse stack. It changes the counts every app sees, raw input
    /// included, so DPI tests and the pad map measure its output, not the sensor's.
    pub raw_accel: bool,
}

pub fn pointer_settings() -> PointerSettings {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETMOUSE, SPI_GETMOUSESPEED, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut mouse = [0i32; 3];
    let mut speed = 10u32;
    unsafe {
        let none = SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0);
        let _ = SystemParametersInfoW(SPI_GETMOUSE, 0, Some(mouse.as_mut_ptr().cast()), none);
        let _ = SystemParametersInfoW(SPI_GETMOUSESPEED, 0, Some((&mut speed as *mut u32).cast()), none);
    }
    PointerSettings { acceleration: mouse[2] != 0, speed, raw_accel: raw_accel_installed() }
}

/// Turn off "Enhance pointer precision" and/or put the pointer speed back to the middle, for
/// good (saved, and every app told).
pub fn set_pointer(acceleration_off: bool, neutral_speed: bool) -> windows::core::Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETMOUSE, SPI_SETMOUSESPEED,
    };
    let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;
    unsafe {
        if acceleration_off {
            // Thresholds and acceleration all zero: what the Settings checkbox writes when cleared.
            let mut mouse = [0i32; 3];
            SystemParametersInfoW(SPI_SETMOUSE, 0, Some(mouse.as_mut_ptr().cast()), flags)?;
        }
        if neutral_speed {
            // This one takes the value itself in place of a pointer.
            SystemParametersInfoW(SPI_SETMOUSESPEED, 0, Some(10usize as *mut std::ffi::c_void), flags)?;
        }
    }
    Ok(())
}

/// Raw Accel is listed as an upper filter of the mouse device class.
fn raw_accel_installed() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_MULTI_SZ};
    let key = w!(r"SYSTEM\CurrentControlSet\Control\Class\{4d36e96f-e325-11ce-bfc1-08002be10318}");
    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key, w!("UpperFilters"), RRF_RT_REG_MULTI_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size))
    };
    r.is_ok()
        && String::from_utf16_lossy(&buf[..(size as usize / 2).min(buf.len())])
            .split('\0')
            .any(|f| f.eq_ignore_ascii_case("rawaccel"))
}

/// Steam's folder, from the registry (per-user first, then the machine-wide install key).
pub fn steam_path() -> Option<std::path::PathBuf> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let read = |root: HKEY, key: PCWSTR, value: PCWSTR| -> Option<String> {
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        unsafe { RegGetValueW(root, key, value, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) }.ok().ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    };
    read(HKEY_CURRENT_USER, w!(r"Software\Valve\Steam"), w!("SteamPath"))
        .or_else(|| read(HKEY_LOCAL_MACHINE, w!(r"SOFTWARE\WOW6432Node\Valve\Steam"), w!("InstallPath")))
        .map(|p| std::path::PathBuf::from(p.replace('/', "\\")))
        .filter(|p| p.is_dir())
}

/// Whether the taskbar uses the light theme (Settings > Personalisation > Colours > Windows mode).
pub fn taskbar_is_light() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    r.is_ok() && value != 0
}

/// Coverage mask (one byte per pixel, `size` x `size`) of a Segoe Fluent Icons glyph.
pub fn glyph_alpha(ch: char, size: i32) -> Option<Vec<u8>> {
    use windows::core::w;
    use windows::Win32::Foundation::{COLORREF, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateDIBSection, CreateFontW, DrawTextW, GdiFlush, SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY,
        CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE,
        FW_NORMAL, OUT_DEFAULT_PRECIS, TRANSPARENT,
    };
    unsafe {
        let dc = CreateCompatibleDC(None);
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
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
        let old_bmp = SelectObject(dc, bmp.into());
        let font = CreateFontW(
            -size,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            FF_DONTCARE.0 as u32,
            w!("Segoe Fluent Icons"),
        );
        let old_font = SelectObject(dc, font.into());
        SetTextColor(dc, COLORREF(0x00FF_FFFF));
        SetBkMode(dc, TRANSPARENT);
        let mut text: Vec<u16> = ch.encode_utf16(&mut [0u16; 2]).to_vec();
        let mut rect = RECT { left: 0, top: 0, right: size, bottom: size };
        DrawTextW(dc, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        let _ = GdiFlush();
        let px = std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize);
        // White on black with grayscale AA: any channel is the coverage.
        let alpha = px.chunks(4).map(|p| p[0].max(p[1]).max(p[2])).collect::<Vec<u8>>();
        SelectObject(dc, old_font);
        SelectObject(dc, old_bmp);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        alpha.iter().any(|a| *a > 0).then_some(alpha)
    }
}

/// WebView2's legacy input window (`Chrome_RenderWidgetHostHWND`, kept for accessibility) is created
/// with WS_HSCROLL | WS_VSCROLL. When both bits are set Windows paints a native scrollbar size box
/// in its bottom-right corner, which shows through this app's translucent Mica page as a grey
/// square. Chromium re-adds the bits on its own, so this is called periodically and on resize:
/// it strips them and repaints the frame. Only very old trackpad drivers rely on WM_VSCROLL.
pub fn strip_webview_scroll_styles(top: isize) {
    use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, WS_HSCROLL, WS_VSCROLL,
    };
    unsafe extern "system" fn each(hwnd: HWND, _: LPARAM) -> BOOL {
        unsafe {
            let mut buf = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut buf);
            if String::from_utf16_lossy(&buf[..n as usize]) == "Chrome_RenderWidgetHostHWND" {
                let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
                let bits = (WS_HSCROLL.0 | WS_VSCROLL.0) as isize;
                if style & bits != 0 {
                    SetWindowLongPtrW(hwnd, GWL_STYLE, style & !bits);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        0,
                        0,
                        0,
                        0,
                        SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    );
                    let _ = RedrawWindow(Some(hwnd), None, None, RDW_FRAME | RDW_INVALIDATE | RDW_ERASE | RDW_UPDATENOW);
                }
            }
            BOOL(1)
        }
    }
    unsafe {
        let _ = EnumChildWindows(Some(HWND(top as *mut _)), Some(each), LPARAM(0));
    }
}

#[cfg(test)]
mod tests {
    /// What this PC's pointer settings are. `cargo test --lib win -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn reads_this_pcs_pointer_settings() {
        eprintln!("{:?}", super::pointer_settings());
    }
}
