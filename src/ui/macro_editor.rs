//! Macro library and editor, shared by the mouse and keyboard Macros tabs.
//! Macros are recorded from this window's keyboard input or built by hand.

use super::theme;
use crate::devices::fantech::layout;
use crate::macros::{self, Event, Input, Macro, MouseBtn, Repeat, LIBRARY_MAX};
use eframe::egui::{self, Key, RichText};

/// Which device the tab is for, to show the right size limits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Mouse,
    Keyboard,
}

pub struct MacroEditor {
    target: Target,
    selected: usize,
    recording: bool,
    last_event: f64,
    last_mods: egui::Modifiers,
    add_key: u8,
    choices: Vec<(String, u8)>,
}

impl MacroEditor {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            selected: 0,
            recording: false,
            last_event: 0.0,
            last_mods: egui::Modifiers::NONE,
            add_key: 0x04,
            choices: layout::usage_choices(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        let mut lib = macros::library().clone();
        let before = lib.clone();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                self.list(ui, &mut lib);
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                if let Some(m) = lib.get_mut(self.selected) {
                    self.editor(ui, m);
                } else {
                    theme::card(ui, "Macros", |ui| {
                        ui.label("Create a macro, then assign it to a button or key.")
                    });
                }
            });
        });
        if lib != before {
            let _ = macros::save_library(&lib);
            *macros::library() = lib;
        }
    }

    fn list(&mut self, ui: &mut egui::Ui, lib: &mut Vec<Macro>) {
        theme::card(ui, "Library", |ui| {
            for (i, m) in lib.iter().enumerate() {
                let label = format!("{}  {}", i + 1, m.name);
                if ui.selectable_label(self.selected == i, label).clicked() {
                    self.selected = i;
                    self.recording = false;
                }
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(lib.len() < LIBRARY_MAX, egui::Button::new("＋ New")).clicked() {
                    lib.push(Macro::new(&format!("Macro {}", lib.len() + 1)));
                    self.selected = lib.len() - 1;
                }
                if ui.add_enabled(self.selected < lib.len(), egui::Button::new("🗑 Delete")).clicked() {
                    lib.remove(self.selected);
                    self.selected = self.selected.saturating_sub(1);
                }
            });
            if self.target == Target::Keyboard {
                ui.label(RichText::new(format!("Keyboard can use macros 1–{}.", macros::KB_SLOTS)).size(12.0).color(theme::MUTED));
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui, m: &mut Macro) {
        if self.recording {
            self.capture(ui, m);
        }
        theme::card(ui, "Macro", |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut m.name);
            });
            ui.horizontal(|ui| {
                ui.label("Playback");
                let mut kind = match m.repeat {
                    Repeat::Times(_) => 0,
                    Repeat::Toggle => 1,
                    Repeat::WhileHeld => 2,
                };
                ui.selectable_value(&mut kind, 0, "Repeat N times");
                ui.selectable_value(&mut kind, 1, "Toggle on/off");
                ui.selectable_value(&mut kind, 2, "While held");
                m.repeat = match (kind, m.repeat) {
                    (0, Repeat::Times(n)) => Repeat::Times(n),
                    (0, _) => Repeat::Times(1),
                    (1, _) => Repeat::Toggle,
                    _ => Repeat::WhileHeld,
                };
                if let Repeat::Times(n) = &mut m.repeat {
                    ui.add(egui::DragValue::new(n).range(1..=255).suffix("×"));
                }
            });
            let (fits, usage) = match self.target {
                Target::Mouse => (m.fits_ry(), format!("{} / {} bytes", m.ry_len(), macros::RY_MAX_BYTES)),
                Target::Keyboard => (m.fits_kb(), format!("{} / {} events", m.events.len(), macros::KB_MAX_EVENTS)),
            };
            ui.colored_label(if fits { theme::MUTED } else { theme::BAD }, format!("Size: {usage}"));
        });

        theme::card(ui, "Events", |ui| {
            ui.horizontal(|ui| {
                let rec = if self.recording { "⏹  Stop recording" } else { "⏺  Record" };
                if ui.add(egui::Button::new(RichText::new(rec).color(if self.recording { theme::BAD } else { theme::TEXT }))).clicked() {
                    self.recording = !self.recording;
                    self.last_event = ui.input(|i| i.time);
                    self.last_mods = ui.input(|i| i.modifiers);
                }
                if ui.button("Clear").clicked() {
                    m.events.clear();
                }
            });
            if self.recording {
                ui.colored_label(theme::WARN, "Recording keys typed in this window… (Win key can't be captured)");
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("addkey").selected_text(layout::usage_name(self.add_key)).height(300.0).show_ui(ui, |ui| {
                    for (n, c) in &self.choices {
                        ui.selectable_value(&mut self.add_key, *c, n);
                    }
                });
                if ui.button("＋ Key press").clicked() {
                    tap(m, Input::Key(self.add_key));
                }
                for (label, b) in [("＋ Left click", MouseBtn::Left), ("＋ Right click", MouseBtn::Right)] {
                    if ui.button(label).clicked() {
                        tap(m, Input::Mouse(b));
                    }
                }
            });
            ui.add_space(4.0);
            event_table(ui, m);
        });
    }

    /// Turns this frame's key events into macro events with real delays.
    fn capture(&mut self, ui: &mut egui::Ui, m: &mut Macro) {
        let (events, mods, now) = ui.input(|i| (i.events.clone(), i.modifiers, i.time));
        let mut push = |input: Input, down: bool, this: &mut Self| {
            if let Some(prev) = m.events.last_mut() {
                prev.delay_ms = ((now - this.last_event) * 1000.0).clamp(1.0, 65535.0) as u16;
            }
            m.events.push(Event { input, down, delay_ms: 0 });
            this.last_event = now;
        };
        for (flag, usage) in [(mods.ctrl, 0xE0), (mods.shift, 0xE1), (mods.alt, 0xE2)] {
            let was = match usage {
                0xE0 => self.last_mods.ctrl,
                0xE1 => self.last_mods.shift,
                _ => self.last_mods.alt,
            };
            if flag != was {
                push(Input::Key(usage), flag, self);
            }
        }
        self.last_mods = mods;
        for e in events {
            if let egui::Event::Key { key, pressed, repeat: false, .. } = e {
                if let Some(u) = egui_key_to_usage(key) {
                    push(Input::Key(u), pressed, self);
                }
            }
        }
        ui.ctx().request_repaint();
    }
}

