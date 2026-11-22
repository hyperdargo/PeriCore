//! Sound settings and the Equalizer APO text they turn into. Pure, so it is
//! unit-tested; writing the file lives in `eapo`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Centre frequencies (Hz) of the 10-band equalizer.
pub const BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
/// Slider range in dB.
pub const MAX_GAIN: f32 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Preset {
    #[default]
    Flat,
    Music,
    Movies,
    Games,
    Vocal,
    Bass,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 7] =
        [Preset::Flat, Preset::Music, Preset::Movies, Preset::Games, Preset::Vocal, Preset::Bass, Preset::Custom];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Flat => "Flat",
            Preset::Music => "Music",
            Preset::Movies => "Movies",
            Preset::Games => "Games",
            Preset::Vocal => "Vocal",
            Preset::Bass => "Bass boost",
            Preset::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Preset::Flat => "No change: the headphones as they are.",
            Preset::Music => "Warm, punchy lows and a little sparkle on top.",
            Preset::Movies => "Deeper explosions and clearer dialogue.",
            Preset::Games => "Footsteps and positional cues forward, less boom.",
            Preset::Vocal => "Voices up front for podcasts, calls and streams.",
            Preset::Bass => "Strong sub-bass and bass boost.",
            Preset::Custom => "Your own slider settings.",
        }
    }

    /// Band gains in dB; None for Custom (keeps the sliders as they are).
    pub fn gains(self) -> Option<[f32; 10]> {
        Some(match self {
            Preset::Flat => [0.0; 10],
            Preset::Music => [4.0, 3.5, 2.0, 0.5, -0.5, -0.5, 0.5, 1.5, 2.5, 2.0],
            Preset::Movies => [5.0, 4.0, 2.0, 0.0, -0.5, 0.5, 2.0, 2.0, 1.0, 0.5],
            Preset::Games => [-1.0, -0.5, -1.0, -1.5, -0.5, 1.0, 3.0, 4.0, 2.5, 1.0],
            Preset::Vocal => [-3.0, -2.5, -1.5, 0.0, 1.0, 2.0, 3.0, 2.5, 1.0, 0.0],
            Preset::Bass => [6.0, 5.5, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            Preset::Custom => return None,
        })
    }
}

/// An audio endpoint as stored in settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Endpoint GUID with braces, lowercase, e.g. "{1a2b...}".
    pub id: String,
    pub name: String,
}

/// A headphone correction imported from an AutoEQ ParametricEQ.txt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub name: String,
    pub preamp: f32,
    /// Filter lines, normalised to "Filter: ON ...".
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Mic {
    pub enabled: bool,
    pub device: Option<Device>,
    /// The RNNoise VST2 dll.
    pub plugin: Option<String>,
}

/// A saved preset: whether the EQ is on, whether the headphone correction
/// is used, and the band gains added on top of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub enabled: bool,
    pub correction: bool,
    pub bands: [f32; 10],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sound {
    pub enabled: bool,
    pub preset: Preset,
    pub bands: [f32; 10],
    /// Headphones/speakers to change; empty means every output device.
    pub outputs: Vec<Device>,
    pub correction: Option<Correction>,
    pub correction_on: bool,
    pub mic: Mic,
    /// Saved presets, switchable from the Sound page and the tray menu.
    pub profiles: Vec<Profile>,
    /// The saved preset in use, until a setting is changed by hand.
    pub active_profile: Option<String>,
}

impl Sound {
    /// Switches to a saved preset; false if there is none by that name.
    pub fn use_profile(&mut self, name: &str) -> bool {
        let Some(p) = self.profiles.iter().find(|p| p.name == name).cloned() else { return false };
        self.enabled = p.enabled;
        self.correction_on = p.correction;
        self.bands = p.bands;
        self.preset = Preset::Custom;
        self.active_profile = Some(p.name);
        true
    }

    /// Saves the current settings as a preset (replacing one of the same name).
    pub fn save_profile(&mut self, name: &str) {
        let p = Profile { name: name.to_string(), enabled: self.enabled, correction: self.correction_on, bands: self.bands };
        match self.profiles.iter_mut().find(|q| q.name == name) {
            Some(q) => *q = p,
            None => self.profiles.push(p),
        }
        self.active_profile = Some(name.to_string());
    }

