//! Clickable on-screen MAXFIT61, shared by the Keys and per-key Lighting tabs.

use super::theme;
use crate::devices::fantech::layout::{self, Key};
use eframe::egui::{self, Color32, RichText, Rounding, Sense, Stroke};

/// How one key cap is drawn.
pub struct Cap {
    pub top: String,
    pub bottom: Option<String>,
    pub fill: Color32,
    pub selected: bool,
}

/// Draws the keyboard scaled to the available width; returns the matrix
/// index of a clicked (or drag-painted) key.
pub fn keyboard(ui: &mut egui::Ui, cap: impl Fn(&Key) -> Cap, paint_drag: bool) -> Option<usize> {
    const UNITS: f32 = 15.0;
    let unit = ((ui.available_width() - 8.0) / UNITS).clamp(30.0, 56.0);
    let gap = 4.0;
    let mut hit = None;
    for row in layout::ROWS {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for key in row {
                let size = egui::vec2(key.width * unit - gap, unit - gap);
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(key.width * unit, unit), Sense::click_and_drag());
                let rect = egui::Rect::from_center_size(rect.center(), size);
                let c = cap(key);
                let stroke = if c.selected {
                    Stroke::new(2.0_f32, theme::ACCENT)
                } else if resp.hovered() {
                    Stroke::new(1.0_f32, theme::ACCENT.linear_multiply(0.7))
                } else {
                    Stroke::new(1.0_f32, theme::BORDER)
                };
                let painter = ui.painter();
                painter.rect(rect, Rounding::same(6.0), c.fill, stroke);
                let text_color = contrast(c.fill);
                let font = egui::FontId::proportional((unit * 0.27).clamp(10.0, 14.0));
                match &c.bottom {
                    Some(b) => {
                        painter.text(rect.center_top() + egui::vec2(0.0, unit * 0.28), egui::Align2::CENTER_CENTER, &c.top, font.clone(), text_color.linear_multiply(0.6));
                        painter.text(rect.center_bottom() - egui::vec2(0.0, unit * 0.28), egui::Align2::CENTER_CENTER, b, font, theme::ACCENT);
                    }
                    None => {
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER, &c.top, font, text_color);
                    }
                }
                let dragged_over = paint_drag
                    && ui.input(|i| i.pointer.primary_down())
                    && ui.rect_contains_pointer(rect);
                if resp.clicked() || dragged_over {
                    hit = Some(key.index);
                }
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        });
        ui.add_space(-4.0);
    }
    hit
}

/// Dark or light text depending on the cap colour.
fn contrast(fill: Color32) -> Color32 {
    let l = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if l > 150.0 {
        Color32::from_rgb(0x10, 0x14, 0x20)
    } else {
        theme::TEXT
    }
}

pub fn small_note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.5).color(theme::MUTED));
}
