//! Background battery polling for wireless devices.

use super::estimate;
use crate::battery_log::{self, Log};
use crate::devices::{self, playstation, ry::protocol::ChargeState, ry::RyDevice, Family, Kind};
use hidapi::HidApi;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// Stable identity (HID path) so each device keeps its own tray icon.
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub percent: Option<u8>,
    pub state: ChargeState,
    /// The device is asleep; `percent` is its last reported level.
    pub asleep: bool,
    /// Estimated minutes to empty (or to full while charging).
    pub minutes_left: Option<u32>,
}

impl Reading {
    pub fn charging(&self) -> bool {
        self.state == ChargeState::Charging
    }
    pub fn full(&self) -> bool {
        self.state == ChargeState::Full
    }
    /// History key: survives the device moving to another USB port.
    pub fn log_key(&self) -> String {
        format!("{}:{}", kind_name(self.kind), self.name)
    }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Controller => "controller",
        Kind::Keyboard => "keyboard",
        Kind::Mouse | Kind::Receiver => "mouse",
    }
}

pub const POLL_EVERY: Duration = Duration::from_secs(20);

/// Reads battery for every wireless-capable device currently plugged in.
pub fn snapshot(api: &mut HidApi) -> Vec<Reading> {
    let _ = api.refresh_devices();
    devices::discover(api)
        .into_iter()
        .filter_map(|d| {
            let (name, battery, asleep) = match d.family {
                Family::Ry if matches!(d.kind, Kind::Receiver | Kind::Mouse) => {
                    let dev = RyDevice::open(api, &d).ok()?;
                    let name = dev.identify().unwrap_or_else(|| d.name.clone());
                    match dev.battery_status() {
                        Ok((b, asleep)) => (name, Some(b), asleep),
                        Err(_) => (name, None, false),
                    }
                }
                Family::PlayStation => {
                    let (model, _) = playstation::lookup(d.vid, d.pid)?;
                    (d.name.clone(), playstation::read_battery(api, &d.path, model).ok(), false)
                }
                _ => return None,
            };
            Some(Reading {
                key: d.path.to_string_lossy().into_owned(),
                name,
                kind: d.kind,
                percent: battery.map(|b| b.percent),
                state: battery.map_or(ChargeState::Discharging, |b| b.state),
                asleep,
                minutes_left: None,
            })
        })
        .collect()
}

/// Logs each reading and adds time-left estimates from the saved history.
pub fn estimate(readings: &mut [Reading], log: &mut Log, t: u64) {
    for r in readings.iter_mut() {
        let key = r.log_key();
        if let Some(p) = r.percent {
            log.record(&key, battery_log::Entry { name: &r.name, kind: kind_name(r.kind), percent: p, state: r.state, asleep: r.asleep }, t);
        }
        r.minutes_left = if r.full() || r.asleep { None } else { log.minutes_left(&key) };
    }
}

/// Polls forever; a message on `wake` triggers an immediate refresh.
pub fn run(tx: Sender<Vec<Reading>>, wake: Receiver<()>) {
    let Ok(mut api) = HidApi::new() else { return };
    let mut log = Log::load();
    loop {
        // Skip while the settings window is uploading to the dock screen.
        if !crate::os::screen_busy() {
            let mut readings = snapshot(&mut api);
            estimate(&mut readings, &mut log, battery_log::now());
            log.save();
            if tx.send(readings).is_err() {
                return;
            }
        }
        match wake.recv_timeout(POLL_EVERY) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// One tooltip line, laptop style: "DualSense: 62% · about 3 h 10 min left".
pub fn line(r: &Reading) -> String {
    let Some(p) = r.percent else {
        return format!("{}: battery unavailable", r.name);
    };
    let detail = if r.full() {
        " · fully charged".to_string()
    } else if r.asleep {
        " · asleep (last known)".to_string()
    } else {
        match (r.charging(), r.minutes_left) {
            (true, Some(m)) => format!(" · charging, {} to full", estimate::format_minutes(m)),
            (true, None) => " · charging".to_string(),
            (false, Some(m)) => format!(" · about {} left", estimate::format_minutes(m)),
            (false, None) => String::new(),
        }
    };
    format!("{}: {p}%{detail}", r.name)
}

/// Tooltip text (Windows caps tray tooltips at 127 chars).
pub fn tooltip(readings: &[Reading]) -> String {
    if readings.is_empty() {
        return "PeriCore - no wireless device".into();
    }
    let lines: Vec<String> = readings.iter().map(line).collect();
    // Truncate on a char boundary: device names may contain non-ASCII text.
    lines.join("\n").chars().take(127).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str, percent: Option<u8>, state: ChargeState) -> Reading {
        Reading { key: name.into(), name: name.into(), kind: Kind::Receiver, percent, state, asleep: false, minutes_left: None }
    }

    #[test]
    fn tooltip_formats() {
        let mut a = r("AJ159 APEX", Some(80), ChargeState::Discharging);
        a.minutes_left = Some(200);
        let readings = vec![
            a,
            r("Pad", Some(5), ChargeState::Charging),
            r("X", None, ChargeState::Discharging),
            r("Full", Some(100), ChargeState::Full),
        ];
        assert_eq!(
            tooltip(&readings),
            "AJ159 APEX: 80% · about 3 h 20 min left\nPad: 5% · charging\nX: battery unavailable\nFull: 100% · fully charged"
        );
    }

    #[test]
    fn asleep_is_marked() {
        let mut a = r("M", Some(86), ChargeState::Discharging);
        a.asleep = true;
        assert_eq!(line(&a), "M: 86% · asleep (last known)");
    }

    #[test]
    fn asleep_readings_are_not_used_for_estimates() {
        let mut log = Log::default();
        let mut a = vec![r("M", Some(86), ChargeState::Discharging)];
        a[0].asleep = true;
        estimate(&mut a, &mut log, 0);
        assert!(log.devices["mouse:M"].samples.is_empty());
        assert_eq!(a[0].minutes_left, None);
    }

    #[test]
    fn tooltip_truncates_multibyte_safely() {
        let readings = vec![r(&"é".repeat(200), Some(1), ChargeState::Discharging)];
        assert_eq!(tooltip(&readings).chars().count(), 127);
    }

    #[test]
    fn tooltip_empty() {
        assert!(tooltip(&[]).contains("no wireless"));
    }
}
