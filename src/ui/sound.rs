//! Sound page: presets/EQ for every app, AutoEQ headphone correction and
//! RNNoise mic noise suppression, all applied through Equalizer APO.

use super::theme;
use crate::audio::config::{self, Device, Preset, Sound, BANDS, MAX_GAIN};
use crate::audio::{self, devices, eapo};
use crate::os;
use eframe::egui::{self, RichText};

const AUTOEQ_URL: &str = "https://autoeq.app";
const RNNOISE_URL: &str = "https://github.com/werman/noise-suppression-for-voice/releases";

pub struct SoundPanel {
    s: Sound,
    outputs: Vec<Device>,
    mics: Vec<Device>,
    status: eapo::Status,
    /// Endpoint ids with Equalizer APO turned on (outputs, then mics).
    active: (Vec<String>, Vec<String>),
    dirty: bool,
    msg: Option<Result<String, String>>,
}

impl SoundPanel {
    pub fn new() -> Self {
        Self {
            s: audio::load(),
            outputs: devices::list(false),
            mics: devices::list(true),
            status: eapo::status(),
            active: (Vec::new(), Vec::new()),
            dirty: false,
            msg: None,
        }
        .with_active()
    }

    fn with_active(mut self) -> Self {
        self.load_active();
        self
    }

    fn load_active(&mut self) {
        let on = |list: &[Device], capture| list.iter().filter(|d| devices::has_eapo(capture, &d.id)).map(|d| d.id.clone()).collect();
        self.active = (on(&self.outputs, false), on(&self.mics, true));
    }

    /// Warns when Equalizer APO isn't turned on for a chosen device.
    fn inactive_warning(&self, ui: &mut egui::Ui, missing: &[&Device]) {
        if self.status != eapo::Status::Ready || missing.is_empty() {
            return;
        }
        let names: Vec<&str> = missing.iter().map(|d| d.name.as_str()).collect();
        ui.add_space(4.0);
        ui.colored_label(
            theme::WARN,
            format!(
                "⚠  Equalizer APO isn't turned on for {}, so nothing changes yet. Click \"Choose devices…\" above, tick it, press OK, then restart Windows.",
                names.join(", ")
            ),
        );
    }

    fn refresh(&mut self) {
        self.outputs = devices::list(false);
        self.mics = devices::list(true);
        self.status = eapo::status();
        self.load_active();
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        super::header(ui, "Sound", "Presets for music, movies and games, and mic noise suppression - in every app.", None);
        self.engine_card(ui);
        self.mode_card(ui);
        self.correction_card(ui);
        self.mic_card(ui);
        super::status_line(ui, &self.msg);

        // Apply once a drag ends rather than on every slider step.
        if self.dirty && !ui.ctx().input(|i| i.pointer.any_down()) {
            self.dirty = false;
            self.msg = audio::save_and_apply(&self.s).err().map(|e| Err(format!("{e:#}")));
        }
    }

