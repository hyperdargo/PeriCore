//! Home page: one card per battery device with a ring gauge, time left and a
//! 24-hour history line. Live readings come from a background thread; history
//! comes from the log the tray keeps, so estimates show up straight away.

use super::theme;
use crate::battery_log::{self, DeviceLog, Log, Sample};
use crate::devices::ry::protocol::ChargeState;
use crate::tray::estimate;
use crate::tray::monitor::{self, Reading};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Rounding, Shape, Stroke, Vec2};
use hidapi::HidApi;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

const POLL: Duration = Duration::from_secs(10);
const CARD_W: f32 = 230.0;
const CARD_H: f32 = 268.0;
const HISTORY_S: u64 = 24 * 3600;

/// One card: a live device, or one seen before that isn't connected now.
struct Card {
    /// HID path of a live device (used to open its page).
    path: Option<String>,
    name: String,
    kind: String,
    percent: Option<u8>,
    state: ChargeState,
    asleep: bool,
    last_seen: u64,
    minutes_left: Option<u32>,
    history: Vec<Sample>,
}

pub struct Dashboard {
    rx: Receiver<(Vec<Reading>, Log)>,
    wake: Sender<()>,
    active: Arc<AtomicBool>,
    cards: Vec<Card>,
    loaded: bool,
}

impl Dashboard {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (wake, wake_rx) = mpsc::channel();
        let active = Arc::new(AtomicBool::new(true));
        let flag = active.clone();
        std::thread::spawn(move || poll(tx, wake_rx, flag));
        Self { rx, wake, active, cards: Vec::new(), loaded: false }
    }

    /// Polling only runs while the dashboard is on screen, so it never talks
    /// to a device while its settings page is busy with it.
    pub fn set_active(&self, on: bool) {
        if on && !self.active.swap(true, Ordering::Relaxed) {
            let _ = self.wake.send(());
        }
        self.active.store(on, Ordering::Relaxed);
    }

    /// Draws the page; returns the HID path of a card that was clicked.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<String> {
        while let Ok((readings, log)) = self.rx.try_recv() {
            self.cards = build_cards(&readings, &log, battery_log::now());
            self.loaded = true;
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Devices").size(26.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([100.0, 32.0], egui::Button::new("⟳  Refresh")).clicked() {
                    let _ = self.wake.send(());
                }
            });
        });
        ui.label(RichText::new("Battery for every wireless device. Click a card for its settings.").color(theme::MUTED));
        ui.add_space(14.0);

        if !self.loaded {
            ui.label("Reading batteries…");
            return None;
        }
        if self.cards.is_empty() {
            ui.label(RichText::new("No battery devices found yet.").color(theme::MUTED));
            return None;
        }
        let now = battery_log::now();
        let t = ui.input(|i| i.time);
        egui::ScrollArea::vertical().show(ui, |ui| {
            let cols = ((ui.available_width() + 14.0) / (CARD_W + 14.0)).floor().max(1.0) as usize;
            for row in self.cards.chunks(cols) {
                ui.horizontal(|ui| {
                    for c in row {
                        if card(ui, c, now, t) {
                            clicked = c.path.clone();
                        }
                        ui.add_space(6.0);
                    }
                });
                ui.add_space(14.0);
            }
        });
        clicked
    }
}

