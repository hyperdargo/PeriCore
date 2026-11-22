//! Per-key RGB painter for the MAXFIT61 (custom lighting effect).

use super::kb_visual::{self, Cap};
use super::theme;
use crate::devices::fantech::layout::{self, CELLS};
use crate::devices::fantech::Maxfit61;
use crate::store;
use eframe::egui::{self, Color32};

const STORE: &str = "maxfit61-perkey";
const SWATCHES: [[u8; 3]; 8] = [
    [255, 255, 255], [255, 0, 0], [255, 128, 0], [255, 230, 0],
    [0, 255, 80], [0, 210, 255], [40, 80, 255], [180, 60, 255],
];

pub struct PerKey {
    colors: [[u8; 3]; CELLS],
    brush: [u8; 3],
    brightness: u8,
}

impl PerKey {
    pub fn new(brightness_max: u8) -> Self {
        let saved: Option<Vec<[u8; 3]>> = store::load(STORE);
        let mut colors = [[255, 255, 255]; CELLS];
        if let Some(v) = saved.filter(|v| v.len() == CELLS) {
            colors.copy_from_slice(&v);
        }
        Self { colors, brush: [0, 210, 255], brightness: brightness_max }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, dev: &Maxfit61, brightness_max: u8) -> Option<Result<String, String>> {
        let mut status = None;
        theme::card(ui, "Paint keys", |ui| {
            ui.horizontal(|ui| {
                ui.label("Brush");
                egui::color_picker::color_edit_button_srgb(ui, &mut self.brush);
                for s in SWATCHES {
                    let c = Color32::from_rgb(s[0], s[1], s[2]);
                    let b = egui::Button::new("").fill(c).min_size(egui::vec2(24.0, 24.0));
                    if ui.add(b).clicked() {
                        self.brush = s;
                    }
                }
            });
            ui.add_space(6.0);
            let colors = &self.colors;
            if let Some(i) = kb_visual::keyboard(
                ui,
                |k| {
                    let [r, g, b] = colors[k.index];
                    Cap { top: k.label.to_string(), bottom: None, fill: Color32::from_rgb(r, g, b), selected: false }
                },
                true,
            ) {
                self.colors[i] = self.brush;
            }
            kb_visual::small_note(ui, "Click or drag across keys to paint them with the brush colour.");
            ui.horizontal(|ui| {
                if ui.button("Fill all").clicked() {
                    for k in layout::all_keys() {
                        self.colors[k.index] = self.brush;
                    }
                }
                if ui.button("Clear (black)").clicked() {
                    self.colors = [[0, 0, 0]; CELLS];
                }
            });
            ui.add(egui::Slider::new(&mut self.brightness, 0..=brightness_max).text("Brightness"));
        });
        if theme::primary(ui, "💾  Apply per-key lighting") {
            status = Some(match dev.write_custom_rgb(&self.colors, self.brightness) {
                Ok(()) => {
                    let _ = store::save(STORE, &self.colors.to_vec());
                    Ok("Per-key colours sent to keyboard".into())
                }
                Err(e) => Err(format!("Failed: {e:#}")),
            });
        }
        status
    }
}
