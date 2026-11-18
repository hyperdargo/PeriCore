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
        if owner.is_some() && owner == me {
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