fn poll(tx: Sender<(Vec<Reading>, Log)>, wake: Receiver<()>, active: Arc<AtomicBool>) {
    let Ok(mut api) = HidApi::new() else { return };
    loop {
        if active.load(Ordering::Relaxed) && !crate::os::screen_busy() {
            let mut readings = monitor::snapshot(&mut api);
            // Work on a copy of the tray's history; only the tray writes the file.
            let mut log = Log::load();
            monitor::estimate(&mut readings, &mut log, battery_log::now());
            if tx.send((readings, log)).is_err() {
                return;
            }
        }
        match wake.recv_timeout(POLL) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn build_cards(readings: &[Reading], log: &Log, now: u64) -> Vec<Card> {
    let mut cards: Vec<Card> = readings
        .iter()
        .map(|r| {
            let key = r.log_key();
            Card {
                path: Some(r.key.clone()),
                name: r.name.clone(),
                kind: monitor::kind_name(r.kind).to_string(),
                percent: r.percent,
                state: r.state,
                asleep: r.asleep,
                last_seen: if r.percent.is_some() { now } else { log.devices.get(&key).map_or(0, |d| d.last_seen) },
                minutes_left: r.minutes_left,
                history: log.recent(&key, HISTORY_S, now),
            }
        })
        .collect();
    // Devices seen in the last week that aren't plugged in now.
    let live: Vec<String> = readings.iter().map(Reading::log_key).collect();
    for (key, d) in &log.devices {
        if !live.contains(key) {
            cards.push(offline_card(key, d, log, now));
        }
    }
    cards
}

fn offline_card(key: &str, d: &DeviceLog, log: &Log, now: u64) -> Card {
    Card {
        path: None,
        name: d.name.clone(),
        kind: d.kind.clone(),
        percent: d.latest.map(|s| s.percent),
        state: ChargeState::Discharging,
        asleep: false,
        last_seen: d.last_seen,
        minutes_left: None,
        history: log.recent(key, HISTORY_S, now),
    }
}

fn glyph(kind: &str) -> &'static str {
    match kind {
        "controller" => "🎮",
        "keyboard" => "⌨",
        _ => "🖱",
    }
}

fn level_color(c: &Card) -> Color32 {
    match (c.path.is_some(), c.state, c.percent) {
        (false, ..) | (_, _, None) => theme::MUTED,
        (_, ChargeState::Charging | ChargeState::Full, _) => theme::GOOD,
        (_, _, Some(p)) => theme::battery_color(p),
    }
}

/// "Updated 40s ago", "Updated 5m ago".
pub fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s ago"),
        60..=3599 => format!("{}m ago", secs / 60),
        3600..=86399 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

/// The line under the name: time left, charging, asleep or offline.
fn detail(c: &Card, now: u64) -> String {
    if c.path.is_none() {
        return format!("Not connected · last seen {}", ago(now.saturating_sub(c.last_seen)));
    }
    if c.percent.is_none() {
        return "Battery unavailable".into();
    }
    if c.asleep {
        return "Asleep · showing last known level".into();
    }
    match (c.state, c.minutes_left) {
        (ChargeState::Full, _) => "Fully charged".into(),
        (ChargeState::Charging, Some(m)) => format!("~{} to full", estimate::format_minutes(m)),
        (ChargeState::Charging, None) => "Charging".into(),
        (_, Some(m)) => format!("~{} left", estimate::format_minutes(m)),
        (_, None) => "Estimating time left…".into(),
    }
}

/// Draws one card; returns true if clicked.
fn card(ui: &mut egui::Ui, c: &Card, now: u64, t: f64) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(CARD_W, CARD_H), egui::Sense::click());
    let charging = c.state == ChargeState::Charging && c.path.is_some();
    let hovered = resp.hovered() && c.path.is_some();
    let p = ui.painter_at(rect);
    let border = if charging { theme::GOOD } else if hovered { theme::ACCENT } else { theme::BORDER };
    p.rect(rect, Rounding::same(12.0), theme::CARD, Stroke::new(1.0_f32, border));

    let color = level_color(c);
    let dim = c.asleep || c.path.is_none();
    let color = if dim { color.gamma_multiply(0.6) } else { color };

    // Ring gauge.
    let centre = Pos2::new(rect.center().x, rect.top() + 82.0);
    let radius = 52.0;
    p.circle_stroke(centre, radius, Stroke::new(9.0_f32, theme::BORDER));
    if let Some(pc) = c.percent {
        arc(&p, centre, radius, f32::from(pc.min(100)) / 100.0, Stroke::new(9.0_f32, color));
        if charging {
            // A bright segment travels around the unfilled part, like a charging laptop.
            let from = f32::from(pc.min(100)) / 100.0;
            let head = from + (1.0 - from) * ((t % 2.0) / 2.0) as f32;
            arc_range(&p, centre, radius, from, head, Stroke::new(9.0_f32, color.linear_multiply(0.35)));
        }
    }
    let label = match (c.percent, charging) {
        (Some(pc), true) => format!("⚡{pc}%"),
        (Some(pc), false) => format!("{pc}%"),
        (None, _) => "--".into(),
    };
    p.text(centre, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(30.0), color);

    // Icon + name.
    let name_y = rect.top() + 160.0;
    let galley = p.layout_no_wrap(c.name.clone(), egui::FontId::proportional(15.0), theme::TEXT);
    let chip = 26.0;
    let total = chip + 8.0 + galley.size().x.min(CARD_W - 60.0);
    let x0 = rect.center().x - total / 2.0;
    let chip_rect = Rect::from_min_size(Pos2::new(x0, name_y - chip / 2.0), Vec2::splat(chip));
    p.rect_filled(chip_rect, Rounding::same(6.0), theme::BORDER);
    p.text(chip_rect.center(), egui::Align2::CENTER_CENTER, glyph(&c.kind), egui::FontId::proportional(15.0), theme::TEXT);
    p.text(Pos2::new(chip_rect.right() + 8.0, name_y), egui::Align2::LEFT_CENTER, &c.name, egui::FontId::proportional(15.0), theme::TEXT);

    // Status lines.
    let mut y = name_y + 24.0;
    if c.path.is_some() && c.percent.is_some() {
        let updated = format!("Updated {}", ago(now.saturating_sub(c.last_seen)));
        p.text(Pos2::new(rect.center().x, y), egui::Align2::CENTER_CENTER, updated, egui::FontId::proportional(12.5), theme::MUTED);
        y += 18.0;
    }
    let d = detail(c, now);
    if charging {
        let g = p.layout_no_wrap(format!("⚡ {d}"), egui::FontId::proportional(12.5), theme::GOOD);
        let r = Rect::from_center_size(Pos2::new(rect.center().x, y), g.size() + Vec2::new(16.0, 6.0));
        p.rect(r, Rounding::same(8.0), theme::GOOD.linear_multiply(0.15), Stroke::new(1.0_f32, theme::GOOD));
        p.galley(r.center() - g.size() / 2.0, g, theme::GOOD);
    } else {
        p.text(Pos2::new(rect.center().x, y), egui::Align2::CENTER_CENTER, d, egui::FontId::proportional(12.5), theme::MUTED);
    }

    // 24-hour history line along the bottom.
    let band = Rect::from_min_max(Pos2::new(rect.left() + 14.0, rect.bottom() - 34.0), Pos2::new(rect.right() - 14.0, rect.bottom() - 12.0));
    sparkline(&p, band, &c.history, now, color);

    if c.path.is_some() {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    } else {
        false
    }
}

