//! Battery history, written by the tray every poll and read by the dashboard.
//! Kept small: one sample per change (or per 10 minutes), 7 days at most.

use crate::devices::ry::protocol::ChargeState;
use crate::tray::estimate::History;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const STORE: &str = "battery-history";
const KEEP_S: u64 = 7 * 24 * 3600;
const MAX_SAMPLES: usize = 3000;
/// Record a sample at least this often even when nothing changed.
const HEARTBEAT_S: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    /// Unix seconds.
    pub t: u64,
    pub percent: u8,
    /// 0 discharging, 1 charging, 2 full.
    pub state: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceLog {
    pub name: String,
    /// "mouse", "controller" or "keyboard".
    pub kind: String,
    /// Last time the device answered (unix seconds).
    pub last_seen: u64,
    pub asleep: bool,
    /// Latest reading (also set while asleep, unlike `samples`).
    pub latest: Option<Sample>,
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Log {
    pub devices: BTreeMap<String, DeviceLog>,
}

/// One reading to record.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub percent: u8,
    pub state: ChargeState,
    pub asleep: bool,
}

pub fn state_code(s: ChargeState) -> u8 {
    match s {
        ChargeState::Discharging => 0,
        ChargeState::Charging => 1,
        ChargeState::Full => 2,
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Log {
    pub fn load() -> Self {
        crate::store::load(STORE).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = crate::store::save(STORE, self);
    }

    /// Records one reading. Asleep readings update `latest` but not the trend.
    pub fn record(&mut self, key: &str, e: Entry, t: u64) {
        let Entry { name, kind, percent, state, asleep } = e;
        let d = self.devices.entry(key.to_string()).or_default();
        let s = Sample { t, percent, state: state_code(state) };
        d.name = name.to_string();
        d.kind = kind.to_string();
        d.last_seen = t;
        d.asleep = asleep;
        d.latest = Some(s);
        if !asleep {
            let changed = d.samples.last().is_none_or(|l| l.percent != percent || l.state != s.state || t >= l.t + HEARTBEAT_S);
            if changed {
                d.samples.push(s);
            }
        }
        d.samples.retain(|x| x.t + KEEP_S >= t);
        if d.samples.len() > MAX_SAMPLES {
            let extra = d.samples.len() - MAX_SAMPLES;
            d.samples.drain(..extra);
        }
    }

    /// Minutes to empty (or to full while charging) from the current trend.
    pub fn minutes_left(&self, key: &str) -> Option<u32> {
        let d = self.devices.get(key)?;
        let last = d.samples.last()?;
        if last.state == 2 {
            return None;
        }
        let mut h = History::default();
        // Only the latest run of the same state, so a plug/unplug starts fresh.
        let start = d.samples.iter().rposition(|s| s.state != last.state).map_or(0, |i| i + 1);
        for s in &d.samples[start..] {
            h.push(s.t as f64, s.percent, s.state == 1);
        }
        h.minutes_left()
    }

    /// Samples from the last `secs` seconds, for the history line.
    pub fn recent(&self, key: &str, secs: u64, t: u64) -> Vec<Sample> {
        self.devices.get(key).map_or_else(Vec::new, |d| d.samples.iter().copied().filter(|s| s.t + secs >= t).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_changes_and_heartbeats_only() {
        let mut l = Log::default();
        for (t, p) in [(0, 80), (20, 80), (40, 79), (60, 79), (700, 79)] {
            l.record("m", Entry { name: "APEX", kind: "mouse", percent: p, state: ChargeState::Discharging, asleep: false }, t);
        }
        let ts: Vec<u64> = l.devices["m"].samples.iter().map(|s| s.t).collect();
        assert_eq!(ts, vec![0, 40, 700]);
    }

    #[test]
    fn asleep_updates_latest_but_not_trend() {
        let mut l = Log::default();
        l.record("m", Entry { name: "APEX", kind: "mouse", percent: 80, state: ChargeState::Discharging, asleep: false }, 0);
        l.record("m", Entry { name: "APEX", kind: "mouse", percent: 86, state: ChargeState::Discharging, asleep: true }, 30);
        let d = &l.devices["m"];
        assert_eq!(d.samples.len(), 1);
        assert_eq!(d.latest.unwrap().percent, 86);
        assert!(d.asleep);
    }

    #[test]
    fn estimate_uses_latest_state_run() {
        let mut l = Log::default();
        // Charged earlier, then a steady discharge of 1% per 6 min.
        l.record("p", Entry { name: "Pad", kind: "controller", percent: 100, state: ChargeState::Full, asleep: false }, 0);
        for i in 0..6u64 {
            l.record("p", Entry { name: "Pad", kind: "controller", percent: 60 - i as u8, state: ChargeState::Discharging, asleep: false }, 1000 + i * 360);
        }
        assert_eq!(l.minutes_left("p"), Some(330));
    }

    #[test]
    fn prunes_old_samples() {
        let mut l = Log::default();
        l.record("m", Entry { name: "A", kind: "mouse", percent: 50, state: ChargeState::Discharging, asleep: false }, 0);
        l.record("m", Entry { name: "A", kind: "mouse", percent: 49, state: ChargeState::Discharging, asleep: false }, KEEP_S + 10);
        assert_eq!(l.devices["m"].samples.len(), 1);
    }
}
