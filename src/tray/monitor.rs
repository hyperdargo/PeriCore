//! Background battery polling for wireless devices.

use crate::devices::{self, ry::protocol::ChargeState, ry::RyDevice, Family, Kind};
use hidapi::HidApi;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub name: String,
    pub percent: Option<u8>,
    pub charging: bool,
}

pub const POLL_EVERY: Duration = Duration::from_secs(60);

/// Reads battery for every wireless-capable device currently plugged in.
pub fn snapshot(api: &mut HidApi) -> Vec<Reading> {
    let _ = api.refresh_devices();
    devices::discover(api)
        .into_iter()
        .filter(|d| d.family == Family::Ry && matches!(d.kind, Kind::Receiver | Kind::Mouse))
        .filter_map(|d| {
            let dev = RyDevice::open(api, &d).ok()?;
            let name = dev.identify().unwrap_or_else(|| d.name.clone());
            let battery = dev.battery().ok();
            Some(Reading {
                name,
                percent: battery.map(|b| b.percent),
                charging: battery.is_some_and(|b| b.state != ChargeState::Discharging),
            })
        })
        .collect()
}

/// Polls forever; a message on `wake` triggers an immediate refresh.
pub fn run(tx: Sender<Vec<Reading>>, wake: Receiver<()>) {
    let Ok(mut api) = HidApi::new() else { return };
    loop {
        if tx.send(snapshot(&mut api)).is_err() {
            return;
        }
        match wake.recv_timeout(POLL_EVERY) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Tooltip text (Windows caps tray tooltips at 127 chars).
pub fn tooltip(readings: &[Reading]) -> String {
    if readings.is_empty() {
        return "PeriCore - no wireless device".into();
    }
    let lines: Vec<String> = readings
        .iter()
        .map(|r| match r.percent {
            Some(p) if r.charging => format!("{}: {p}% (charging)", r.name),
            Some(p) => format!("{}: {p}%", r.name),
            None => format!("{}: battery unavailable", r.name),
        })
        .collect();
    // Truncate on a char boundary: device names may contain non-ASCII text.
    lines.join("\n").chars().take(127).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_formats() {
        let r = vec![
            Reading { name: "AJ159 APEX".into(), percent: Some(80), charging: false },
            Reading { name: "Kb".into(), percent: Some(5), charging: true },
            Reading { name: "X".into(), percent: None, charging: false },
        ];
        assert_eq!(tooltip(&r), "AJ159 APEX: 80%\nKb: 5% (charging)\nX: battery unavailable");
    }

    #[test]
    fn tooltip_truncates_multibyte_safely() {
        let r = vec![Reading { name: "é".repeat(200), percent: Some(1), charging: false }];
        assert_eq!(tooltip(&r).chars().count(), 127);
    }

    #[test]
    fn tooltip_empty() {
        assert!(tooltip(&[]).contains("no wireless"));
    }
}
