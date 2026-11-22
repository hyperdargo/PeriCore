//! Laptop-style "time left" from recent battery readings: a least-squares
//! line through the last few hours of samples, only while the trend is clear.

use std::collections::VecDeque;

/// Keep this much history (seconds).
const WINDOW_S: f64 = 3.0 * 3600.0;
/// Need at least this span and change before trusting a trend.
const MIN_SPAN_S: f64 = 20.0 * 60.0;
const MIN_CHANGE: f64 = 3.0;
const MIN_POINTS: usize = 3;

#[derive(Debug, Default, Clone)]
pub struct History {
    samples: VecDeque<(f64, f64)>,
    charging: bool,
}

impl History {
    /// Adds a reading at `t` seconds. Changing between charging and
    /// discharging starts a fresh trend.
    pub fn push(&mut self, t: f64, percent: u8, charging: bool) {
        if charging != self.charging {
            self.samples.clear();
            self.charging = charging;
        }
        if self.samples.back().is_some_and(|&(_, p)| p == f64::from(percent)) && self.samples.len() > 1 {
            // Flat stretch: move the last point forward instead of piling up duplicates.
            let n = self.samples.len();
            if self.samples[n - 2].1 == f64::from(percent) {
                self.samples.pop_back();
            }
        }
        self.samples.push_back((t, f64::from(percent)));
        while self.samples.front().is_some_and(|&(t0, _)| t - t0 > WINDOW_S) {
            self.samples.pop_front();
        }
    }

    /// Minutes until empty (discharging) or full (charging), if the trend is clear.
    pub fn minutes_left(&self) -> Option<u32> {
        let s = &self.samples;
        if s.len() < MIN_POINTS {
            return None;
        }
        let (first, last) = (s.front()?, s.back()?);
        if last.0 - first.0 < MIN_SPAN_S || (last.1 - first.1).abs() < MIN_CHANGE {
            return None;
        }
        let n = s.len() as f64;
        let mt = s.iter().map(|p| p.0).sum::<f64>() / n;
        let mp = s.iter().map(|p| p.1).sum::<f64>() / n;
        let cov: f64 = s.iter().map(|p| (p.0 - mt) * (p.1 - mp)).sum();
        let var: f64 = s.iter().map(|p| (p.0 - mt).powi(2)).sum();
        let slope = cov / var; // percent per second
        let remaining = if self.charging {
            (slope > 0.0).then(|| (100.0 - last.1) / slope)?
        } else {
            (slope < 0.0).then(|| last.1 / -slope)?
        };
        let mins = remaining / 60.0;
        (1.0..=7.0 * 24.0 * 60.0).contains(&mins).then_some(mins.round() as u32)
    }
}

/// "3 h 20 min", "45 min".
pub fn format_minutes(m: u32) -> String {
    match (m / 60, m % 60) {
        (0, min) => format!("{min} min"),
        (h, 0) => format!("{h} h"),
        (h, min) => format!("{h} h {min} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_discharge_predicts_time_to_empty() {
        let mut h = History::default();
        // 1% every 6 minutes from 60%.
        for i in 0..6 {
            h.push(f64::from(i) * 360.0, 60 - i as u8, false);
        }
        // Now at 55%, 6 min per % -> 330 min.
        assert_eq!(h.minutes_left(), Some(330));
    }

    #[test]
    fn needs_enough_data() {
        let mut h = History::default();
        h.push(0.0, 50, false);
        h.push(60.0, 49, false);
        h.push(120.0, 48, false);
        assert_eq!(h.minutes_left(), None, "span too short");
    }

    #[test]
    fn charging_predicts_time_to_full_and_resets_on_switch() {
        let mut h = History::default();
        for i in 0..5u8 {
            h.push(f64::from(i) * 600.0, 50 + i * 2, true);
        }
        // 2% per 10 min, at 58% -> 42% left -> 210 min.
        assert_eq!(h.minutes_left(), Some(210));
        h.push(3000.0, 58, false);
        assert_eq!(h.minutes_left(), None);
    }

    #[test]
    fn rising_while_discharging_gives_nothing() {
        let mut h = History::default();
        for i in 0..5u8 {
            h.push(f64::from(i) * 600.0, 40 + i * 2, false);
        }
        assert_eq!(h.minutes_left(), None);
    }

    #[test]
    fn formats() {
        assert_eq!(format_minutes(45), "45 min");
        assert_eq!(format_minutes(120), "2 h");
        assert_eq!(format_minutes(200), "3 h 20 min");
    }
}
