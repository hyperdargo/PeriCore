//! Settings window (egui). Opened from the tray; starts in well under a second.

mod controller;
mod dashboard;
mod dock;
mod kb_keys;
mod kb_perkey;
mod kb_visual;
mod keyboard;
mod macro_editor;
mod mouse;
mod mouse_buttons;
mod sound;
mod theme;

use crate::devices::{self, fantech::Maxfit61, ry::RyDevice, Detected, Family};
use crate::os;
use anyhow::Result;
use eframe::egui;
use hidapi::HidApi;

enum Panel {
    Mouse(Box<mouse::MousePanel>),
    Keyboard(Box<keyboard::KeyboardPanel>),
    Controller(Box<controller::ControllerPanel>),
    Sound(Box<sound::SoundPanel>),
    Error(String),
}

struct App {
    api: HidApi,
    devices: Vec<Detected>,
    selected: usize,
    panel: Option<Panel>,
    /// Showing the dashboard (home) instead of a device page.
    home: bool,
    dashboard: dashboard::Dashboard,
}

impl App {
    fn new(api: HidApi) -> Self {
        let devices = devices::discover();
        Self { api, devices, selected: 0, panel: None, home: true, dashboard: dashboard::Dashboard::new() }
    }

    fn rescan(&mut self) {
        self.devices = devices::discover();
        self.selected = self.selected.min(self.devices.len().saturating_sub(1));
        if !self.home && !matches!(self.panel, Some(Panel::Sound(_))) {
            self.open_selected();
        }
    }

    fn go_home(&mut self) {
        // Close the device page so only the dashboard talks to devices.
        self.panel = None;
        self.home = true;
    }

    /// Opens the device whose HID path is `path` (from a dashboard card).
    fn open_path(&mut self, path: &str) {
        if let Some(i) = self.devices.iter().position(|d| d.path.to_string_lossy() == path) {
            self.selected = i;
            self.home = false;
            self.open_selected();
        }
    }

    fn open_selected(&mut self) {
        let Some(info) = self.devices.get(self.selected) else {
            self.panel = None;
            return;
        };
        let panel: Result<Panel> = match info.family {
            // Mouse SET commands must never reach an RY keyboard.
            Family::Ry => RyDevice::open(&self.api, info).map(|d| {
                if d.is_mouse() {
                    let screen = (info.kind == devices::Kind::Receiver)
                        .then(|| crate::devices::ry::screen::ScreenLink::open(&self.api, &info.path).ok())
                        .flatten();
                    Panel::Mouse(Box::new(mouse::MousePanel::new(d, screen)))
                } else {
                    Panel::Error(format!("{}: keyboard settings for this model aren't supported yet.", info.name))
                }
            }),
            Family::PlayStation => match devices::playstation::lookup(info.vid, info.pid) {
                Some((model, _)) => devices::playstation::Pad::open(&self.api, &info.path, model)
                    .map(|pad| Panel::Controller(Box::new(controller::ControllerPanel::new(pad, info)))),
                None => Err(anyhow::anyhow!("unknown controller")),
            },
            Family::FantechMaxfit61 => {
                Maxfit61::open(&self.api, info).map(|d| Panel::Keyboard(Box::new(keyboard::KeyboardPanel::new(d))))
            }
        };
        self.panel = Some(panel.unwrap_or_else(|e| Panel::Error(format!("Could not open device: {e:#}"))));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Peri").size(24.0).strong().color(theme::ACCENT));
            ui.add_space(-10.0);
            ui.label(egui::RichText::new("Core").size(24.0).strong().color(theme::ACCENT2));
        });
        ui.label(egui::RichText::new("Device hub").color(theme::MUTED));
        ui.add_space(18.0);
        if nav_item(ui, "🏠  Dashboard", self.home) {
            self.go_home();
        }
        let on_sound = matches!(self.panel, Some(Panel::Sound(_)));
        if nav_item(ui, "🎧  Sound", on_sound) && !on_sound {
            self.home = false;
            self.panel = Some(Panel::Sound(Box::new(sound::SoundPanel::new())));
        }
        ui.add_space(12.0);
        ui.label(egui::RichText::new("DEVICES").size(11.5).color(theme::MUTED));
        ui.add_space(4.0);
        let mut clicked = None;
        for (i, d) in self.devices.iter().enumerate() {
            if device_card(ui, d, !self.home && !on_sound && i == self.selected) {
                clicked = Some(i);
            }
        }
        if self.devices.is_empty() {
            ui.label(egui::RichText::new("No supported device found.").color(theme::MUTED));
        }
        if let Some(i) = clicked {
            self.selected = i;
            self.home = false;
            self.open_selected();
        }
        ui.add_space(10.0);
        if ui.add_sized([ui.available_width(), 32.0], egui::Button::new("⟳  Rescan devices")).clicked() {
            self.rescan();
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).size(11.5).color(theme::MUTED));
        });
    }
}