fn arc(p: &egui::Painter, centre: Pos2, r: f32, frac: f32, stroke: Stroke) {
    arc_range(p, centre, r, 0.0, frac, stroke);
}

/// Clockwise arc from 12 o'clock, between two fractions of a full turn.
fn arc_range(p: &egui::Painter, centre: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    if to <= from {
        return;
    }
    let steps = ((to - from) * 96.0).ceil().max(2.0) as usize;
    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let f = from + (to - from) * i as f32 / steps as f32;
            let a = f * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            centre + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::line(pts, stroke));
}

fn sparkline(p: &egui::Painter, band: Rect, samples: &[Sample], now: u64, color: Color32) {
    if samples.len() < 2 {
        p.line_segment([band.left_bottom(), band.right_bottom()], Stroke::new(1.0_f32, theme::BORDER));
        return;
    }
    let start = now.saturating_sub(HISTORY_S) as f32;
    let pts: Vec<Pos2> = samples
        .iter()
        .map(|s| {
            let x = band.left() + band.width() * ((s.t as f32 - start) / HISTORY_S as f32).clamp(0.0, 1.0);
            let y = band.bottom() - band.height() * f32::from(s.percent.min(100)) / 100.0;
            Pos2::new(x, y)
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(2.0_f32, color)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery_log::Entry;
    use crate::devices::Kind;

    #[test]
    fn ago_units() {
        assert_eq!(ago(40), "40s ago");
        assert_eq!(ago(300), "5m ago");
        assert_eq!(ago(7200), "2h ago");
        assert_eq!(ago(200_000), "2d ago");
    }

    #[test]
    fn offline_devices_from_history_get_cards() {
        let mut log = Log::default();
        log.record("controller:DualShock 4", Entry { name: "DualShock 4", kind: "controller", percent: 58, state: ChargeState::Discharging, asleep: false }, 100);
        let cards = build_cards(&[], &log, 400);
        assert_eq!(cards.len(), 1);
        assert!(cards[0].path.is_none());
        assert_eq!(detail(&cards[0], 400), "Not connected · last seen 5m ago");
    }

    #[test]
    fn live_reading_detail() {
        let r = Reading {
            key: "path".into(),
            name: "APEX".into(),
            kind: Kind::Receiver,
            percent: Some(93),
            state: ChargeState::Discharging,
            asleep: false,
            minutes_left: Some(14 * 60),
        };
        let cards = build_cards(&[r], &Log::default(), 0);
        assert_eq!(detail(&cards[0], 0), "~14 h left");
        assert_eq!(cards[0].kind, "mouse");
    }
}
