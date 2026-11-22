//! Keys tab: onboard key remapping for the MAXFIT61 (Default layer).

use super::kb_visual::{self, Cap};
use super::theme;
use crate::devices::fantech::keymap::{KeyAction, Keymap};
use crate::devices::fantech::layout;
use crate::devices::fantech::Maxfit61;
use crate::store;
use eframe::egui;

const STORE: &str = "maxfit61-keymap";
const MODS: [(&str, u8); 4] = [("Ctrl", 0xE0), ("Shift", 0xE1), ("Alt", 0xE2), ("Win", 0xE3)];

pub struct KeysTab {
    keymap: Keymap,
    selected: Option<usize>,
    choices: Vec<(String, u8)>,
}

impl KeysTab {
    pub fn new() -> Self {
        Self {
            keymap: store::load(STORE).unwrap_or_default(),
            selected: None,
            choices: layout::usage_choices(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, dev: &Maxfit61) -> Option<Result<String, String>> {
        let mut status = None;
        theme::card(ui, "Keyboard", |ui| {
            let km = &self.keymap;
            let sel = self.selected;
            let hit = kb_visual::keyboard(
                ui,
                |k| {
                    let action = km.get(k.index);
                    let changed = !km.is_default(k.index);
                    Cap {
                        top: k.label.to_string(),
                        bottom: changed.then(|| action.map_or_else(String::new, KeyAction::label)),
                        fill: if changed { theme::ACCENT2.linear_multiply(0.35) } else { theme::CARD.gamma_multiply(1.4) },
                        selected: sel == Some(k.index),
                    }
                },
                false,
            );
            if hit.is_some() {
                self.selected = hit;
            }
            kb_visual::small_note(ui, "Click a key to change what it does. Remapped keys show their new action in cyan.");
        });

        if let Some(idx) = self.selected {
            self.editor(ui, idx);
        }

        ui.horizontal(|ui| {
            if theme::primary(ui, "💾  Apply to keyboard") {
                status = Some(match upload_macros(dev, &self.keymap).and_then(|_| dev.write_keymap(&self.keymap)) {
                    Ok(()) => {
                        let _ = store::save(STORE, &self.keymap);
                        Ok("Key assignments saved to the keyboard".into())
                    }
                    Err(e) => Err(format!("Write failed: {e:#}")),
                });
            }
            if ui.add_sized([170.0, 34.0], egui::Button::new("↺  Reset all keys")).clicked() {
                self.keymap = Keymap::default();
                self.selected = None;
            }
        });
        status
    }

    fn editor(&mut self, ui: &mut egui::Ui, idx: usize) {
        let Some(key) = layout::key_at(idx) else { return };
        let Some(mut action) = self.keymap.get(idx) else { return };
        let default = Keymap::default().get(idx).unwrap_or(KeyAction::Disabled);
        theme::card(ui, &format!("Key: {}", key.label), |ui| {
            #[derive(PartialEq, Clone, Copy)]
            enum Kind {
                Key,
                Combo,
                Disabled,
                Fn,
                Macro,
            }
            let mut kind = match action {
                KeyAction::Key(_) => Kind::Key,
                KeyAction::Combo { .. } => Kind::Combo,
                KeyAction::Disabled => Kind::Disabled,
                KeyAction::Fn => Kind::Fn,
                KeyAction::Macro(_) => Kind::Macro,
            };
            ui.horizontal(|ui| {
                ui.selectable_value(&mut kind, Kind::Key, "Key");
                ui.selectable_value(&mut kind, Kind::Combo, "Shortcut");
                ui.selectable_value(&mut kind, Kind::Disabled, "Disabled");
                ui.selectable_value(&mut kind, Kind::Fn, "Fn");
                ui.selectable_value(&mut kind, Kind::Macro, "Macro");
            });
            let base_key = match action {
                KeyAction::Key(u) | KeyAction::Combo { key: u, .. } => u,
                _ => key.usage.max(0x04),
            };
            action = match kind {
                Kind::Disabled => KeyAction::Disabled,
                Kind::Fn => KeyAction::Fn,
                Kind::Macro => {
                    let mut slot = match action {
                        KeyAction::Macro(s) => s,
                        _ => 0,
                    };
                    let lib = crate::macros::library();
                    let usable = lib.len().min(crate::macros::KB_SLOTS);
                    if usable == 0 {
                        ui.colored_label(theme::WARN, "No macros yet. Create one in the Macros tab.");
                    } else {
                        let cur = lib.get(slot as usize).map_or("Choose…".to_string(), |m| m.name.clone());
                        egui::ComboBox::from_label("Macro").selected_text(cur).show_ui(ui, |ui| {
                            for (i, m) in lib.iter().take(usable).enumerate() {
                                ui.selectable_value(&mut slot, i as u8, &m.name);
                            }
                        });
                    }
                    KeyAction::Macro(slot)
                }
                Kind::Key => KeyAction::Key(self.usage_combo(ui, "Sends", base_key)),
                Kind::Combo => {
                    let mut mods = match action {
                        KeyAction::Combo { mods, .. } => mods,
                        _ => [0xE0, 0],
                    };
                    ui.horizontal(|ui| {
                        for (name, code) in MODS {
                            let mut on = mods.contains(&code);
                            let full = mods.iter().all(|&m| m != 0);
                            if ui.add_enabled(on || !full, egui::Checkbox::new(&mut on, name)).changed() {
                                mods = toggle_mod(mods, code, on);
                            }
                        }
                    });
                    let k = self.usage_combo(ui, "plus key", base_key);
                    if mods == [0, 0] {
                        KeyAction::Key(k)
                    } else {
                        KeyAction::Combo { mods, key: k }
                    }
                }
            };
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Restore default").clicked() {
                    action = default;
                }
                kb_visual::small_note(ui, &format!("Default: {}", default.label()));
            });
        });
        self.keymap.set(idx, action);
    }

    fn usage_combo(&self, ui: &mut egui::Ui, label: &str, current: u8) -> u8 {
        let mut v = current;
        egui::ComboBox::from_label(label)
            .selected_text(layout::usage_name(v))
            .height(320.0)
            .show_ui(ui, |ui| {
                for (name, code) in &self.choices {
                    ui.selectable_value(&mut v, *code, name);
                }
            });
        v
    }
}

