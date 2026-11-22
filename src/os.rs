//! Thin Windows helpers: single instance, message pump, autostart, launching.

use anyhow::{bail, Result};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowExW, GetWindowThreadProcessId, IsIconic, MsgWaitForMultipleObjects, PeekMessageW,
    SetForegroundWindow, ShowWindow, TranslateMessage, MSG, PM_REMOVE, QS_ALLINPUT, SW_RESTORE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "PeriCore";
pub const WINDOW_TITLE: &str = "PeriCore";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Named-mutex guard; `acquire` returns None if another instance holds it.
pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn acquire(name: &str) -> Option<Self> {
        let name = wide(&format!("Local\\{name}"));
        // SAFETY: valid null-terminated wide string; null security attributes.
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if h.is_null() {
            return None;
        }
        // SAFETY: plain FFI call reading thread-local error state.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: h is a handle we own.
            unsafe { CloseHandle(h) };
            return None;
        }
        Some(Self(h))
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: handle created by CreateMutexW and owned by self.
        unsafe { CloseHandle(self.0) };
    }
}

/// Waits up to `timeout_ms` for window messages and dispatches them.
pub fn pump_messages(timeout_ms: u32) {
    // SAFETY: standard Win32 message loop on the calling thread.
    unsafe {
        MsgWaitForMultipleObjects(0, std::ptr::null(), 0, timeout_ms, QS_ALLINPUT);
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Full image path of the process that owns `hwnd`.
fn window_exe(hwnd: HWND) -> Option<PathBuf> {
    let mut pid = 0u32;
    // SAFETY: hwnd came from FindWindowExW; pid is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    // SAFETY: limited-query access only; handle closed below.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: buf/len describe a writable buffer of `len` u16s.
    let ok = unsafe { QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) };
    // SAFETY: h is an open process handle we own.
    unsafe { CloseHandle(h) };
    (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

/// Brings our own settings window to the front (ignores other windows that
/// happen to share the title, e.g. an Explorer folder). Returns false if none.
pub fn focus_settings_window() -> bool {
    focus_window(true)
}

/// Brings any PeriCore settings window forward, even one from another copy
/// of the exe (e.g. a second download), when the single-instance lock is held.
pub fn focus_any_settings_window() -> bool {
    focus_window(false)
}

fn focus_window(same_exe: bool) -> bool {
    let title = wide(WINDOW_TITLE);
    let me = std::env::current_exe().ok().and_then(|p| p.canonicalize().ok());
    let mut hwnd: HWND = std::ptr::null_mut();
    loop {
        // SAFETY: iterates top-level windows with the given title.
        hwnd = unsafe { FindWindowExW(std::ptr::null_mut(), hwnd, std::ptr::null(), title.as_ptr()) };
        if hwnd.is_null() {
            return false;
        }
        let owner = window_exe(hwnd).and_then(|p| p.canonicalize().ok());
        let ours = owner.as_ref().is_some_and(|o| o.file_stem().is_some_and(|n| n.to_string_lossy().starts_with("PeriCore")));
        if (owner.is_some() && owner == me) || (!same_exe && ours) {
            // SAFETY: hwnd is a live top-level window owned by our exe.
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                SetForegroundWindow(hwnd);
            }
            return true;
        }
    }
}

pub fn open_settings_window() {
    if focus_settings_window() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--ui").spawn();
    }
}

/// Starts the tray process if it isn't already running.
pub fn ensure_tray_running() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe).arg("--tray").spawn();
    }
}

fn reg(args: &[&str]) -> Result<bool> {
    let out = Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()?;
    Ok(out.status.success())
}

pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE]).unwrap_or(false)
}

pub fn set_autostart(on: bool) -> Result<()> {
    let ok = if on {
        let exe = std::env::current_exe()?;
        let cmd = format!("\"{}\" --tray", exe.display());
        reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])?
    } else {
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])?
    };
    if !ok {
        bail!("reg.exe failed");
    }
    Ok(())
}

/// Held by the settings window while it uploads to the dock screen, so the
/// tray's battery poll doesn't interleave with the transfer.
pub const SCREEN_BUSY: &str = "PeriCore.ScreenBusy";

pub fn screen_busy() -> bool {
    SingleInstance::acquire(SCREEN_BUSY).is_none()
}

/// Local wall-clock time: (year, month, day, hour, minute, second).
pub fn local_time() -> (u16, u8, u8, u8, u8, u8) {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: GetLocalTime fills the struct we pass.
    let t = unsafe {
        let mut t = std::mem::zeroed();
        GetLocalTime(&mut t);
        t
    };
    (t.wYear, t.wMonth as u8, t.wDay as u8, t.wHour as u8, t.wMinute as u8, t.wSecond as u8)
}

/// Native "Open" dialog for images; None if cancelled.
pub fn pick_image_file() -> Option<PathBuf> {
    pick_file("Images (*.gif;*.png;*.jpg;*.jpeg)\0*.gif;*.png;*.jpg;*.jpeg\0All files\0*.*\0")
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(CREATE_NO_WINDOW).spawn();
}

/// Native "Open" dialog; `filter` is a Win32 filter string ("Name\0*.ext\0...").
pub fn pick_file(filter: &str) -> Option<PathBuf> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let filter = wide(filter);
    let mut buf = vec![0u16; 1024];
    // SAFETY: zeroed is a valid OPENFILENAMEW; required fields are set below.
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = buf.len() as u32;
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    // SAFETY: ofn points to valid buffers that outlive the call.
    if unsafe { GetOpenFileNameW(&mut ofn) } == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

/// Vendor helpers that poll the devices and break uploads if left running.
const VENDOR_PROCS: [&str; 2] = ["iot_driver", "AJAZZ Driver"];

fn tasklist() -> String {
    Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// True if the AJAZZ app or its driver helper is running.
pub fn vendor_app_running() -> bool {
    let list = tasklist();
    VENDOR_PROCS.iter().any(|p| list.contains(p))
}

/// Closes the AJAZZ app and its helper (the user's explicit choice).
pub fn close_vendor_apps() {
    for line in tasklist().lines() {
        let name = line.split(',').next().unwrap_or("").trim_matches('"');
        if VENDOR_PROCS.iter().any(|p| name.starts_with(p)) {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", name])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    }
}

/// Appends a line to %APPDATA%\PeriCore\pericore.log (kept under 256 KB).
pub fn log_error(msg: &str) {
    use std::io::Write;
    let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("PeriCore")) else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("pericore.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] v{} {msg}", env!("CARGO_PKG_VERSION"));
    }
}

/// Logs panics (the release build aborts on panic, so this is the only trace).
pub fn install_panic_log() {
    std::panic::set_hook(Box::new(|info| log_error(&format!("panic: {info}"))));
}