    pub fn delete_profile(&mut self, name: &str) {
        self.profiles.retain(|p| p.name != name);
        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }
    }

    /// Name shown for what's playing now.
    pub fn label(&self) -> &str {
        self.active_profile.as_deref().unwrap_or(self.preset.name())
    }
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            enabled: true,
            preset: Preset::Flat,
            bands: [0.0; 10],
            outputs: Vec::new(),
            correction: None,
            correction_on: true,
            mic: Mic::default(),
            profiles: Vec::new(),
            active_profile: None,
        }
    }
}

/// Parses an AutoEQ "ParametricEQ.txt" (Equalizer APO format).
pub fn parse_correction(name: &str, text: &str) -> Result<Correction> {
    let mut preamp = 0.0;
    let mut filters = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("Preamp:") {
            preamp = v.trim().trim_end_matches("dB").trim().parse()?;
        } else if line.starts_with("Filter") {
            let Some((_, rest)) = line.split_once(':') else { continue };
            let rest = rest.trim();
            let words: Vec<&str> = rest.split_whitespace().collect();
            // Only plain "ON <type> Fc <f> Hz Gain <g> dB Q <q>" filters.
            let ok = words.len() == 10
                && words[0] == "ON"
                && ["PK", "LSC", "HSC", "LS", "HS"].contains(&words[1])
                && words[2] == "Fc"
                && words[4] == "Hz"
                && words[5] == "Gain"
                && words[7] == "dB"
                && words[8] == "Q"
                && [3, 6, 9].iter().all(|&i| words[i].parse::<f32>().is_ok_and(f32::is_finite));
            if ok {
                filters.push(format!("Filter: {}", words.join(" ")));
            }
        }
    }
    if filters.is_empty() {
        bail!("no filters found - pick an AutoEQ \"ParametricEQ.txt\" file");
    }
    if !(-40.0..=0.0).contains(&preamp) {
        bail!("unexpected preamp {preamp} dB");
    }
    Ok(Correction { name: name.to_string(), preamp, filters })
}

/// `Device:` line matching the given endpoints by GUID.
fn device_line(devices: &[Device]) -> String {
    let ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
    format!("Device: {}", ids.join("; "))
}

fn fmt(v: f32) -> String {
    let s = format!("{v:.1}");
    if s == "-0.0" { "0.0".into() } else { s }
}

