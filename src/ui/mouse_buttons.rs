//! Buttons tab: per-button remapping for RY mice (written one slot at a time).

use super::theme;
use crate::devices::fantech::layout;
use crate::devices::ry::buttons::{self, code, ButtonAction, DpiOp, MEDIA, SLOTS};
use crate::devices::ry::RyDevice;
use eframe::egui::{self, RichText};

type Table = [[u8; 4]; SLOTS];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mouse,
    Key,
    Media,
    Dpi,
    Fire,
    Macro,
    Disabled,
}

const MOUSE_ACTIONS: [(u8, u8); 7] = [
    (code::LEFT, 0), (code::RIGHT, 0), (code::MIDDLE, 0), (code::BACK, 0), (code::FORWARD, 0),
    (code::WHEEL, 0x01), (code::WHEEL, 0xFF),
];
const MODS: [(&str, u8); 5] = [("None", 0), ("Ctrl", 0xE0), ("Shift", 0xE1), ("Alt", 0xE2), ("Win", 0xE3)];

pub struct ButtonsTab {
    pub table: Option<Table>,
    selected: Option<usize>,
    draft: ButtonAction,
    choices: Vec<(String, u8)>,
}

impl ButtonsTab {
    pub fn new() -> Self {
        Self { table: None, selected: None, draft: ButtonAction::Disabled, choices: layout::usage_choices() }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, dev: &RyDevice) -> Option<Result<String, String>> {
        let Some(table) = self.table else {
            theme::card(ui, "Buttons", |ui| ui.label("Couldn't read the button table. Wake the mouse and press Reload."));
            return None;
        };
        let mut status = None;
        theme::card(ui, "Buttons", |ui| {
            egui::Grid::new("btns").num_columns(3).spacing([18.0, 8.0]).show(ui, |ui| {
                for (slot, name) in buttons::BUTTONS {
                    let action = ButtonAction::decode(table[slot]);
                    let changed = table[slot] != buttons::defaults()[slot];
                    ui.label(RichText::new(name).strong());
                    let text = RichText::new(action.label()).color(if changed { theme::ACCENT } else { theme::TEXT });
                    ui.label(text);
                    if ui.selectable_label(self.selected == Some(slot), "Edit").clicked() {
                        self.selected = Some(slot);
                        self.draft = action;
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(slot) = self.selected {
            status = self.editor(ui, dev, slot, &table);
        }
        status
    }

    fn editor(&mut self, ui: &mut egui::Ui, dev: &RyDevice, slot: usize, table: &Table) -> Option<Result<String, String>> {
        let name = buttons::BUTTONS.iter().find(|b| b.0 == slot).map_or("Button", |b| b.1);
        let mut result = None;
        theme::card(ui, &format!("Assign: {name}"), |ui| {
            let mut kind = kind_of(self.draft);
            ui.horizontal_wrapped(|ui| {
                for (k, label) in [
                    (Kind::Mouse, "Mouse"), (Kind::Key, "Keyboard"), (Kind::Media, "Media"),
                    (Kind::Dpi, "DPI"), (Kind::Fire, "Rapid fire"), (Kind::Macro, "Macro"), (Kind::Disabled, "Disabled"),
                ] {
                    ui.selectable_value(&mut kind, k, label);
                }
            });
            if kind != kind_of(self.draft) {
                self.draft = default_for(kind);
            }
            ui.add_space(4.0);
            self.draft_details(ui);

            let ok = buttons::keeps_left_click(table, slot, self.draft);
            if !ok {
                ui.colored_label(theme::WARN, "At least one button must stay a left click.");
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(ok, egui::Button::new(RichText::new("💾  Apply").strong())).clicked() {
                    result = Some(self.write(dev, slot, self.draft));
                }
                if ui.button("Restore default").clicked() {
                    let def = ButtonAction::decode(buttons::defaults()[slot]);
                    self.draft = def;
                    result = Some(self.write(dev, slot, def));
                }
            });
        });
        result
    }

    fn write(&mut self, dev: &RyDevice, slot: usize, action: ButtonAction) -> Result<String, String> {
        let action = match action {
            ButtonAction::Macro { index, .. } => {
                let m = crate::macros::library().get(index as usize).cloned().ok_or("Pick a macro first")?;
                dev.write_macro(index, &m).map_err(|e| format!("Macro upload failed: {e:#}"))?;
                let [_, mode, ..] = crate::macros::ry_binding(index, m.repeat);
                ButtonAction::Macro { mode, index }
            }
            a => a,
        };
        dev.write_button(slot, action).map_err(|e| format!("Write failed: {e:#}"))?;
        if let Some(t) = &mut self.table {
            t[slot] = action.encode();
        }
        Ok(format!("Button set to {}", action.label()))
    }

    fn draft_details(&mut self, ui: &mut egui::Ui) {
        match &mut self.draft {
            ButtonAction::Mouse { code, param } => {
                let mut cur = (*code, *param);
                egui::ComboBox::from_label("Action").selected_text(buttons::mouse_label(cur.0, cur.1)).show_ui(ui, |ui| {
                    for a in MOUSE_ACTIONS {
                        ui.selectable_value(&mut cur, a, buttons::mouse_label(a.0, a.1));
                    }
                });
                (*code, *param) = cur;
            }
            ButtonAction::Key { modifier, key } => {
                ui.horizontal(|ui| {
                    for (n, m) in MODS {
                        ui.radio_value(modifier, m, n);
                    }
                });
                egui::ComboBox::from_label("Key").selected_text(layout::usage_name(*key)).height(320.0).show_ui(ui, |ui| {
                    for (n, c) in &self.choices {
                        ui.selectable_value(key, *c, n);
                    }
                });
            }
            ButtonAction::Media(c) => {
                let label = MEDIA.iter().find(|m| m.1 == *c).map_or("?", |m| m.0);
                egui::ComboBox::from_label("Media key").selected_text(label).show_ui(ui, |ui| {
                    for (n, v) in MEDIA {
                        ui.selectable_value(c, v, n);
                    }
                });
            }
            ButtonAction::Dpi(op) => {
                for o in DpiOp::ALL {
                    ui.radio_value(op, o, o.label());
                }
            }
            ButtonAction::Fire { code, interval_ms, count } => {
                let mut cur = (*code, 0);
                egui::ComboBox::from_label("Repeats").selected_text(buttons::mouse_label(cur.0, 0)).show_ui(ui, |ui| {
                    for a in &MOUSE_ACTIONS[..5] {
                        ui.selectable_value(&mut cur, *a, buttons::mouse_label(a.0, a.1));
                    }
                });
                *code = cur.0;
                ui.add(egui::Slider::new(interval_ms, 1..=255).text("Interval (ms)"));
                ui.add(egui::Slider::new(count, 1..=255).text("Clicks per press"));
            }
            ButtonAction::Macro { index, .. } => {
                let lib = crate::macros::library();
                if lib.is_empty() {
                    ui.colored_label(theme::WARN, "No macros yet. Create one in the Macros tab.");
                } else {
                    let cur = lib.get(*index as usize).map_or("Choose…".to_string(), |m| m.name.clone());
                    egui::ComboBox::from_label("Macro").selected_text(cur).show_ui(ui, |ui| {
                        for (i, m) in lib.iter().enumerate() {
                            ui.selectable_value(index, i as u8, &m.name);
                        }
                    });
                    ui.label(RichText::new("Playback mode comes from the macro's settings.").color(theme::MUTED));
                }
            }
            ButtonAction::Disabled => {
                ui.label(RichText::new("This button will do nothing.").color(theme::MUTED));
            }
            ButtonAction::Raw(_) => {
                ui.label(RichText::new("Set by another app; pick a type above to change it.").color(theme::MUTED));
            }
        }
    }
}

fn kind_of(a: ButtonAction) -> Kind {
    match a {
        ButtonAction::Mouse { .. } | ButtonAction::Raw(_) => Kind::Mouse,
        ButtonAction::Key { .. } => Kind::Key,
        ButtonAction::Media(_) => Kind::Media,
        ButtonAction::Dpi(_) => Kind::Dpi,
        ButtonAction::Fire { .. } => Kind::Fire,
        ButtonAction::Macro { .. } => Kind::Macro,
        ButtonAction::Disabled => Kind::Disabled,
    }
}

fn default_for(k: Kind) -> ButtonAction {
    match k {
        Kind::Mouse => ButtonAction::Mouse { code: code::LEFT, param: 0 },
        Kind::Key => ButtonAction::Key { modifier: 0, key: 0x04 },
        Kind::Media => ButtonAction::Media(0xCD),
        Kind::Dpi => ButtonAction::Dpi(DpiOp::Loop),
        Kind::Fire => ButtonAction::Fire { code: code::LEFT, interval_ms: 50, count: 3 },
        Kind::Macro => ButtonAction::Macro { mode: 0, index: 0 },
        Kind::Disabled => ButtonAction::Disabled,
    }
}
