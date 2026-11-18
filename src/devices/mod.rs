//! Device discovery: maps HID interfaces to a protocol family.

pub mod db;
pub mod fantech;
pub mod ry;

use hidapi::HidApi;
use std::ffi::CString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Mouse,
    Keyboard,
    /// 2.4G USB receiver; the real device sits behind it.
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Rongyuan firmware (Ajazz mice/keyboards and many rebrands).
    Ry,
    /// Fantech MAXFIT61 (MK857, XS61BRGB firmware, VID 0416 PID C345).
    FantechMaxfit61,
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

/// Lists every supported device's configuration interface.
pub fn discover(api: &HidApi) -> Vec<Detected> {
    let mut found: Vec<Detected> = Vec::new();
    for d in api.device_list() {
        let (vid, pid) = (d.vendor_id(), d.product_id());
        let hit = if fantech::matches(d) {
            Some((Family::FantechMaxfit61, Kind::Keyboard, fantech::NAME.to_string()))
        } else {
            ry_match(vid, pid, d.usage_page(), d.usage())
        };
        if let Some((family, kind, name)) = hit {
            let path = d.path().to_owned();
            if !found.iter().any(|f| f.path == path) {
                found.push(Detected { path, vid, pid, family, kind, name });
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
