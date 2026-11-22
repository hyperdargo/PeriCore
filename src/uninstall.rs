//! `PeriCore.exe --uninstall`: what "Uninstall" in Windows' Installed apps
//! runs. Removes shortcuts, auto-start and the Apps entry, then deletes the
//! install folder once this process has exited. Settings in %APPDATA% stay.

use crate::install_layout::{self as layout, RUN_KEY, RUN_VALUE, UNINSTALL_KEY};
use anyhow::Result;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_YESNO};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn message(text: &str, flags: u32) -> i32 {
    let (t, c) = (wide(text), wide("PeriCore"));
    // SAFETY: valid NUL-terminated strings; no owner window.
    unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags) }
}

pub fn run(quiet: bool) -> Result<()> {
    if !quiet && message("Remove PeriCore from this PC?\n\nYour settings and battery history are kept.", MB_YESNO | MB_ICONQUESTION) != IDYES {
        return Ok(());
    }
    layout::stop_other_instances();

    let exe = std::env::current_exe()?;
    let dir = exe.parent().map(std::path::Path::to_path_buf);
    let installed_dir = layout::install_dir();
    // Only ever delete the folder the installer created, never a portable copy's folder.
    let is_installed = matches!((&dir, &installed_dir), (Some(d), Some(i)) if layout::same_path(d, i));

    let mut links: Vec<std::path::PathBuf> = layout::start_menu_link().into_iter().collect();
    if let Some(m) = dir.as_deref().and_then(layout::read_manifest) {
        links.extend(m.shortcuts);
    }
    for l in links {
        let _ = std::fs::remove_file(l);
    }
    layout::reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    layout::reg(&["delete", UNINSTALL_KEY, "/f"]);

    if let (true, Some(d)) = (is_installed, dir) {
        schedule_delete(&d);
    }
    if !quiet {
        message("PeriCore was removed.", MB_ICONINFORMATION);
    }
    Ok(())
}

/// A running exe can't delete itself: a hidden cmd waits for us to exit, then removes the folder.
fn schedule_delete(dir: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let script = format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display());
    let _ = std::process::Command::new("cmd")
        .arg("/c")
        .raw_arg(&script) // verbatim: cmd needs the quotes as written
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .current_dir(std::env::temp_dir())
        .spawn();
}
