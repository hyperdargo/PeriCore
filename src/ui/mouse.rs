//! Mouse settings panel: DPI stages, polling rate, sensor options, lighting.

use crate::devices::ry::protocol::{flag, LightEffect, Lighting, MouseOption0, MouseOption1, MAX_DPI_STAGES, POLL_RATES};
use crate::devices::ry::protocol::ChargeState;
use crate::devices::ry::RyDevice;
use crate::devices::Kind;
use eframe::egui;

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
    motion_sync: bool,
    power_save: bool,
    angle_snap: bool,
    lift_cutoff: u8,
    sleep_secs: u16,
    /// None = unknown lighting block; left untouched on save.
    lighting: Option<Lighting>,
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
            motion_sync: o0.has_flag(flag::MOTION_SYNC),
            power_save: o0.has_flag(flag::POWER_SAVE),
            angle_snap: o0.angle_snap(),
            lift_cutoff: o0.lift_cutoff(),
            sleep_secs: o0.sleep_secs(),
            lighting: o0.lighting(),
        }
    }

    fn apply_to(&self, o0: &MouseOption0, o1: &MouseOption1) -> (MouseOption0, MouseOption1) {
        let mut n0 = o0.clone();
        if let Some(hz) = self.rate {
            n0 = n0.with_poll_rate(hz);
        }
        if let Some(l) = &self.lighting {
            n0 = n0.with_lighting(l);
        }
        let n0 = n0
            .with_debounce_ms(self.debounce)
            .with_flag(flag::MOTION_SYNC, self.motion_sync)
            .with_flag(flag::POWER_SAVE, self.power_save)
            .with_angle_snap(self.angle_snap)
            .with_lift_cutoff(self.lift_cutoff)
            .with_sleep_secs(self.sleep_secs);
        let n1 = o1.with_stages(self.current, &self.stages, &self.colors);
        (n0, n1)
    }
}

pub struct MousePanel {
    dev: RyDevice,
    title: String,
    battery: Option<String>,
    loaded: Option<(MouseOption0, MouseOption1)>,
    form: Option<Form>,
    status: Option<Result<String, String>>,
}

impl MousePanel {
    pub fn new(dev: RyDevice) -> Self {
        let title = dev.identify().unwrap_or_else(|| dev.info.name.clone());
        let mut p = Self { dev, title, battery: None, loaded: None, form: None, status: None };
        p.reload();
        p
    }

    fn reload(&mut self) {
        self.battery = self.dev.battery().ok().map(|b| match b.state {
            ChargeState::Discharging => format!("{}%", b.percent),
            s => format!("{}% ({s:?})", b.percent),
        });
        if self.dev.info.kind == Kind::Receiver {
            self.form = None;
            self.loaded = None;
            self.status = Some(Err(
                "Wireless (2.4G) settings aren't supported yet - battery only. \
                 Connect the mouse with its USB cable to change DPI, polling rate and lighting."
                    .into(),
            ));
            return;
        }
        let read = self.dev.read_option0().and_then(|o0| Ok((o0, self.dev.read_option1()?)));
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
        ui.heading(&self.title);
        ui.horizontal(|ui| {
            ui.label(format!("Battery: {}", self.battery.as_deref().unwrap_or("unavailable")));
            if ui.button("Reload").clicked() {
                self.reload();
            }
        });
        super::status_line(ui, &self.status);
        ui.separator();

        let mut save = false;
        if let Some(form) = &mut self.form {
            dpi_section(ui, form);
            performance_section(ui, form);
            match &mut form.lighting {
                Some(l) => lighting_section(ui, l),
                None => {
                    ui.weak("Lighting: unknown format on this model (left unchanged).");
                }
            }
            ui.add_space(8.0);
            save = ui.add_sized([140.0, 32.0], egui::Button::new("💾 Apply to mouse")).clicked();
        }
        if save {
            self.save();
        }
    }
}

fn dpi_section(ui: &mut egui::Ui, f: &mut Form) {
    ui.collapsing("DPI", |ui| {
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
    })
    .header_response
    .on_hover_text("Selected stage is the active DPI");
}

fn performance_section(ui: &mut egui::Ui, f: &mut Form) {
    ui.collapsing("Performance", |ui| {
        let shown = f.rate.map_or("Unknown (unchanged)".to_string(), |hz| format!("{hz} Hz"));
        egui::ComboBox::from_label("Polling rate").selected_text(shown).show_ui(ui, |ui| {
            for hz in POLL_RATES {
                ui.selectable_value(&mut f.rate, Some(hz), format!("{hz} Hz"));
            }
        });
        ui.add(egui::Slider::new(&mut f.debounce, 0..=30).clamping(egui::SliderClamping::Never).text("Debounce (ms)"));
        ui.add(egui::Slider::new(&mut f.lift_cutoff, 0..=2).clamping(egui::SliderClamping::Never).text("Lift-off distance (0 low - 2 high)"));
        ui.checkbox(&mut f.motion_sync, "Motion sync");
        ui.checkbox(&mut f.angle_snap, "Angle snapping");
        ui.checkbox(&mut f.power_save, "Power-saving mode");
        ui.add(egui::Slider::new(&mut f.sleep_secs, 0..=1800).clamping(egui::SliderClamping::Never).text("Sleep after idle (s, 0 = never)"));
    })
    .header_response
    .on_hover_text("Polling rates above 1000 Hz need the 8K receiver");
}

pub(crate) fn lighting_section(ui: &mut egui::Ui, l: &mut Lighting) {
    ui.collapsing("Lighting", |ui| {
        egui::ComboBox::from_label("Effect").selected_text(l.effect.label()).show_ui(ui, |ui| {
            for e in LightEffect::ALL {
                ui.selectable_value(&mut l.effect, e, e.label());
            }
        });
        if l.effect != LightEffect::Off {
            ui.add(egui::Slider::new(&mut l.brightness, 0..=4).text("Brightness"));
            ui.add(egui::Slider::new(&mut l.speed, 0..=4).text("Speed"));
            ui.checkbox(&mut l.rainbow, "Rainbow colours");
            if !l.rainbow {
                ui.horizontal(|ui| {
                    ui.label("Colour");
                    egui::color_picker::color_edit_button_srgb(ui, &mut l.rgb);
                });
            }
        }
    });
}
