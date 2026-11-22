//! Keeps PeriCore's tray icons on the taskbar instead of in the "^" overflow.
//!
//! Windows 11 records every tray icon it has seen under
//! HKCU\Control Panel\NotifyIconSettings\<id>, and new icons start hidden.
//! Explorer shows an icon on the taskbar when that entry's IsPromoted value
//! is 1, the same value its own "show on taskbar" switch writes. This only
//! touches entries whose ExecutablePath is this PeriCore.exe, and each entry
//! only once, so hiding the icon again in Settings sticks.

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const SETTINGS: &str = r"Control Panel\NotifyIconSettings";
/// Store file listing the entries already promoted.
const DONE: &str = "tray-promoted";

/// Promotes this exe's new tray entries; returns how many were changed.
pub fn promote() -> usize {
    let Ok(exe) = std::env::current_exe() else { return 0 };
    let exe = exe.to_string_lossy().to_lowercase();
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(SETTINGS, KEY_READ) else {
        return 0;
    };
    let mut done: Vec<String> = crate::store::load(DONE).unwrap_or_default();
    let mut changed = 0;
    for name in root.enum_keys().flatten() {
        if done.contains(&name) {
            continue;
        }
        let Ok(key) = root.open_subkey_with_flags(&name, KEY_READ | KEY_SET_VALUE) else { continue };
        let path: String = key.get_value("ExecutablePath").unwrap_or_default();
        if path.to_lowercase() == exe && key.set_value("IsPromoted", &1u32).is_ok() {
            done.push(name);
            changed += 1;
        }
    }
    if changed > 0 {
        let _ = crate::store::save(DONE, &done);
    }
    changed
}
