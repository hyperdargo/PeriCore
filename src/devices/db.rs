//! Embedded catalogue of RY-protocol devices (3000+ Ajazz / Fantech / other
//! brand mice and keyboards), extracted from the vendor driver.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
pub struct DbEntry {
    pub id: i64,
    pub vid: u16,
    pub pid: u16,
    pub usage: u16,
    pub usage_page: u32,
    pub name: String,
    pub display_name: String,
    /// "mouse" | "keyboard" | "dangle" | "audio"
    pub kind: String,
    pub company: String,
}

static RAW: &str = include_str!("../../assets/ry_devices.json");

pub fn all() -> &'static [DbEntry] {
    static DB: OnceLock<Vec<DbEntry>> = OnceLock::new();
    DB.get_or_init(|| serde_json::from_str(RAW).unwrap_or_default())
}

/// Entries matching a HID interface.
pub fn match_interface(vid: u16, pid: u16, usage_page: u16, usage: u16) -> Vec<&'static DbEntry> {
    all()
        .iter()
        .filter(|e| e.vid == vid && e.pid == pid && e.usage_page == usage_page as u32 && e.usage == usage)
        .collect()
}

/// Looks up the exact model a receiver reports via its device id.
pub fn by_id(id: i64) -> Option<&'static DbEntry> {
    all().iter().find(|e| e.id == id)
}

/// Picks a friendly name when several models share one VID/PID.
pub fn best_name(entries: &[&DbEntry]) -> String {
    let prefer = entries.iter().find(|e| e.company.eq_ignore_ascii_case("AJAZZMOUSE"));
    prefer
        .or(entries.first())
        .map(|e| e.display_name.clone())
        .unwrap_or_else(|| "Unknown device".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_loads() {
        assert!(all().len() > 1000);
    }

    #[test]
    fn knows_ajazz_apex_models() {
        assert_eq!(by_id(2349).unwrap().display_name, "AJ159 APEX");
        assert_eq!(by_id(2360).unwrap().display_name, "AJ179 APEX");
    }

    #[test]
    fn matches_wireless_receiver() {
        let m = match_interface(0x3151, 0x5007, 0xFFFF, 2);
        assert!(!m.is_empty());
        assert!(m.iter().all(|e| e.kind == "dangle"));
    }
}