/// Plain sidebar navigation entry; returns true when clicked.
fn nav_item(ui: &mut egui::Ui, text: &str, selected: bool) -> bool {
    let fill = if selected { theme::ACCENT.linear_multiply(0.14) } else { egui::Color32::TRANSPARENT };
    let stroke = if selected { egui::Stroke::new(1.0_f32, theme::ACCENT) } else { egui::Stroke::NONE };
    let resp = ui.add_sized(
        [ui.available_width(), 36.0],
        egui::Button::new(egui::RichText::new(text).size(15.0)).fill(fill).stroke(stroke),
    );
    resp.clicked()
}

/// Selectable sidebar card for one device; returns true when clicked.
fn device_card(ui: &mut egui::Ui, d: &Detected, selected: bool) -> bool {
    let (fill, stroke) = if selected {
        (theme::ACCENT.linear_multiply(0.14), egui::Stroke::new(1.0_f32, theme::ACCENT))
    } else {
        (theme::CARD, egui::Stroke::new(1.0_f32, theme::BORDER))
    };
    let kind = match d.kind {
        devices::Kind::Keyboard => "Keyboard",
        devices::Kind::Mouse => "Mouse · wired",
        devices::Kind::Receiver => "Mouse · 2.4G",
        devices::Kind::Controller => "Controller",
    };
    let resp = egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(egui::Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(12.0, 9.0))
        .outer_margin(egui::Margin { bottom: 6.0, ..Default::default() })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(kind_glyph(d)).size(22.0));
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&d.name).strong());
                    ui.label(egui::RichText::new(kind).size(12.0).color(theme::MUTED));
                });
            });
        })
        .response
        .interact(egui::Sense::click());
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn kind_glyph(d: &Detected) -> &'static str {
    match d.kind {
        devices::Kind::Keyboard => "⌨",
        devices::Kind::Mouse | devices::Kind::Receiver => "🖱",
        devices::Kind::Controller => "🎮",
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("devices")
            .resizable(false)
            .exact_width(230.0)
            .frame(egui::Frame::none().fill(theme::SIDEBAR).inner_margin(egui::Margin::symmetric(14.0, 6.0)))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::BG).inner_margin(egui::Margin::same(22.0)))
            .show(ctx, |ui| {
            self.dashboard.set_active(self.home);
            if self.home {
                if let Some(path) = self.dashboard.show(ui) {
                    self.open_path(&path);
                }
                return;
            }
            egui::ScrollArea::vertical().show(ui, |ui| match &mut self.panel {
                Some(Panel::Mouse(p)) => p.show(ui),
                Some(Panel::Keyboard(p)) => p.show(ui),
                Some(Panel::Controller(p)) => p.show(ui),
                Some(Panel::Sound(p)) => p.show(ui),
                Some(Panel::Error(e)) => {
                    theme::card(ui, "Can't open this device", |ui| ui.colored_label(theme::BAD, e.as_str()));
                }
                None => {
                    theme::card(ui, "No device", |ui| {
                        ui.label("Plug in a supported mouse or keyboard, then press Rescan.")
                    });
                }
            });
        });
    }
}

pub fn run() -> Result<()> {
    if os::focus_settings_window() {
        return Ok(());
    }
    let Some(_guard) = os::SingleInstance::acquire("PeriCore.Ui") else {
        os::focus_any_settings_window();
        return Ok(());
    };
    os::ensure_tray_running();
    let api = crate::devices::hid_api()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(os::WINDOW_TITLE)
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../assets/pericore-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(os::WINDOW_TITLE, options, Box::new(move |cc| {
        theme::apply(&cc.egui_ctx);
        Ok(Box::new(App::new(api)))
    }))
        .map_err(|e| anyhow::anyhow!("window error: {e}"))
}

/// Shows a status line with colour by outcome.
pub(crate) fn status_line(ui: &mut egui::Ui, status: &Option<Result<String, String>>) {
    match status {
        Some(Ok(m)) => {
            ui.colored_label(theme::GOOD, format!("✔  {m}"));
        }
        Some(Err(m)) => {
            ui.colored_label(theme::BAD, format!("⚠  {m}"));
        }
        None => {}
    }
}

/// Page header: device title, subtitle and optional battery pill.
pub(crate) fn header(ui: &mut egui::Ui, title: &str, subtitle: &str, battery: Option<(u8, bool)>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).size(26.0).strong());
            ui.label(egui::RichText::new(subtitle).color(theme::MUTED));
        });
        if let Some((p, charging)) = battery {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if charging {
                    theme::pill(ui, &format!("⚡ {p}% charging"), theme::ACCENT);
                } else {
                    theme::pill(ui, &format!("🔋 {p}%"), theme::battery_color(p));
                }
            });
        }
    });
    ui.add_space(12.0);
}