/// Uploads every macro the keymap refers to.
fn upload_macros(dev: &Maxfit61, km: &Keymap) -> anyhow::Result<()> {
    let mut slots: Vec<u8> = km.keys.iter().filter_map(|(_, a)| match a {
        KeyAction::Macro(s) => Some(*s),
        _ => None,
    }).collect();
    slots.sort_unstable();
    slots.dedup();
    for s in slots {
        let m = crate::macros::library().get(s as usize).cloned()
            .ok_or_else(|| anyhow::anyhow!("macro {} no longer exists", s + 1))?;
        dev.write_macro(s, &m)?;
    }
    Ok(())
}

/// Adds or removes a modifier, keeping at most two (firmware limit).
fn toggle_mod(mods: [u8; 2], code: u8, on: bool) -> [u8; 2] {
    let mut list: Vec<u8> = mods.iter().copied().filter(|&m| m != 0 && m != code).collect();
    if on {
        list.push(code);
    }
    [list.first().copied().unwrap_or(0), list.get(1).copied().unwrap_or(0)]
}

#[cfg(test)]
mod tests {
    use super::toggle_mod;

    #[test]
    fn mods_toggle_and_cap_at_two() {
        assert_eq!(toggle_mod([0xE0, 0], 0xE1, true), [0xE0, 0xE1]);
        assert_eq!(toggle_mod([0xE0, 0xE1], 0xE0, false), [0xE1, 0]);
        assert_eq!(toggle_mod([0, 0], 0xE2, true), [0xE2, 0]);
    }
}
