//! Controller page: a live, laptop-style battery gauge for PlayStation pads.

use super::theme;
use crate::devices::playstation::{Model, Pad};
use crate::devices::ry::protocol::{BatteryInfo, ChargeState};
use crate::devices::Detected;
use crate::tray::estimate::{self, History};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Rounding, Stroke, Vec2};
use std::time::Duration;

/// Re-read the pad this often (it streams reports, so a read is instant).
const REFRESH_S: f64 = 5.0;

pub struct ControllerPanel {
    pad: Pad,
    name: String,
    link: &'static str,
    battery: Option<BatteryInfo>,
    error: Option<String>,
    read_at: f64,
    history: History,
}

impl ControllerPanel {
    pub fn new(pad: Pad, info: &Detected) -> Self {
        // Bluetooth HID paths on Windows carry the BTHENUM / {00001124 service tag.
        let path = info.path.to_string_lossy().to_ascii_lowercase();
        let link = if path.contains("00001124") || path.contains("bth") { "Bluetooth" } else { "USB" };
        Self { pad, name: info.name.clone(), link, battery: None, error: None, read_at: f64::NEG_INFINITY, history: History::default() }
    }

    fn refresh(&mut self, now: f64) {
        self.read_at = now;
        match self.pad.battery() {
            Ok(b) => {
                if b.state != ChargeState::Full {
                    self.history.push(now, b.percent, b.state == ChargeState::Charging);
                }
                self.battery = Some(b);
                self.error = None;
            }
            Err(e) => self.error = Some(format!("{e:#}")),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        if now - self.read_at >= REFRESH_S {
            self.refresh(now);
        }
        let charging = self.battery.is_some_and(|b| b.state == ChargeState::Charging);
        // Repaint often while charging (animation), otherwise just for the next read.
        ui.ctx().request_repaint_after(Duration::from_millis(if charging { 50 } else { 1000 }));

        let model = match self.pad.model {
            Model::DualSense => "PlayStation 5 controller",
            Model::DualShock4 => "PlayStation 4 controller",
        };
        let pill = self.battery.map(|b| (b.percent, b.state != ChargeState::Discharging));
        super::header(ui, &self.name, &format!("{model} · {}", self.link), pill);

        theme::card(ui, "Battery", |ui| match (self.battery, &self.error) {
            (Some(b), _) => {
                gauge(ui, b, now);
                ui.add_space(8.0);
                ui.label(RichText::new(status_text(b, self.history.minutes_left())).size(16.0).color(theme::TEXT));
            }
            (None, Some(e)) => {
                ui.colored_label(theme::WARN, e);
            }
            (None, None) => {
                ui.label("Reading battery…");
            }
        });
        theme::card(ui, "Tips", |ui| {
            ui.label(RichText::new(
                "The tray shows this controller as its own battery icon. Plug it in with a USB cable to charge; \
                 the icon animates while charging and shows a check mark when full.",
            ).color(theme::MUTED));
        });
    }
}

/// "62% · about 3 h left", "Charging · 1 h 10 min to full", "Fully charged".
fn status_text(b: BatteryInfo, minutes: Option<u32>) -> String {
    match (b.state, minutes) {
        (ChargeState::Full, _) => "Fully charged".into(),
        (ChargeState::Charging, Some(m)) => format!("Charging · {} to full", estimate::format_minutes(m)),
        (ChargeState::Charging, None) => "Charging".into(),
        (ChargeState::Discharging, Some(m)) => format!("{}% · about {} left", b.percent, estimate::format_minutes(m)),
        (ChargeState::Discharging, None) => format!("{}% remaining", b.percent),
    }
}

/// Big battery drawing. While charging, a sweep rises from the level to full.
fn gauge(ui: &mut egui::Ui, b: BatteryInfo, t: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(420.0), 92.0), egui::Sense::hover());
    let p = ui.painter();
    let body = Rect::from_min_max(rect.min, Pos2::new(rect.max.x - 14.0, rect.max.y));
    let nub = Rect::from_min_max(Pos2::new(body.max.x + 2.0, body.center().y - 16.0), Pos2::new(rect.max.x, body.center().y + 16.0));
    p.rect_stroke(body, Rounding::same(12.0), Stroke::new(3.0_f32, theme::TEXT));
    p.rect_filled(nub, Rounding::same(4.0), theme::TEXT);

    let color = match b.state {
        ChargeState::Discharging => theme::battery_color(b.percent),
        _ => theme::GOOD,
    };
    let inner = body.shrink(8.0);
    let level = f32::from(b.percent.min(100)) / 100.0;
    let w = |frac: f32| Rect::from_min_size(inner.min, Vec2::new(inner.width() * frac.clamp(0.02, 1.0), inner.height()));
    p.rect_filled(w(level), Rounding::same(6.0), color);
    if b.state == ChargeState::Charging {
        let sweep = ((t % 2.0) / 2.0) as f32; // 0..1 every 2 s
        let to = level + (1.0 - level) * sweep;
        p.rect_filled(w(to), Rounding::same(6.0), color.linear_multiply(0.35));
    }
    let label = match b.state {
        ChargeState::Charging => format!("⚡ {}%", b.percent),
        ChargeState::Full => "✔ 100%".into(),
        ChargeState::Discharging => format!("{}%", b.percent),
    };
    p.text(body.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(30.0), Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines() {
        let b = |percent, state| BatteryInfo { percent, state };
        assert_eq!(status_text(b(100, ChargeState::Full), None), "Fully charged");
        assert_eq!(status_text(b(40, ChargeState::Charging), Some(70)), "Charging · 1 h 10 min to full");
        assert_eq!(status_text(b(62, ChargeState::Discharging), Some(180)), "62% · about 3 h left");
        assert_eq!(status_text(b(62, ChargeState::Discharging), None), "62% remaining");
    }
}
