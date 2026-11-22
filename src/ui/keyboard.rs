//! Fantech MAXFIT61 panel: lighting effect, colours, brightness/speed, reset.

use crate::devices::fantech::protocol::{KbLighting, LightCaps};
use crate::devices::fantech::Maxfit61;
use crate::devices::fantech::keymap::CUSTOM_EFFECT;
use super::kb_keys::KeysTab;
use super::macro_editor::{MacroEditor, Target};
use super::kb_perkey::PerKey;
use super::theme;
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Lighting,
    Keys,
    Macros,
    Device,
}

const DIRECTIONS: [&str; 4] = ["Right", "Left", "Down", "Up"];

pub struct KeyboardPanel {
    dev: Maxfit61,
    firmware: Option<String>,
    /// None = capability query failed; lighting controls are hidden.
    caps: Option<LightCaps>,
    lighting: KbLighting,
    confirm_reset: bool,
    tab: Tab,
    keys: KeysTab,
    macros: MacroEditor,
    perkey: PerKey,
    per_key_mode: bool,
    status: Option<Result<String, String>>,
}

/// Effect names by ledType, taken from the vendor app's effect button group IDs.
fn effect_name(id: u8) -> String {
    const NAMES: [&str; 18] = [
        "Static", "Neon", "Breathing", "Wave", "Twinkle", "Snake", "Radar", "Aurora", "Ripple",
        "Speed Respond", "Reactive", "Custom", "Cross", "DPI", "Music Rhythm", "Auto Ripple",
        "Striation", "Fireworks",
    ];
    NAMES.get(id as usize).map_or_else(|| format!("Effect {id}"), |n| (*n).to_string())
}

impl KeyboardPanel {
    pub fn new(dev: Maxfit61) -> Self {
        let firmware = dev.identify();
        let caps = dev.light_caps();
        let lighting = caps.as_ref().map_or_else(KbLighting::default, |c| KbLighting {
            effect: c.effects.iter().copied().find(|&e| e != 0).unwrap_or(0),
            brightness: c.brightness_max,
            speed: c.speed_max / 2,
            ..Default::default()
        });
        let bmax = caps.as_ref().map_or(4, |c| c.brightness_max);
        Self {
            dev,
            firmware,
            caps,
            lighting,
            confirm_reset: false,
            tab: Tab::Lighting,
            keys: KeysTab::new(),
            macros: MacroEditor::new(Target::Keyboard),
            perkey: PerKey::new(bmax),
            per_key_mode: false,
            status: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let name = self.dev.info.name.clone();
        super::header(ui, &name, "Wired keyboard · USB", None);
        theme::tabs(
            ui,
            &mut self.tab,
            &[(Tab::Lighting, "Lighting"), (Tab::Keys, "Keys"), (Tab::Macros, "Macros"), (Tab::Device, "Device")],
        );
        ui.add_space(10.0);
        match self.tab {
            Tab::Lighting => {
                // Per-key colours need the firmware's custom effect (ledType 11).
                let per_key_ok = self.caps.as_ref().is_some_and(|c| c.effects.contains(&CUSTOM_EFFECT));
                self.per_key_mode &= per_key_ok;
                if per_key_ok {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.per_key_mode, false, "✨ Effects");
                        ui.selectable_value(&mut self.per_key_mode, true, "🎨 Per-key colours");
                    });
                    ui.add_space(6.0);
                }
                if self.per_key_mode {
                    let bmax = self.caps.as_ref().map_or(4, |c| c.brightness_max);
                    if let Some(s) = self.perkey.show(ui, &self.dev, bmax) {
                        self.status = Some(s);
                    }
                } else {
                    self.lighting_tab(ui);
                }
            }
            Tab::Keys => {
                if let Some(s) = self.keys.show(ui, &self.dev) {
                    self.status = Some(s);
                }
            }
            Tab::Macros => {
                self.macros.show(ui);
                theme::card(ui, "Use a macro", |ui| {
                    ui.label("Assign a macro to a key in the Keys tab (choose Macro). It's uploaded when you press Apply to keyboard.")
                });
            }
            Tab::Device => {
                theme::card(ui, "Device info", |ui| {
                    egui::Grid::new("kbinfo").num_columns(2).show(ui, |ui| {
                        ui.label(egui::RichText::new("Firmware").color(theme::MUTED));
                        ui.label(self.firmware.as_deref().unwrap_or("unknown"));
                        ui.end_row();
                        ui.label(egui::RichText::new("USB ID").color(theme::MUTED));
                        ui.label(format!("{:04X}:{:04X}", self.dev.info.vid, self.dev.info.pid));
                        ui.end_row();
                    });
                });
                self.maintenance(ui);
            }
        }
        super::status_line(ui, &self.status);
    }

    fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let l = &mut self.lighting;
        let Some(caps) = &self.caps else {
            theme::card(ui, "Lighting", |ui| {
                ui.colored_label(theme::BAD, "Couldn't read lighting capabilities, so lighting is disabled.")
            });
            return;
        };
        theme::card(ui, "Effect", |ui| {
            ui.checkbox(&mut l.on, "Backlight on");
            ui.horizontal_wrapped(|ui| {
                for &id in caps.effects.iter().filter(|&&id| id != CUSTOM_EFFECT) {
                    ui.selectable_value(&mut l.effect, id, effect_name(id));
                }
            });
        });
        theme::card(ui, "Style", |ui| {
            ui.add(egui::Slider::new(&mut l.brightness, 0..=caps.brightness_max).text("Brightness"));
            ui.add(egui::Slider::new(&mut l.speed, 0..=caps.speed_max).text("Speed"));
            egui::ComboBox::from_label("Direction")
                .selected_text(DIRECTIONS.get(l.direction as usize).copied().unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, d) in DIRECTIONS.iter().enumerate() {
                        ui.selectable_value(&mut l.direction, i as u8, *d);
                    }
                });
        });
        theme::card(ui, "Colours", |ui| {
            ui.checkbox(&mut l.rainbow, "Rainbow colours");
            ui.horizontal(|ui| {
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgb(ui, &mut l.color);
                ui.add_space(16.0);
                ui.label("Background");
                egui::color_picker::color_edit_button_srgb(ui, &mut l.background);
            });
        });
        if theme::primary(ui, "💾  Apply lighting") {
            self.status = Some(
                self.dev
                    .set_lighting(&self.lighting)
                    .map(|_| "Lighting sent to keyboard".into())
                    .map_err(|e| format!("Failed: {e:#}")),
            );
        }
    }

    fn maintenance(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, "Maintenance", |ui| {
            if !self.confirm_reset {
                if ui.button("Factory reset…").clicked() {
                    self.confirm_reset = true;
                }
            } else {
                ui.colored_label(theme::WARN, "Erase all keyboard settings and restore defaults?");
                ui.horizontal(|ui| {
                    if ui.button("Yes, reset").clicked() {
                        self.status = Some(
                            self.dev.factory_reset().map(|_| "Keyboard reset".into()).map_err(|e| format!("{e:#}")),
                        );
                        self.confirm_reset = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_reset = false;
                    }
                });
            }
        });
    }
}
