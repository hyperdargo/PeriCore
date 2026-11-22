//! Visual style: dark navy surfaces with the cyan/violet accent of the icon,
//! plus small reusable widgets (cards, tab bar, pills).

use eframe::egui::{self, Color32, Margin, RichText, Rounding, Stroke};

pub const BG: Color32 = Color32::from_rgb(0x0E, 0x11, 0x1A);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x13, 0x17, 0x23);
pub const CARD: Color32 = Color32::from_rgb(0x1A, 0x1F, 0x2E);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x31, 0x45);
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE9, 0xF2);
pub const MUTED: Color32 = Color32::from_rgb(0x8A, 0x93, 0xA8);
pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xD3, 0xEE);
pub const ACCENT2: Color32 = Color32::from_rgb(0x8B, 0x5C, 0xF6);
pub const GOOD: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);
pub const WARN: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
pub const BAD: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(visuals());
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.slider_width = 220.0;
        s.text_styles.insert(egui::TextStyle::Heading, egui::FontId::proportional(22.0));
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
    });
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    let r = Rounding::same(8.0);
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(0x0A, 0x0D, 0x14);
    v.faint_bg_color = CARD;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_rounding = Rounding::same(12.0);
    for (w, fill, stroke) in [
        (&mut v.widgets.noninteractive, CARD, BORDER),
        (&mut v.widgets.inactive, Color32::from_rgb(0x24, 0x2A, 0x3C), BORDER),
        (&mut v.widgets.hovered, Color32::from_rgb(0x2C, 0x34, 0x4A), ACCENT.linear_multiply(0.6)),
        (&mut v.widgets.active, Color32::from_rgb(0x33, 0x3C, 0x55), ACCENT),
        (&mut v.widgets.open, Color32::from_rgb(0x2C, 0x34, 0x4A), ACCENT),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, stroke);
        w.rounding = r;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    v
}

/// A titled rounded card.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::same(16.0))
        .outer_margin(Margin { bottom: 12.0, ..Default::default() })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(15.5).strong().color(TEXT));
            ui.add_space(6.0);
            add(ui)
        })
        .inner
}

/// Horizontal tab strip; returns true when the selection changed.
pub fn tabs<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, items: &[(T, &str)]) -> bool {
    let before = *current;
    ui.horizontal(|ui| {
        for &(value, label) in items {
            let on = *current == value;
            let text = RichText::new(label).size(15.0).color(if on { TEXT } else { MUTED });
            let btn = egui::Button::new(text)
                .fill(if on { ACCENT.linear_multiply(0.18) } else { Color32::TRANSPARENT })
                .stroke(if on { Stroke::new(1.0_f32, ACCENT) } else { Stroke::NONE })
                .rounding(Rounding::same(18.0))
                .min_size(egui::vec2(96.0, 32.0));
            if ui.add(btn).clicked() {
                *current = value;
            }
        }
    });
    *current != before
}

/// Small coloured badge, e.g. a battery level.
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::none()
        .fill(color.linear_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color))
        .rounding(Rounding::same(12.0))
        .inner_margin(Margin::symmetric(10.0, 3.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).strong());
        });
}

/// Colour for a battery percentage, matching the tray icon thresholds.
pub fn battery_color(percent: u8) -> Color32 {
    match percent {
        0..=15 => BAD,
        16..=35 => WARN,
        _ => GOOD,
    }
}

/// Primary call-to-action button.
pub fn primary(ui: &mut egui::Ui, label: &str) -> bool {
    let btn = egui::Button::new(RichText::new(label).strong().color(Color32::from_rgb(0x06, 0x12, 0x18)))
        .fill(ACCENT)
        .rounding(Rounding::same(8.0))
        .min_size(egui::vec2(150.0, 34.0));
    ui.add(btn).clicked()
}
