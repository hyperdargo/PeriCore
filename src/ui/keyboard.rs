//! Fantech MAXFIT61 panel: lighting effect, colours, brightness/speed, reset.

use crate::devices::fantech::protocol::{KbLighting, LightCaps};
use crate::devices::fantech::Maxfit61;
use eframe::egui;

const DIRECTIONS: [&str; 4] = ["Right", "Left", "Down", "Up"];

pub struct KeyboardPanel {
    dev: Maxfit61,
    firmware: Option<String>,
    /// None = capability query failed; lighting controls are hidden.
    caps: Option<LightCaps>,
    lighting: KbLighting,
    confirm_reset: bool,
    status: Option<Result<String, String>>,
}

fn effect_name(id: u8) -> String {
    if id == 0 {
        "Off".into()
    } else {
        format!("Effect {id}")
    }
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
        Self { dev, firmware, caps, lighting, confirm_reset: false, status: None }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading(&self.dev.info.name);
        ui.label(format!("Firmware: {}", self.firmware.as_deref().unwrap_or("unknown")));
        ui.weak("Wired keyboard - no battery.");
        super::status_line(ui, &self.status);
        ui.separator();

        let mut apply = false;
        let l = &mut self.lighting;
        let Some(caps) = &self.caps else {
            ui.colored_label(egui::Color32::LIGHT_RED, "Couldn't read lighting capabilities; lighting is disabled.");
            self.maintenance(ui);
            return;
        };
        ui.collapsing("Lighting", |ui| {
            ui.checkbox(&mut l.on, "Backlight on");
            egui::ComboBox::from_label("Effect").selected_text(effect_name(l.effect)).show_ui(ui, |ui| {
                for &id in &caps.effects {
                    ui.selectable_value(&mut l.effect, id, effect_name(id));
                }
            });
            ui.add(egui::Slider::new(&mut l.brightness, 0..=caps.brightness_max).text("Brightness"));
            ui.add(egui::Slider::new(&mut l.speed, 0..=caps.speed_max).text("Speed"));
            egui::ComboBox::from_label("Direction")
                .selected_text(DIRECTIONS.get(l.direction as usize).copied().unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, d) in DIRECTIONS.iter().enumerate() {
                        ui.selectable_value(&mut l.direction, i as u8, *d);
                    }
                });
            ui.checkbox(&mut l.rainbow, "Rainbow colours");
            ui.horizontal(|ui| {
                ui.label("Colour");
                egui::color_picker::color_edit_button_srgb(ui, &mut l.color);
                ui.label("Background");
                egui::color_picker::color_edit_button_srgb(ui, &mut l.background);
            });
            ui.add_space(6.0);
            apply = ui.add_sized([140.0, 30.0], egui::Button::new("💾 Apply lighting")).clicked();
        });
        if apply {
            self.status = Some(
                self.dev
                    .set_lighting(&self.lighting)
                    .map(|_| "Lighting sent to keyboard".into())
                    .map_err(|e| format!("Failed: {e:#}")),
            );
        }

        self.maintenance(ui);
    }

    fn maintenance(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Maintenance", |ui| {
            if !self.confirm_reset {
                if ui.button("Factory reset…").clicked() {
                    self.confirm_reset = true;
                }
            } else {
                ui.colored_label(egui::Color32::YELLOW, "Erase all keyboard settings and restore defaults?");
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
