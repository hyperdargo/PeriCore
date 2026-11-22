//! Frequency response of the EQ PeriCore writes, used to pick a preamp that
//! keeps the loudest point at 0 dB so boosts never clip.
//!
//! Filters follow the RBJ "Audio EQ Cookbook" biquads that Equalizer APO and
//! AutoEQ use; GraphicEQ is linear in dB between bands on a log-frequency
//! axis and flat beyond the outer bands, as Equalizer APO draws it.

use super::config::BANDS;
use std::f64::consts::PI;

const FS: f64 = 48_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Peak,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Biquad {
    kind: Kind,
    fc: f64,
    gain: f64,
    q: f64,
}

/// Parses "Filter: ON PK Fc 100 Hz Gain 3 dB Q 1" (the form PeriCore stores).
fn parse(line: &str) -> Option<Biquad> {
    let w: Vec<&str> = line.split_once(':')?.1.split_whitespace().collect();
    let kind = match *w.get(1)? {
        "PK" => Kind::Peak,
        "LSC" | "LS" => Kind::LowShelf,
        "HSC" | "HS" => Kind::HighShelf,
        _ => return None,
    };
    Some(Biquad { kind, fc: w.get(3)?.parse().ok()?, gain: w.get(6)?.parse().ok()?, q: w.get(9)?.parse().ok()? })
}

impl Biquad {
    /// Gain in dB at frequency `f`.
    fn db(&self, f: f64) -> f64 {
        let a = 10f64.powf(self.gain / 40.0);
        let w0 = 2.0 * PI * self.fc / FS;
        let (sn, c) = w0.sin_cos();
        let alpha = sn / (2.0 * self.q);
        let sa = 2.0 * a.sqrt() * alpha;
        let (b, den) = match self.kind {
            Kind::Peak => ([1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a]),
            Kind::LowShelf => (
                [a * ((a + 1.0) - (a - 1.0) * c + sa), 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - sa)],
                [(a + 1.0) + (a - 1.0) * c + sa, -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - sa],
            ),
            Kind::HighShelf => (
                [a * ((a + 1.0) + (a - 1.0) * c + sa), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - sa)],
                [(a + 1.0) - (a - 1.0) * c + sa, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - sa],
            ),
        };
        let w = 2.0 * PI * f / FS;
        let mag = |k: [f64; 3]| {
            let re = k[0] + k[1] * w.cos() + k[2] * (2.0 * w).cos();
            let im = -(k[1] * w.sin() + k[2] * (2.0 * w).sin());
            (re * re + im * im).sqrt()
        };
        20.0 * (mag(b) / mag(den)).log10()
    }
}

/// GraphicEQ gain at `f` for the 10 band gains.
fn graphic_db(bands: &[f32; 10], f: f64) -> f64 {
    let pts: Vec<(f64, f64)> = BANDS.iter().zip(bands).map(|(&b, &g)| (f64::from(b).log10(), f64::from(g))).collect();
    let x = f.log10();
    if x <= pts[0].0 {
        return pts[0].1;
    }
    for pair in pts.windows(2) {
        let ((x0, g0), (x1, g1)) = (pair[0], pair[1]);
        if x <= x1 {
            return g0 + (g1 - g0) * (x - x0) / (x1 - x0);
        }
    }
    pts[pts.len() - 1].1
}

/// Total gain in dB at `f` of the correction filters plus the graphic EQ.
pub fn total_db(filters: &[String], bands: &[f32; 10], f: f64) -> f64 {
    filters.iter().filter_map(|l| parse(l)).map(|b| b.db(f)).sum::<f64>() + graphic_db(bands, f)
}

/// Highest gain (dB) anywhere from 20 Hz to 20 kHz; 0 if it never boosts.
pub fn peak_db(filters: &[String], bands: &[f32; 10]) -> f64 {
    const POINTS: usize = 600;
    (0..=POINTS)
        .map(|i| 20.0 * 1000f64.powf(i as f64 / POINTS as f64))
        .map(|f| total_db(filters, bands, f))
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn peak_filter_hits_its_gain_at_centre() {
        let f = [line("Filter: ON PK Fc 1000 Hz Gain 6.0 dB Q 1.0")];
        assert!((total_db(&f, &[0.0; 10], 1000.0) - 6.0).abs() < 0.01);
        assert!(total_db(&f, &[0.0; 10], 50.0).abs() < 0.1);
    }

    #[test]
    fn shelves_reach_full_gain_away_from_corner() {
        let ls = [line("Filter: ON LSC Fc 105 Hz Gain 4.0 dB Q 0.7")];
        assert!((total_db(&ls, &[0.0; 10], 20.0) - 4.0).abs() < 0.2);
        let hs = [line("Filter: ON HSC Fc 10000 Hz Gain -5.0 dB Q 0.7")];
        assert!((total_db(&hs, &[0.0; 10], 20000.0) + 5.0).abs() < 0.6);
        assert!(total_db(&hs, &[0.0; 10], 1000.0).abs() < 0.2);
    }

    #[test]
    fn graphic_eq_interpolates_on_log_axis() {
        let mut b = [0.0; 10];
        b[0] = 6.0; // 31 Hz
        assert!((graphic_db(&b, 20.0) - 6.0).abs() < 1e-9);
        assert!((graphic_db(&b, 31.0) - 6.0).abs() < 1e-9);
        let mid = (31.0_f64 * 62.0).sqrt();
        assert!((graphic_db(&b, mid) - 3.0).abs() < 1e-6);
        assert!(graphic_db(&b, 200.0).abs() < 1e-9);
    }

    #[test]
    fn peak_combines_correction_and_bands() {
        let f = [line("Filter: ON LSC Fc 105 Hz Gain 3.0 dB Q 0.7")];
        let mut b = [0.0; 10];
        b[0] = 2.0;
        let p = peak_db(&f, &b);
        assert!(p > 4.8 && p < 5.1, "{p}");
        assert_eq!(peak_db(&[], &[-3.0; 10]), 0.0);
    }
}
