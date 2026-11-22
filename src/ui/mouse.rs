//! Mouse settings panel: DPI stages, polling rate, sensor options, buttons, macros.

use crate::devices::ry::protocol::{flag, MouseOption0, MouseOption1, MAX_DPI_STAGES, POLL_RATES};
use crate::devices::ry::RyDevice;
use super::dock::DockTab;
use super::macro_editor::{MacroEditor, Target};
use super::mouse_buttons::ButtonsTab;
use super::theme;
use eframe::egui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Performance,
    Buttons,
    Macros,
    Dock,
}

const DEFAULT_COLORS: [[u8; 3]; MAX_DPI_STAGES] = [
    [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0],
    [255, 0, 255], [0, 255, 255], [255, 128, 0], [255, 255, 255],
];

/// Editable copy of the device settings.
struct Form {
    stages: Vec<u16>,
    colors: Vec<[u8; 3]>,
    current: u8,
    /// None = device reported a code we don't understand; left untouched on save.
    rate: Option<u32>,
    debounce: u8,
    ripple: bool,
    double_click: u8,
    power_save: bool,
    angle_snap: bool,
    lift_cutoff: u8,
    sleep_secs: u16,
}

impl Form {
    fn from_device(o0: &MouseOption0, o1: &MouseOption1) -> Self {
        let stages = o1.stages();
        Self {
            colors: (0..MAX_DPI_STAGES).map(|i| o1.stage_color(i)).collect(),
            current: o1.current_stage(),
            stages,
            rate: o0.poll_rate(),
            debounce: o0.debounce_ms(),
            ripple: o0.has_flag(flag::RIPPLE_CORRECTION),
            double_click: o0.double_click(),
            power_save: o0.has_flag(flag::POWER_SAVE),
            angle_snap: o0.angle_snap(),
            lift_cutoff: o0.lift_cutoff(),
            sleep_secs: o0.sleep_secs(),
        }
    }

    fn apply_to(&self, o0: &MouseOption0, o1: &MouseOption1) -> (MouseOption0, MouseOption1) {
        let mut n0 = o0.clone();
        if let Some(hz) = self.rate {
            n0 = n0.with_poll_rate(hz);
        }
        let n0 = n0
            .with_debounce_ms(self.debounce)
            .with_double_click(self.double_click)
            .with_flag(flag::RIPPLE_CORRECTION, self.ripple)
            .with_flag(flag::POWER_SAVE, self.power_save)
            .with_angle_snap(self.angle_snap)
            .with_lift_cutoff(self.lift_cutoff)
            .with_sleep_secs(self.sleep_secs);
        let n1 = o1.with_stages(self.current, &self.stages, &self.colors);
        (n0, n1)
    }
}

const BATTERY_REFRESH_S: f64 = 15.0;

pub struct MousePanel {
    dev: RyDevice,
    title: String,
    /// (percent, charging) and when it was last read.
    battery: Option<(u8, bool)>,
    battery_at: f64,
    tab: Tab,
    buttons: ButtonsTab,
    macros: MacroEditor,
    /// Present for 2.4G receivers with a dock screen.
    dock: Option<DockTab>,
    loaded: Option<(MouseOption0, MouseOption1)>,
    form: Option<Form>,
    status: Option<Result<String, String>>,
}

impl MousePanel {
    pub fn new(dev: RyDevice, screen: Option<crate::devices::ry::screen::ScreenLink>) -> Self {
        let title = dev.identify().unwrap_or_else(|| dev.info.name.clone());
        let mut p = Self { dev, title, battery: None, battery_at: 0.0, tab: Tab::Performance, buttons: ButtonsTab::new(), macros: MacroEditor::new(Target::Mouse), dock: screen.map(|l| DockTab::new(Some(l))), loaded: None, form: None, status: None };
        p.reload();
        p
    }

    fn read_battery(&mut self) {
        use crate::devices::ry::protocol::ChargeState;
        self.battery = self.dev.battery().ok().map(|b| (b.percent, b.state != ChargeState::Discharging));
    }

    fn reload(&mut self) {
        self.read_battery();
        let read = self.dev.read_option0().and_then(|o0| Ok((o0, self.dev.read_option1()?)));
        self.buttons.table = if read.is_ok() { self.dev.read_buttons().ok() } else { None };
        match read {
            Ok((o0, o1)) => {
                self.form = Some(Form::from_device(&o0, &o1));
                self.loaded = Some((o0, o1));
                self.status = Some(Ok("Settings loaded from mouse".into()));
            }
            Err(e) => {
                // Never keep a stale form around: Apply must only write freshly read blocks.
                self.form = None;
                self.loaded = None;
                self.status = Some(Err(format!("Couldn't read settings: {e:#}")));
            }
        }
    }

