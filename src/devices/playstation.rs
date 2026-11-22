//! Sony PlayStation controllers (DualShock 4, DualSense, DualSense Edge):
//! battery from the controller's own input report, over USB or Bluetooth.
//!
//! Layouts (publicly documented, e.g. by the Linux hid-playstation driver):
//! - DualSense USB report 0x01 (64 B): status at byte 53. Bluetooth 0x31: byte 54.
//!   Low nibble = level 0..10, high nibble = 0 discharging / 1 charging / 2 full.
//! - DualShock 4 USB report 0x01 (64 B): status at byte 30. Bluetooth 0x11: byte 32.
//!   Low nibble = level, bit 4 = cable plugged in (level 11 = full on cable).
//!
//! Over Bluetooth both pads start in a short "basic" report without battery;
//! reading their calibration feature report switches them to the full report.

use crate::devices::ry::protocol::{BatteryInfo, ChargeState};
use anyhow::{anyhow, Result};
use hidapi::{HidApi, HidDevice};
use std::ffi::CStr;

pub const SONY_VID: u16 = 0x054C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    DualShock4,
    DualSense,
}

/// (pid, model, name) for supported pads.
const PADS: [(u16, Model, &str); 5] = [
    (0x05C4, Model::DualShock4, "DualShock 4"),
    (0x09CC, Model::DualShock4, "DualShock 4"),
    (0x0BA0, Model::DualShock4, "DualShock 4 (wireless adapter)"),
    (0x0CE6, Model::DualSense, "DualSense"),
    (0x0DF2, Model::DualSense, "DualSense Edge"),
];

/// Generic Desktop page, Game Pad usage: the interface that carries input reports.
const PAGE_GENERIC: u16 = 0x01;
const USAGE_GAMEPAD: u16 = 0x05;

pub fn lookup(vid: u16, pid: u16) -> Option<(Model, &'static str)> {
    (vid == SONY_VID).then(|| PADS.iter().find(|p| p.0 == pid).map(|p| (p.1, p.2)))?
}

/// True for the controller's gamepad interface.
pub fn matches(vid: u16, pid: u16, page: u16, usage: u16) -> bool {
    lookup(vid, pid).is_some() && page == PAGE_GENERIC && usage == USAGE_GAMEPAD
}

fn percent(level: u8) -> u8 {
    (level.min(10) * 10 + 5).min(100)
}

/// Parses one input report (report ID at byte 0). None = no battery in it.
pub fn parse(model: Model, r: &[u8]) -> Option<BatteryInfo> {
    match model {
        Model::DualSense => {
            let at = match (r.first()?, r.len()) {
                (0x01, n) if n >= 64 => 53,
                (0x31, n) if n >= 78 => 54,
                _ => return None,
            };
            let (level, status) = (r[at] & 0x0F, r[at] >> 4);
            match status {
                0x0 => Some(BatteryInfo { percent: percent(level), state: ChargeState::Discharging }),
                0x1 => Some(BatteryInfo { percent: percent(level), state: ChargeState::Charging }),
                0x2 => Some(BatteryInfo { percent: 100, state: ChargeState::Full }),
                _ => None, // voltage / temperature / charging error
            }
        }
        Model::DualShock4 => {
            let at = match (r.first()?, r.len()) {
                (0x01, n) if n >= 64 => 30,
                (0x11, n) if n >= 78 => 32,
                _ => return None,
            };
            let (level, cable) = (r[at] & 0x0F, r[at] & 0x10 != 0);
            match (cable, level) {
                (true, 0..=9) => Some(BatteryInfo { percent: percent(level), state: ChargeState::Charging }),
                (true, 10 | 11) => Some(BatteryInfo { percent: 100, state: ChargeState::Full }),
                (true, _) => None,
                (false, l) => Some(BatteryInfo { percent: percent(l), state: ChargeState::Discharging }),
            }
        }
    }
}

/// Asks a Bluetooth pad for its calibration data, which makes it send full reports.
fn enable_full_reports(dev: &HidDevice, model: Model) {
    let mut buf = [0u8; 64];
    buf[0] = match model {
        Model::DualSense => 0x05,
        Model::DualShock4 => 0x02,
    };
    let _ = dev.get_feature_report(&mut buf);
}

/// An open controller.
pub struct Pad {
    dev: HidDevice,
    pub model: Model,
    switched: std::cell::Cell<bool>,
}

impl Pad {
    pub fn open(api: &HidApi, path: &CStr, model: Model) -> Result<Self> {
        Ok(Self { dev: api.open_path(path)?, model, switched: std::cell::Cell::new(false) })
    }

    /// Waits up to ~1 s for an input report that carries the battery.
    pub fn battery(&self) -> Result<BatteryInfo> {
        let mut buf = [0u8; 128];
        for _ in 0..20 {
            let n = self.dev.read_timeout(&mut buf, 50)?;
            if n == 0 {
                continue;
            }
            if let Some(b) = parse(self.model, &buf[..n]) {
                return Ok(b);
            }
            // A short report means Bluetooth basic mode: switch once, then keep reading.
            if !self.switched.replace(true) {
                enable_full_reports(&self.dev, self.model);
            }
        }
        Err(anyhow!("controller sent no battery report (is it switched on?)"))
    }
}

/// Reads the battery of the controller at `path`.
pub fn read_battery(api: &HidApi, path: &CStr, model: Model) -> Result<BatteryInfo> {
    Pad::open(api, path, model)?.battery()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u8, len: usize, at: usize, status: u8) -> Vec<u8> {
        let mut r = vec![0u8; len];
        r[0] = id;
        r[at] = status;
        r
    }

    #[test]
    fn dualsense_usb_and_bt() {
        let b = parse(Model::DualSense, &report(0x01, 64, 53, 0x07)).unwrap();
        assert_eq!(b, BatteryInfo { percent: 75, state: ChargeState::Discharging });
        let b = parse(Model::DualSense, &report(0x31, 78, 54, 0x13)).unwrap();
        assert_eq!(b, BatteryInfo { percent: 35, state: ChargeState::Charging });
        let b = parse(Model::DualSense, &report(0x01, 64, 53, 0x2A)).unwrap();
        assert_eq!(b, BatteryInfo { percent: 100, state: ChargeState::Full });
        assert_eq!(parse(Model::DualSense, &report(0x01, 64, 53, 0xF0)), None);
        // Bluetooth basic report: too short, no battery.
        assert_eq!(parse(Model::DualSense, &report(0x01, 10, 5, 0)), None);
    }

    #[test]
    fn dualshock4_usb_and_bt() {
        let b = parse(Model::DualShock4, &report(0x01, 64, 30, 0x08)).unwrap();
        assert_eq!(b, BatteryInfo { percent: 85, state: ChargeState::Discharging });
        let b = parse(Model::DualShock4, &report(0x11, 78, 32, 0x14)).unwrap();
        assert_eq!(b, BatteryInfo { percent: 45, state: ChargeState::Charging });
        let b = parse(Model::DualShock4, &report(0x01, 64, 30, 0x1B)).unwrap();
        assert_eq!(b.state, ChargeState::Full);
        assert_eq!(parse(Model::DualShock4, &report(0x01, 64, 30, 0x0A)).unwrap().percent, 100);
    }

    #[test]
    fn matching() {
        assert!(matches(0x054C, 0x0CE6, 0x01, 0x05));
        assert!(!matches(0x054C, 0x0CE6, 0x01, 0x02));
        assert!(!matches(0x054D, 0x0CE6, 0x01, 0x05));
        assert_eq!(lookup(0x054C, 0x0DF2).unwrap().1, "DualSense Edge");
    }
}
