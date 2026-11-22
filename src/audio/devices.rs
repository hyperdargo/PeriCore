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

/// Equalizer APO's pre-mix and post-mix effect classes.
const EAPO_CLASSES: [&str; 2] = ["{eacd2258-fcac-4ff4-b36d-419e924a6d79}", "{ec1cc9ce-faed-4822-828a-82a81a6f018f}"];

/// True if Equalizer APO is turned on for this endpoint (ticked in its
/// Device Selector). Windows needs a restart before it actually runs.
pub fn has_eapo(capture: bool, id: &str) -> bool {
    let path = format!(r"{ROOT}\{}\{id}\FxProperties", if capture { "Capture" } else { "Render" });
    let Ok(fx) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ) else {
        return false;
    };
    fx.enum_values().flatten().any(|(_, v)| {
        let text = v.to_string().to_lowercase();
        EAPO_CLASSES.iter().any(|c| text.contains(c))
    })
}

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
