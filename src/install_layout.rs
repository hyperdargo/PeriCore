//! Where PeriCore-Setup puts things. Shared (via `#[path]`) by the installer
//! in tools/installer and by `PeriCore.exe --uninstall`, so both agree.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

#[allow(dead_code)] // used by the installer
pub const EXE_NAME: &str = "PeriCore.exe";
pub const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\PeriCore";
pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_VALUE: &str = "PeriCore";
/// Written next to the exe: what the installer created, for a clean uninstall.
pub const MANIFEST: &str = "install.json";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Shortcut files created (Start menu, desktop).
    pub shortcuts: Vec<PathBuf>,
}

/// %LOCALAPPDATA%\Programs\PeriCore (per-user, no admin rights needed).
pub fn install_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs").join("PeriCore"))
}

/// Start menu shortcut path.
pub fn start_menu_link() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Start Menu\Programs\PeriCore.lnk"))
}

pub fn read_manifest(dir: &std::path::Path) -> Option<Manifest> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(MANIFEST)).ok()?).ok()
}

/// Runs reg.exe without a console window; true on success.
pub fn reg(args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output().is_ok_and(|o| o.status.success())
}

/// Ends every PeriCore.exe except this process.
pub fn stop_other_instances() {
    use std::os::windows::process::CommandExt;
    let me = std::process::id().to_string();
    let _ = Command::new("taskkill")
        .args(["/F", "/FI", "IMAGENAME eq PeriCore.exe", "/FI", &format!("PID ne {me}")])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Case-insensitive path equality (Windows paths).
#[allow(dead_code)] // used by --uninstall
pub fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}
