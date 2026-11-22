//! The install steps. Everything is per-user (no admin): files go to
//! %LOCALAPPDATA%\Programs\PeriCore and settings to HKCU.

use crate::layout::{self, Manifest, EXE_NAME, MANIFEST, RUN_KEY, RUN_VALUE, UNINSTALL_KEY};
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// PeriCore.exe, embedded at build time.
const PAYLOAD: &[u8] = include_bytes!(env!("PERICORE_EXE"));
pub const VERSION: &str = env!("PERICORE_VERSION");
const REPO: &str = "https://github.com/hyperdargo/PeriCore";

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub desktop_shortcut: bool,
    pub autostart: bool,
}

/// Version already installed, if any.
pub fn installed_version() -> Option<String> {
    layout::read_manifest(&layout::install_dir()?).map(|m| m.version)
}

/// The user's Desktop folder (follows OneDrive redirection).
fn desktop_dir() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};
    let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: standard known-folder query; the returned buffer is freed below.
    unsafe {
        if SHGetKnownFolderPath(&FOLDERID_Desktop, 0, std::ptr::null_mut(), &mut p) != 0 || p.is_null() {
            return None;
        }
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        CoTaskMemFree(p.cast());
        Some(PathBuf::from(s))
    }
}

fn shortcut(target: &Path, link: &Path) -> Result<()> {
    let mut sl = mslnk::ShellLink::new(target).map_err(|e| anyhow!("{e:?}"))?;
    sl.set_name(Some("PeriCore device hub".into()));
    sl.set_working_dir(target.parent().map(|d| d.display().to_string()));
    sl.set_icon_location(Some(target.display().to_string()));
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir)?;
    }
    sl.create_lnk(link).map_err(|e| anyhow!("{e:?}"))
}

/// Writes the exe, retrying briefly while a just-closed PeriCore releases it.
fn write_exe(dir: &Path) -> Result<PathBuf> {
    let exe = dir.join(EXE_NAME);
    let tmp = dir.join("PeriCore.exe.new");
    std::fs::write(&tmp, PAYLOAD).context("writing PeriCore.exe")?;
    let mut last = None;
    for _ in 0..15 {
        match std::fs::rename(&tmp, &exe) {
            Ok(()) => return Ok(exe),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    let _ = std::fs::remove_file(&tmp);
    Err(anyhow!("PeriCore.exe is in use - close PeriCore and try again ({:?})", last))
}

fn reg_value(name: &str, kind: &str, data: &str) {
    layout::reg(&["add", UNINSTALL_KEY, "/v", name, "/t", kind, "/d", data, "/f"]);
}

/// Registers PeriCore in Settings > Apps > Installed apps.
fn register_app(exe: &Path, dir: &Path) {
    let exe_s = exe.display().to_string();
    reg_value("DisplayName", "REG_SZ", "PeriCore");
    reg_value("DisplayVersion", "REG_SZ", VERSION);
    reg_value("Publisher", "REG_SZ", "PeriCore");
    reg_value("DisplayIcon", "REG_SZ", &exe_s);
    reg_value("InstallLocation", "REG_SZ", &dir.display().to_string());
    reg_value("UninstallString", "REG_SZ", &format!("\"{exe_s}\" --uninstall"));
    reg_value("QuietUninstallString", "REG_SZ", &format!("\"{exe_s}\" --uninstall --quiet"));
    reg_value("URLInfoAbout", "REG_SZ", REPO);
    reg_value("NoModify", "REG_DWORD", "1");
    reg_value("NoRepair", "REG_DWORD", "1");
    reg_value("EstimatedSize", "REG_DWORD", &(PAYLOAD.len() / 1024).to_string());
}

/// Installs (or updates) PeriCore; returns the installed exe path.
pub fn install(opts: Options, progress: &dyn Fn(&str)) -> Result<PathBuf> {
    let dir = layout::install_dir().ok_or_else(|| anyhow!("LOCALAPPDATA is not set"))?;
    let previous = layout::read_manifest(&dir).unwrap_or_default();

    progress("Closing PeriCore if it's running…");
    layout::stop_other_instances();
    std::fs::create_dir_all(&dir).context("creating the install folder")?;

    progress("Copying PeriCore…");
    let exe = write_exe(&dir)?;

    progress("Creating shortcuts…");
    let mut shortcuts = Vec::new();
    if let Some(start) = layout::start_menu_link() {
        shortcut(&exe, &start).context("Start menu shortcut")?;
        shortcuts.push(start);
    }
    let desktop = desktop_dir().map(|d| d.join("PeriCore.lnk"));
    match (&desktop, opts.desktop_shortcut) {
        (Some(link), true) => {
            shortcut(&exe, link).context("desktop shortcut")?;
            shortcuts.push(link.clone());
        }
        (Some(link), false) if previous.shortcuts.contains(link) => {
            let _ = std::fs::remove_file(link);
        }
        _ => {}
    }

    progress("Registering with Windows…");
    if opts.autostart {
        layout::reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &format!("\"{}\" --tray", exe.display()), "/f"]);
    } else {
        layout::reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
    }
    register_app(&exe, &dir);
    let manifest = Manifest { version: VERSION.to_string(), shortcuts };
    std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(exe)
}

/// Starts PeriCore (opens the dashboard and the tray).
pub fn launch(exe: &Path) {
    let _ = std::process::Command::new(exe).current_dir(exe.parent().unwrap_or(Path::new("."))).spawn();
}