/// Adds a down/up pair with short default delays.
fn tap(m: &mut Macro, input: Input) {
    m.events.push(Event { input, down: true, delay_ms: 30 });
    m.events.push(Event { input, down: false, delay_ms: 30 });
}

fn event_table(ui: &mut egui::Ui, m: &mut Macro) {
    if m.events.is_empty() {
        ui.label(RichText::new("No events yet.").color(theme::MUTED));
        return;
    }
    let mut remove = None;
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        egui::Grid::new("events").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for (i, e) in m.events.iter_mut().enumerate() {
                ui.label(RichText::new(if e.down { "▼ press" } else { "▲ release" }).color(if e.down { theme::ACCENT } else { theme::MUTED }));
                ui.label(macros::input_label(e.input));
                ui.add(egui::DragValue::new(&mut e.delay_ms).range(0..=65535).suffix(" ms"));
                if ui.small_button("✕").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some(i) = remove {
        m.events.remove(i);
    }
}

/// egui key to HID keyboard usage.
fn egui_key_to_usage(k: Key) -> Option<u8> {
    use Key::*;
    let letters = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    if let Some(i) = letters.iter().position(|&l| l == k) {
        return Some(0x04 + i as u8);
    }
    let digits = [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0];
    if let Some(i) = digits.iter().position(|&d| d == k) {
        return Some(0x1E + i as u8);
    }
    let fkeys = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    if let Some(i) = fkeys.iter().position(|&f| f == k) {
        return Some(0x3A + i as u8);
    }
    Some(match k {
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equals => 0x2E,
        OpenBracket => 0x2F,
        CloseBracket => 0x30,
        Backslash => 0x31,
        Semicolon => 0x33,
        Quote => 0x34,
        Backtick => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash => 0x38,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_mapping() {
        assert_eq!(egui_key_to_usage(Key::A), Some(0x04));
        assert_eq!(egui_key_to_usage(Key::Num0), Some(0x27));
        assert_eq!(egui_key_to_usage(Key::F12), Some(0x45));
        assert_eq!(egui_key_to_usage(Key::ArrowUp), Some(0x52));
    }
}