/// Renders the PeriCore part of the Equalizer APO configuration.
/// `all_outputs` is used when no output is chosen, so the EQ never lands on
/// a microphone.
pub fn render(s: &Sound, all_outputs: &[Device]) -> String {
    let mut out = String::from("# Written by PeriCore. Change these settings in PeriCore; edits here are replaced.\n");
    let outputs = if s.outputs.is_empty() { all_outputs } else { &s.outputs };
    if s.enabled && !outputs.is_empty() {
        out += &format!("\n# Sound: {}\n{}\n", s.label().replace('\n', " "), device_line(outputs));
        let correction = s.correction.as_ref().filter(|_| s.correction_on);
        let filters = correction.map_or(&[][..], |c| c.filters.as_slice());
        // Lower the level by the highest boost of correction + bands combined.
        let peak = super::response::peak_db(filters, &s.bands);
        let preamp = -((peak * 10.0).ceil() / 10.0) as f32;
        if preamp < 0.0 {
            out += &format!("Preamp: {} dB\n", fmt(preamp));
        }
        if let Some(c) = correction {
            out += &format!("# Headphone correction: {}\n", c.name.replace('\n', " "));
            for f in &c.filters {
                out += f;
                out.push('\n');
            }
        }
        if s.bands.iter().any(|&g| g != 0.0) {
            let pairs: Vec<String> = BANDS.iter().zip(s.bands).map(|(f, g)| format!("{f} {}", fmt(g))).collect();
            out += &format!("GraphicEQ: {}\n", pairs.join("; "));
        }
    }
    if let (true, Some(dev), Some(plugin)) = (s.mic.enabled, &s.mic.device, &s.mic.plugin) {
        out += &format!("\n# Microphone noise suppression\n{}\n", device_line(std::slice::from_ref(dev)));
        out += &format!("VSTPlugin: Library \"{plugin}\"\n");
    }
    // Leave the device filter on nothing so lines after the Include stay unaffected.
    out += "\nDevice: all\n";
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str) -> Device {
        Device { id: id.into(), name: "x".into() }
    }

    const AUTOEQ: &str = "Preamp: -6.4 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70\nFilter 2: ON PK Fc 2000 Hz Gain -3.1 dB Q 1.41\nFilter 3: ON HSC Fc 10000 Hz Gain 2.0 dB Q 0.70\n";

    #[test]
    fn parses_autoeq_file() {
        let c = parse_correction("KZ Castor", AUTOEQ).unwrap();
        assert_eq!(c.preamp, -6.4);
        assert_eq!(c.filters.len(), 3);
        assert_eq!(c.filters[1], "Filter: ON PK Fc 2000 Hz Gain -3.1 dB Q 1.41");
    }

    #[test]
    fn rejects_files_without_filters() {
        assert!(parse_correction("x", "hello").is_err());
        assert!(parse_correction("x", "Filter 1: ON PK Fc abc Hz Gain 1 dB Q 1").is_err());
        assert!(parse_correction("x", "Filter: ON IIR Order 2 Coefficients 1 2 3").is_err());
    }

    #[test]
    fn flat_writes_no_eq() {
        let s = Sound::default();
        let t = render(&s, &[dev("{a}")]);
        assert!(t.contains("Device: {a}"));
        assert!(!t.contains("GraphicEQ"));
        assert!(!t.contains("Preamp"));
        assert!(t.trim_end().ends_with("Device: all"));
    }

    #[test]
    fn preset_gets_headroom_and_graphic_eq() {
        let s = Sound { preset: Preset::Bass, bands: Preset::Bass.gains().unwrap(), ..Sound::default() };
        let t = render(&s, &[dev("{a}"), dev("{b}")]);
        assert!(t.contains("Device: {a}; {b}"));
        assert!(t.contains("Preamp: -6.0 dB"));
        assert!(t.contains("GraphicEQ: 31 6.0; 62 5.5; 125 4.0"));
    }

    #[test]
    fn correction_adds_its_preamp() {
        let s = Sound {
            bands: [2.0; 10],
            correction: Some(parse_correction("c", AUTOEQ).unwrap()),
            outputs: vec![dev("{h}")],
            ..Sound::default()
        };
        let t = render(&s, &[dev("{a}")]);
        assert!(t.contains("Device: {h}\n"));
        let peak = crate::audio::response::peak_db(&s.correction.as_ref().unwrap().filters, &s.bands);
        assert!(t.contains(&format!("Preamp: -{:.1} dB", (peak * 10.0).ceil() / 10.0)), "{t}");
        assert!(peak > 7.0, "{peak}");
        assert!(t.contains("Filter: ON LSC Fc 105 Hz"));
    }

    #[test]
    fn disabled_sound_writes_only_mic() {
        let s = Sound {
            enabled: false,
            bands: [3.0; 10],
            mic: Mic { enabled: true, device: Some(dev("{m}")), plugin: Some(r"C:\p\rnnoise.dll".into()) },
            ..Sound::default()
        };
        let t = render(&s, &[dev("{a}")]);
        assert!(!t.contains("GraphicEQ"));
        assert!(t.contains("Device: {m}\nVSTPlugin: Library \"C:\\p\\rnnoise.dll\""));
    }

    #[test]
    fn saved_profiles_switch_everything() {
        let mut s = Sound { correction: Some(parse_correction("c", AUTOEQ).unwrap()), ..Sound::default() };
        s.bands = [1.0; 10];
        s.save_profile("Games");
        s.enabled = false;
        s.correction_on = false;
        s.save_profile("Default");
        assert!(s.use_profile("Games"));
        assert!(s.enabled && s.correction_on && s.bands == [1.0; 10]);
        assert_eq!(s.label(), "Games");
        assert!(!s.use_profile("Nope"));
        s.delete_profile("Games");
        assert_eq!(s.active_profile, None);
        assert_eq!(s.profiles.len(), 1);
    }

    #[test]
    fn old_settings_files_still_load() {
        let old = r#"{"enabled":true,"preset":"Games","bands":[0,0,0,0,0,0,0,0,0,0],"outputs":[],"correction":null,"correction_on":true,"mic":{"enabled":false,"device":null,"plugin":null}}"#;
        let s: Sound = serde_json::from_str(old).unwrap();
        assert!(s.profiles.is_empty() && s.active_profile.is_none());
    }

    #[test]
    fn mic_needs_device_and_plugin() {
        let s = Sound { mic: Mic { enabled: true, device: Some(dev("{m}")), plugin: None }, ..Sound::default() };
        assert!(!render(&s, &[]).contains("VSTPlugin"));
    }
}
