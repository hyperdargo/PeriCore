//! Lists active audio endpoints from the registry (no COM needed).

use super::config::Device;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const ROOT: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio";
/// PKEY_Device_DeviceDesc ("Headphones", "Microphone", ...).
const DESC: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";
/// PKEY_DeviceInterface_FriendlyName (the adapter, e.g. "Realtek(R) Audio").
const ADAPTER: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";
const DEVICE_STATE_ACTIVE: u32 = 1;

/// Active playback (`capture == false`) or recording endpoints.
pub fn list(capture: bool) -> Vec<Device> {
    let path = format!(r"{ROOT}\{}", if capture { "Capture" } else { "Render" });
    let Ok(root) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ) else {
        return Vec::new();
    };
    let mut out: Vec<Device> = root
        .enum_keys()
        .flatten()
        .filter_map(|guid| {
            let key = root.open_subkey_with_flags(&guid, KEY_READ).ok()?;
            let state: u32 = key.get_value("DeviceState").ok()?;
            if state != DEVICE_STATE_ACTIVE {
                return None;
            }
            let props = key.open_subkey_with_flags("Properties", KEY_READ).ok()?;
            let desc: String = props.get_value(DESC).unwrap_or_default();
            let adapter: String = props.get_value(ADAPTER).unwrap_or_default();
            let name = if adapter.is_empty() { desc } else { format!("{desc} ({adapter})") };
            Some(Device { id: guid.to_lowercase(), name })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}
