//! Device discovery: maps HID interfaces to a protocol family.

pub mod db;
pub mod fantech;
pub mod hidlist;
pub mod playstation;
pub mod ry;

use hidapi::HidApi;
use std::ffi::CString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mouse,
    Keyboard,
    /// 2.4G USB receiver; the real device sits behind it.
    Receiver,
    /// Game controller (USB or Bluetooth).
    Controller,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Rongyuan firmware (Ajazz mice/keyboards and many rebrands).
    Ry,
    /// Fantech MAXFIT61 (MK857, XS61BRGB firmware, VID 0416 PID C345).
    FantechMaxfit61,
    /// Sony DualShock 4 / DualSense (battery only).
    PlayStation,
}

#[derive(Debug, Clone)]
pub struct Detected {
    pub path: CString,
    pub vid: u16,
    pub pid: u16,
    pub family: Family,
    pub kind: Kind,
    pub name: String,
}

/// A hidapi handle for opening devices by path. Its own device listing is
/// turned off: `discover` lists devices instead (see `hidlist`).
pub fn hid_api() -> hidapi::HidResult<HidApi> {
    HidApi::disable_device_discovery();
    HidApi::new()
}

/// Vendor ids PeriCore supports (catalogue, Sony, Fantech).
fn supported_vids() -> &'static [u16] {
    static VIDS: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
    VIDS.get_or_init(|| {
        let mut v: Vec<u16> = db::all().iter().map(|e| e.vid).collect();
        v.extend([playstation::SONY_VID, fantech::VID]);
        v.sort_unstable();
        v.dedup();
        v
    })
}

/// Lists every supported device's configuration interface. Uses our own
/// listing (see `hidlist`), so slow unrelated devices can't stall it.
pub fn discover() -> Vec<Detected> {
    let mut found: Vec<Detected> = Vec::new();
    for d in hidlist::list(supported_vids()) {
        let (vid, pid) = (d.vid, d.pid);
        let hit = if playstation::matches(vid, pid, d.usage_page, d.usage) {
            playstation::lookup(vid, pid).map(|(_, name)| (Family::PlayStation, Kind::Controller, name.to_string()))
        } else if fantech::matches(&d) {
            Some((Family::FantechMaxfit61, Kind::Keyboard, fantech::NAME.to_string()))
        } else {
            ry_match(vid, pid, d.usage_page, d.usage)
        };
        if let Some((family, kind, name)) = hit {
            if !found.iter().any(|f| f.path == d.path) {
                found.push(Detected { path: d.path, vid, pid, family, kind, name });
            }
        }
    }
    found
}

fn ry_match(vid: u16, pid: u16, page: u16, usage: u16) -> Option<(Family, Kind, String)> {
    let entries = db::match_interface(vid, pid, page, usage);
    let first = entries.first()?;
    let kind = match first.kind.as_str() {
        "mouse" => Kind::Mouse,
        "dangle" => Kind::Receiver,
        "keyboard" | "keyboard3123" => Kind::Keyboard,
        _ => return None,
    };
    Some((Family::Ry, kind, db::best_name(&entries)))
}
