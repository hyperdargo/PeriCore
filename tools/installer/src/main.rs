#![cfg_attr(not(test), windows_subsystem = "windows")]
//! PeriCore-Setup.exe: a small installer window around the embedded app.

#[path = "../../../src/install_layout.rs"]
mod layout;
mod install;

use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

const BG: Color32 = Color32::from_rgb(0x0F, 0x13, 0x1C);
const CARD: Color32 = Color32::from_rgb(0x17, 0x1D, 0x2A);
const BORDER: Color32 = Color32::from_rgb(0x2A, 0x33, 0x45);
const TEXT: Color32 = Color32::from_rgb(0xE6, 0xEA, 0xF2);
const MUTED: Color32 = Color32::from_rgb(0x8A, 0x94, 0xA8);
const ACCENT: Color32 = Color32::from_rgb(0x22, 0xD3, 0xEE);
const ACCENT2: Color32 = Color32::from_rgb(0xA7, 0x8B, 0xFA);
const GOOD: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);
const BAD: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);

enum Msg {
    Step(String),
    Done(Result<PathBuf, String>),
}

enum Stage {
    Options,
    Working { rx: Receiver<Msg>, step: String },
    Finished(PathBuf),
    Failed(String),
}

struct Setup {
    stage: Stage,
    opts: install::Options,
    launch: bool,
    previous: Option<String>,
}

impl Setup {
    fn start(&mut self) {
        let (tx, rx) = mpsc::channel();
        let opts = self.opts;
        std::thread::spawn(move || {
            let step_tx = tx.clone();
            let r = install::install(opts, &move |s| {
                let _ = step_tx.send(Msg::Step(s.to_string()));
            });
            let _ = tx.send(Msg::Done(r.map_err(|e| format!("{e:#}"))));
        });
        self.stage = Stage::Working { rx, step: "Starting…".into() };
    }

    fn options_page(&mut self, ui: &mut egui::Ui) {
        let title = match &self.previous {
            Some(v) if v == install::VERSION => format!("Reinstall PeriCore {}", install::VERSION),
            Some(v) => format!("Update PeriCore {v} to {}", install::VERSION),
            None => format!("Install PeriCore {}", install::VERSION),
        };
        ui.label(RichText::new(title).size(18.0).strong().color(TEXT));
        ui.add_space(4.0);
        ui.label(RichText::new("Battery dashboard, device settings and sound presets for your mouse, keyboard, controllers and headphones.").color(MUTED));
        ui.add_space(14.0);
        card(ui, |ui| {
            ui.label(RichText::new("Install location").color(MUTED).size(12.0));
            let dir = layout::install_dir().map_or_else(|| "?".into(), |d| d.display().to_string());
            ui.label(RichText::new(dir).monospace().color(TEXT));
            ui.label(RichText::new("Just for you - no administrator rights needed.").color(MUTED).size(12.0));
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.checkbox(&mut self.opts.desktop_shortcut, "Create a desktop shortcut");
            ui.checkbox(&mut self.opts.autostart, "Start with Windows (battery in the tray)");
            ui.checkbox(&mut self.launch, "Open PeriCore when setup finishes");
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if primary(ui, if self.previous.is_some() { "Update" } else { "Install" }) {
                self.start();
            }
            if ui.add_sized([100.0, 36.0], egui::Button::new("Cancel")).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn working_page(ui: &mut egui::Ui, step: &str) {
        ui.label(RichText::new("Installing…").size(18.0).strong().color(TEXT));
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(RichText::new(step).color(MUTED));
        });
    }
}

impl eframe::App for Setup {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut next = None;
        if let Stage::Working { rx, step } = &mut self.stage {
            while let Ok(m) = rx.try_recv() {
                match m {
                    Msg::Step(s) => *step = s,
                    Msg::Done(Ok(exe)) => {
                        if self.launch {
                            install::launch(&exe);
                        }
                        next = Some(Stage::Finished(exe));
                    }
                    Msg::Done(Err(e)) => next = Some(Stage::Failed(e)),
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if let Some(s) = next {
            self.stage = s;
        }

        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(26.0))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Peri").size(30.0).strong().color(ACCENT));
                ui.add_space(-8.0);
                ui.label(RichText::new("Core").size(30.0).strong().color(ACCENT2));
                ui.label(RichText::new("  Setup").size(20.0).color(MUTED));
            });
            ui.add_space(14.0);
            match &self.stage {
                Stage::Options => self.options_page(ui),
                Stage::Working { step, .. } => Self::working_page(ui, step),
                Stage::Finished(exe) => {
                    ui.label(RichText::new("✔  PeriCore is installed").size(18.0).strong().color(GOOD));
                    ui.add_space(6.0);
                    ui.label(RichText::new("Find it in the Start menu. To remove it: Settings > Apps > Installed apps > PeriCore > Uninstall.").color(MUTED));
                    ui.add_space(18.0);
                    let exe = exe.clone();
                    ui.horizontal(|ui| {
                        if primary(ui, "Finish") {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if !self.launch && ui.add_sized([140.0, 36.0], egui::Button::new("Open PeriCore")).clicked() {
                            install::launch(&exe);
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                }
                Stage::Failed(e) => {
                    ui.label(RichText::new("Setup couldn't finish").size(18.0).strong().color(BAD));
                    ui.add_space(6.0);
                    ui.label(RichText::new(e).color(TEXT));
                    ui.add_space(18.0);
                    if primary(ui, "Try again") {
                        self.stage = Stage::Options;
                    }
                }
            }
        });
    }
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add_sized([140.0, 36.0], egui::Button::new(RichText::new(text).strong().color(BG)).fill(ACCENT)).clicked()
}

fn theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.linear_multiply(0.6);
    v.widgets.inactive.bg_fill = CARD;
    v.widgets.inactive.weak_bg_fill = CARD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    ctx.set_visuals(v);
    ctx.style_mut(|s| s.spacing.item_spacing = egui::vec2(8.0, 8.0));
}

fn main() -> eframe::Result<()> {
    // `PeriCore-Setup.exe /S`: install with the default options, no window.
    if std::env::args().skip(1).any(|a| a.eq_ignore_ascii_case("/S") || a == "--silent") {
        let opts = install::Options { desktop_shortcut: true, autostart: true };
        let code = i32::from(install::install(opts, &|_| {}).is_err());
        std::process::exit(code);
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PeriCore Setup")
            .with_inner_size([560.0, 430.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../../assets/pericore-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "PeriCore Setup",
        options,
        Box::new(|cc| {
            theme(&cc.egui_ctx);
            Ok(Box::new(Setup {
                stage: Stage::Options,
                opts: install::Options { desktop_shortcut: true, autostart: true },
                launch: true,
                previous: install::installed_version(),
            }))
        }),
    )
}
