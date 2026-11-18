//! Settings window (egui). Opened from the tray; starts in well under a second.

mod keyboard;
mod mouse;

use crate::devices::{self, fantech::Maxfit61, ry::RyDevice, Detected, Family};
use crate::os;
use anyhow::Result;
use eframe::egui;
use hidapi::HidApi;

enum Panel {
    Mouse(Box<mouse::MousePanel>),
    Keyboard(keyboard::KeyboardPanel),
    Error(String),
}

struct App {
    api: HidApi,
    devices: Vec<Detected>,
    selected: usize,
    panel: Option<Panel>,
}

impl App {
    fn new(api: HidApi) -> Self {
        let devices = devices::discover(&api);
        let mut app = Self { api, devices, selected: 0, panel: None };
        app.open_selected();
        app
    }

    fn rescan(&mut self) {
        let _ = self.api.refresh_devices();
        self.devices = devices::discover(&self.api);
        self.selected = self.selected.min(self.devices.len().saturating_sub(1));
        self.open_selected();
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
                    Panel::Mouse(Box::new(mouse::MousePanel::new(d)))
                } else {
                    Panel::Error(format!("{}: keyboard settings for this model aren't supported yet.", info.name))
                }
            }),
            Family::FantechMaxfit61 => {
                Maxfit61::open(&self.api, info).map(|d| Panel::Keyboard(keyboard::KeyboardPanel::new(d)))
            }
        };
        self.panel = Some(panel.unwrap_or_else(|e| Panel::Error(format!("Could not open device: {e:#}"))));
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.heading("Devices");
        ui.add_space(6.0);
        let mut clicked = None;
        for (i, d) in self.devices.iter().enumerate() {
            let label = format!("{}  {}", kind_glyph(d), d.name);
            if ui.selectable_label(i == self.selected, label).clicked() {
                clicked = Some(i);
            }
        }
        if self.devices.is_empty() {
            ui.weak("No supported device found.");
        }
        if let Some(i) = clicked {
            self.selected = i;
            self.open_selected();
        }
        ui.add_space(10.0);
        if ui.button("⟳ Rescan").clicked() {
            self.rescan();
        }
    }
}

fn kind_glyph(d: &Detected) -> &'static str {
    match d.kind {
        devices::Kind::Keyboard => "⌨",
        devices::Kind::Mouse | devices::Kind::Receiver => "🖱",
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("devices").resizable(false).exact_width(200.0).show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match &mut self.panel {
                Some(Panel::Mouse(p)) => p.show(ui),
                Some(Panel::Keyboard(p)) => p.show(ui),
                Some(Panel::Error(e)) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, e.as_str());
                }
                None => {
                    ui.label("Plug in a supported mouse or keyboard, then press Rescan.");
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
        return Ok(());
    };
    os::ensure_tray_running();
    let api = HidApi::new()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(os::WINDOW_TITLE)
            .with_inner_size([820.0, 620.0])
            .with_min_inner_size([640.0, 420.0])
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../assets/pericore-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(os::WINDOW_TITLE, options, Box::new(move |_cc| Ok(Box::new(App::new(api)))))
        .map_err(|e| anyhow::anyhow!("window error: {e}"))
}

/// Shows a status line with colour by outcome.
pub(crate) fn status_line(ui: &mut egui::Ui, status: &Option<Result<String, String>>) {
    match status {
        Some(Ok(m)) => {
            ui.colored_label(egui::Color32::LIGHT_GREEN, m.as_str());
        }
        Some(Err(m)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, m.as_str());
        }
        None => {}
    }
}