    fn save(&mut self) {
        let (Some((o0, o1)), Some(form)) = (&self.loaded, &self.form) else { return };
        let (n0, n1) = form.apply_to(o0, o1);
        let res = self.dev.write_option0(&n0).and_then(|_| self.dev.write_option1(&n1));
        self.status = Some(match res {
            Ok(()) => {
                self.loaded = Some((n0, n1));
                Ok("Saved to mouse".into())
            }
            Err(e) => Err(format!("Save failed: {e:#}")),
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let subtitle = match self.dev.info.kind {
            crate::devices::Kind::Receiver => "Wireless mouse · 2.4G receiver",
            _ => "Wired mouse",
        };
        // Re-read battery now and then so docking/undocking shows up.
        let now = ui.input(|i| i.time);
        if now - self.battery_at > BATTERY_REFRESH_S && !crate::os::screen_busy() {
            self.battery_at = now;
            self.read_battery();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(BATTERY_REFRESH_S));
        super::header(ui, &self.title, subtitle, self.battery);
        let mut tabs = vec![(Tab::Performance, "Performance"), (Tab::Buttons, "Buttons"), (Tab::Macros, "Macros")];
        if self.dock.is_some() {
            tabs.push((Tab::Dock, "Dock screen"));
        }
        theme::tabs(ui, &mut self.tab, &tabs);
        ui.add_space(10.0);

        let mut save = false;
        let mut reload = false;
        if let Some(form) = &mut self.form {
            match self.tab {
                Tab::Performance => {
                    dpi_section(ui, form);
                    performance_section(ui, form);
                }
                Tab::Buttons | Tab::Dock | Tab::Macros => {}
            }
        }
        if self.tab == Tab::Macros {
            self.macros.show(ui);
            theme::card(ui, "Use a macro", |ui| {
                ui.label("Assign a macro to a button in the Buttons tab (choose Macro). It's uploaded to the mouse when you press Apply there.")
            });
            return;
        }
        if self.tab == Tab::Dock {
            if let Some(d) = &mut self.dock {
                d.show(ui);
            }
            return;
        }
        if self.tab == Tab::Buttons {
            if let Some(s) = self.buttons.show(ui, &self.dev) {
                self.status = Some(s);
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if self.form.is_some() && self.tab != Tab::Buttons {
                save = theme::primary(ui, "💾  Apply to mouse");
            }
            reload = ui.add_sized([110.0, 34.0], egui::Button::new("⟳  Reload")).clicked();
            super::status_line(ui, &self.status);
        });
        if save {
            self.save();
        }
        if reload {
            self.reload();
        }
    }
}

fn dpi_section(ui: &mut egui::Ui, f: &mut Form) {
    theme::card(ui, "DPI stages", |ui| {
        let mut count = f.stages.len();
        ui.add(egui::Slider::new(&mut count, 1..=MAX_DPI_STAGES).text("stages"));
        while f.stages.len() < count {
            f.stages.push(f.stages.last().copied().unwrap_or(800).saturating_add(400).min(26000));
        }
        f.stages.truncate(count);
        while f.colors.len() < MAX_DPI_STAGES {
            f.colors.push(DEFAULT_COLORS[f.colors.len()]);
        }
        f.current = f.current.min(count as u8 - 1);
        egui::Grid::new("dpi").num_columns(3).show(ui, |ui| {
            for i in 0..count {
                ui.radio_value(&mut f.current, i as u8, format!("Stage {}", i + 1));
                ui.add(egui::DragValue::new(&mut f.stages[i]).range(50..=26000).speed(50).suffix(" DPI"));
                egui::color_picker::color_edit_button_srgb(ui, &mut f.colors[i]);
                ui.end_row();
            }
        });
        ui.label(egui::RichText::new("The selected stage is the active DPI.").color(theme::MUTED));
    });
}

fn performance_section(ui: &mut egui::Ui, f: &mut Form) {
    theme::card(ui, "Sensor & performance", |ui| {
        let shown = f.rate.map_or("Unknown (unchanged)".to_string(), |hz| format!("{hz} Hz"));
        egui::ComboBox::from_label("Polling rate").selected_text(shown).show_ui(ui, |ui| {
            for hz in POLL_RATES {
                ui.selectable_value(&mut f.rate, Some(hz), format!("{hz} Hz"));
            }
        });
        if f.double_click > 0 {
            ui.add(egui::Slider::new(&mut f.double_click, 1..=255).text("Double-click interval"));
        }
        ui.add(egui::Slider::new(&mut f.debounce, 0..=30).clamping(egui::SliderClamping::Never).text("Debounce (ms)"));
        ui.add(egui::Slider::new(&mut f.lift_cutoff, 0..=2).clamping(egui::SliderClamping::Never).text("Lift-off distance (0 low - 2 high)"));
        ui.checkbox(&mut f.ripple, "Ripple correction (smoother tracking)");
        ui.checkbox(&mut f.angle_snap, "Angle snapping");
        ui.checkbox(&mut f.power_save, "Power-saving mode");
        ui.add(egui::Slider::new(&mut f.sleep_secs, 0..=1800).clamping(egui::SliderClamping::Never).text("Sleep after idle (s, 0 = never)"));
        ui.label(egui::RichText::new("Polling rates above 1000 Hz need the 8K receiver.").color(theme::MUTED));
    });
}