    fn engine_card(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, "Sound engine", |ui| match self.status {
            eapo::Status::NotInstalled => {
                ui.label("Windows only lets an app change the sound of every other app through an audio add-on. PeriCore uses the free Equalizer APO.");
                ui.add_space(4.0);
                ui.label(RichText::new("1. Download and install Equalizer APO.\n2. In its Device Selector, tick your headphones and your microphone.\n3. Restart Windows, then come back here.").color(theme::MUTED));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary(ui, "Download Equalizer APO") {
                        os::open_url(eapo::DOWNLOAD_URL);
                    }
                    if ui.button("I've installed it").clicked() {
                        self.refresh();
                    }
                });
            }
            eapo::Status::NotLinked => {
                ui.label("Equalizer APO is installed. Connect it so PeriCore can control it. Your current Equalizer APO settings are backed up first.");
                ui.add_space(6.0);
                if theme::primary(ui, "Connect") {
                    self.msg = Some(eapo::link().map(|()| "Connected to Equalizer APO".to_string()).map_err(|e| format!("{e:#}")));
                    self.refresh();
                    self.dirty = true;
                }
            }
            eapo::Status::Ready => {
                ui.horizontal(|ui| {
                    ui.colored_label(theme::GOOD, "✔  Equalizer APO connected");
                    if ui.button("Choose devices…").on_hover_text("Opens Equalizer APO's Device Selector (needs admin). Tick the devices PeriCore should change, then restart.").clicked() {
                        eapo::open_configurator();
                    }
                    if ui.button("⟳").on_hover_text("Reload the device list").clicked() {
                        self.refresh();
                    }
                });
            }
        });
    }

    fn output_combo(&mut self, ui: &mut egui::Ui) {
        let current = match self.s.outputs.first() {
            None => "All speakers and headphones".to_string(),
            Some(d) => d.name.clone(),
        };
        egui::ComboBox::from_label("Output").width(320.0).selected_text(current).show_ui(ui, |ui| {
            if ui.selectable_label(self.s.outputs.is_empty(), "All speakers and headphones").clicked() {
                self.s.outputs.clear();
                self.dirty = true;
            }
            for d in &self.outputs {
                let on = self.s.outputs.first() == Some(d);
                if ui.selectable_label(on, &d.name).clicked() {
                    self.s.outputs = vec![d.clone()];
                    self.dirty = true;
                }
            }
        });
    }

    fn mode_card(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, "Sound mode", |ui| {
            ui.horizontal(|ui| {
                self.dirty |= ui.checkbox(&mut self.s.enabled, "On").changed();
                ui.add_space(12.0);
                self.output_combo(ui);
            });
            if self.s.enabled {
                // With "all outputs", at least one must have Equalizer APO.
                let chosen: Vec<&Device> = if self.s.outputs.is_empty() { self.outputs.iter().collect() } else { self.s.outputs.iter().collect() };
                let missing: Vec<&Device> = chosen.iter().copied().filter(|d| !self.active.0.contains(&d.id)).collect();
                let none_active = missing.len() == chosen.len();
                let shown: Vec<&Device> = if self.s.outputs.is_empty() && !none_active { Vec::new() } else { missing };
                self.inactive_warning(ui, &shown);
            }
            ui.add_space(6.0);
            let items: Vec<(Preset, &str)> = Preset::ALL.iter().map(|&p| (p, p.name())).collect();
            if theme::tabs(ui, &mut self.s.preset, &items) {
                if let Some(g) = self.s.preset.gains() {
                    self.s.bands = g;
                }
                self.dirty = true;
            }
            ui.label(RichText::new(self.s.preset.description()).color(theme::MUTED));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().slider_width = 140.0;
                for (i, f) in BANDS.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(52.0);
                        let r = ui.add(
                            egui::Slider::new(&mut self.s.bands[i], -MAX_GAIN..=MAX_GAIN)
                                .vertical()
                                .step_by(0.5)
                                .show_value(false),
                        );
                        if r.changed() {
                            self.s.preset = Preset::Custom;
                            self.dirty = true;
                        }
                        ui.label(RichText::new(format!("{:+.1}", self.s.bands[i])).size(12.0));
                        let label = if *f >= 1000 { format!("{}k", f / 1000) } else { f.to_string() };
                        ui.label(RichText::new(label).size(12.0).color(theme::MUTED));
                    });
                }
            });
        });
    }

    fn import_correction(&mut self) {
        let Some(path) = os::pick_file("AutoEQ ParametricEQ.txt\0*.txt\0All files\0*.*\0") else { return };
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let name = stem.trim_end_matches("ParametricEQ").trim().to_string();
        let parsed = std::fs::read_to_string(&path)
            .map_err(anyhow::Error::from)
            .and_then(|t| config::parse_correction(if name.is_empty() { &stem } else { &name }, &t));
        match parsed {
            Ok(c) => {
                self.msg = Some(Ok(format!("Imported correction for {}", c.name)));
                self.s.correction = Some(c);
                self.s.correction_on = true;
                self.dirty = true;
            }
            Err(e) => self.msg = Some(Err(format!("{e:#}"))),
        }
    }

    fn correction_card(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, "Headphone correction", |ui| {
            match &self.s.correction {
                None => {
                    ui.label("Make your exact earphones sound neutral before the sound mode is added. Find your model on AutoEQ, download its \"ParametricEQ.txt\" and import it here.");
                }
                Some(c) => {
                    let label = format!("Correct for {} ({} filters)", c.name, c.filters.len());
                    self.dirty |= ui.checkbox(&mut self.s.correction_on, label).changed();
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Open AutoEQ").clicked() {
                    os::open_url(AUTOEQ_URL);
                }
                if ui.button("Import ParametricEQ.txt…").clicked() {
                    self.import_correction();
                }
                if self.s.correction.is_some() && ui.button("Remove").clicked() {
                    self.s.correction = None;
                    self.dirty = true;
                }
            });
        });
    }

    fn mic_card(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, "Microphone noise suppression", |ui| {
            ui.horizontal(|ui| {
                self.dirty |= ui.checkbox(&mut self.s.mic.enabled, "Remove background noise").changed();
                ui.add_space(12.0);
                let current = self.s.mic.device.as_ref().map_or("Choose a microphone", |d| d.name.as_str()).to_string();
                egui::ComboBox::from_label("Microphone").width(300.0).selected_text(current).show_ui(ui, |ui| {
                    for d in &self.mics {
                        if ui.selectable_label(self.s.mic.device.as_ref() == Some(d), &d.name).clicked() {
                            self.s.mic.device = Some(d.clone());
                            self.dirty = true;
                        }
                    }
                });
            });
            ui.add_space(4.0);
            match &self.s.mic.plugin {
                Some(p) => ui.label(RichText::new(format!("RNNoise plugin: {p}")).color(theme::MUTED)),
                None => ui.label("Uses the free RNNoise plugin: download the Windows release, unzip it, then locate the VST \"rnnoise\" .dll (the stereo one works with any mic)."),
            };
            ui.horizontal(|ui| {
                if ui.button("Get RNNoise").clicked() {
                    os::open_url(RNNOISE_URL);
                }
                if ui.button("Locate plugin .dll…").clicked() {
                    if let Some(p) = os::pick_file("VST plugin (*.dll)\0*.dll\0") {
                        self.s.mic.plugin = Some(p.display().to_string());
                        self.dirty = true;
                    }
                }
            });
            if let (true, Some(d)) = (self.s.mic.enabled, &self.s.mic.device) {
                if !self.active.1.contains(&d.id) {
                    self.inactive_warning(ui, &[d]);
                }
            }
            ui.label(RichText::new("RNNoise needs the mic at 48 kHz: Sound settings > your mic > Properties > Advanced. The mic must also be ticked in Equalizer APO's Device Selector.").size(12.0).color(theme::MUTED));
        });
    }
}
