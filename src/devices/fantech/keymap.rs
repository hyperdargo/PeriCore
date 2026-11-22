//! MAXFIT61 onboard key remapping (opcode 18) and per-key RGB (opcode 09).
//! Both address keys by the 6x22 matrix index from [`super::layout`].

use super::layout::{self, CELLS};
use super::protocol::{LEN, REPORT_ID};
use serde::{Deserialize, Serialize};

pub const OP_KEYMAP: u8 = 0x18;
pub const OP_CUSTOM_RGB: u8 = 0x09;
/// ledType for the per-key "custom" effect (vendor app effect group ID 11;
/// 0x0A is "Reactive", which the MAXFIT61 doesn't support).
pub const CUSTOM_EFFECT: u8 = 0x0B;

const SLOTS_PER_PAGE: usize = 14;
const CELLS_PER_RGB_PAGE: usize = 18;

/// What one physical key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyAction {
    /// Sends a HID keyboard usage.
    Key(u8),
    /// Vendor Fn layer key.
    Fn,
    Disabled,
    /// Up to two modifiers (HID usages E0..E7) plus a key.
    Combo { mods: [u8; 2], key: u8 },
    /// Plays onboard macro slot (0..9).
    Macro(u8),
}

impl KeyAction {
    /// Wire record; `source` is the physical key's own usage (macro records carry it).
    pub fn record(self, source: u8) -> [u8; 4] {
        match self {
            Self::Macro(slot) => [0x10, source, slot, 0],
            Self::Key(u) => [0x00, u, 0, 0],
            Self::Fn => [0x01, 0, 0, 0],
            Self::Disabled => [0x14, 0, 0, 0],
            Self::Combo { mods: [m1, 0], key } => [0x13, m1, key, 0],
            Self::Combo { mods: [m1, m2], key } => [0x13, m1, m2, key],
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Key(u) => layout::usage_name(u),
            Self::Fn => "Fn".into(),
            Self::Disabled => "Disabled".into(),
            Self::Macro(slot) => {
                let lib = crate::macros::library();
                lib.get(slot as usize).map_or_else(|| format!("Macro {}", slot + 1), |m| format!("⚡{}", m.name))
            }
            Self::Combo { mods, key } => {
                let mut parts: Vec<String> = mods.iter().filter(|&&m| m != 0).map(|&m| mod_short(m)).collect();
                parts.push(layout::usage_name(key));
                parts.join("+")
            }
        }
    }
}

fn mod_short(usage: u8) -> String {
    match usage {
        0xE0 | 0xE4 => "Ctrl".into(),
        0xE1 | 0xE5 => "Shift".into(),
        0xE2 | 0xE6 => "Alt".into(),
        0xE3 | 0xE7 => "Win".into(),
        u => layout::usage_name(u),
    }
}

/// Default-layer assignment for every physical key (factory QWERTY).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keymap {
    /// (matrix index, action) for each physical key.
    pub keys: Vec<(usize, KeyAction)>,
}

impl Default for Keymap {
    fn default() -> Self {
        let keys = layout::all_keys()
            .map(|k| (k.index, if k.usage == 0 { KeyAction::Fn } else { KeyAction::Key(k.usage) }))
            .collect();
        Self { keys }
    }
}

impl Keymap {
    pub fn get(&self, index: usize) -> Option<KeyAction> {
        self.keys.iter().find(|(i, _)| *i == index).map(|(_, a)| *a)
    }

    pub fn set(&mut self, index: usize, action: KeyAction) {
        if let Some(slot) = self.keys.iter_mut().find(|(i, _)| *i == index) {
            slot.1 = action;
        }
    }

    pub fn is_default(&self, index: usize) -> bool {
        Keymap::default().get(index) == self.get(index)
    }

    /// The ten 18-reports for the Default layer (layer selector 0).
    pub fn pages(&self) -> Vec<[u8; LEN]> {
        let mut table = vec![[0u8; 4]; CELLS];
        for &(i, a) in &self.keys {
            if i < CELLS {
                let source = layout::key_at(i).map_or(0, |k| k.usage);
                table[i] = a.record(source);
            }
        }
        table
            .chunks(SLOTS_PER_PAGE)
            .enumerate()
            .map(|(page, slots)| {
                let body: Vec<u8> = slots.iter().flatten().copied().collect();
                frame(OP_KEYMAP, 0, page as u8, &body)
            })
            .collect()
    }
}

/// The eight 09-reports carrying a full RGB matrix (row-major, holes black).
pub fn rgb_pages(selector: u8, colors: &[[u8; 3]; CELLS]) -> Vec<[u8; LEN]> {
    colors
        .chunks(CELLS_PER_RGB_PAGE)
        .enumerate()
        .map(|(page, cells)| {
            let body: Vec<u8> = cells.iter().flatten().copied().collect();
            frame(OP_CUSTOM_RGB, selector, page as u8, &body)
        })
        .collect()
}

/// `[01, op, sel, 00, page, len, body…]` zero-padded to 64 bytes.
fn frame(op: u8, selector: u8, page: u8, body: &[u8]) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = REPORT_ID;
    p[1] = op;
    p[2] = selector;
    p[4] = page;
    p[5] = body.len() as u8;
    p[6..6 + body.len()].copy_from_slice(body);
    p
}

/// Opcode 17: host colour streaming on (true) / off (false).
pub fn light_sync(on: bool) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = REPORT_ID;
    p[1] = 0x17;
    p[5] = 0x01;
    p[6] = u8::from(on);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn default_page_matches_spec() {
        let pages = Keymap::default().pages();
        assert_eq!(pages.len(), 10);
        let p1 = hex(
            "01 18 00 00 01 38 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00
             00 00 00 00 00 00 00 29 00 00 00 1E 00 00 00 1F 00 00 00 20 00 00 00 21 00 00 00 22 00 00 00 00",
        );
        assert_eq!(pages[1].to_vec(), p1);
        assert_eq!(pages[9][5], 0x18);
        // Fn at index 121 -> page 8, slot 9.
        assert_eq!(&pages[8][6 + 36..6 + 40], &[0x01, 0, 0, 0]);
    }

    #[test]
    fn caps_to_ctrl_matches_spec() {
        let mut km = Keymap::default();
        km.set(66, KeyAction::Key(0xE0));
        let p = km.pages()[4];
        assert_eq!(&p[46..50], &[0x00, 0xE0, 0, 0]);
        assert!(!km.is_default(66));
    }

    #[test]
    fn combo_records() {
        assert_eq!(KeyAction::Combo { mods: [0xE0, 0], key: 0x04 }.record(0), [0x13, 0xE0, 0x04, 0]);
        assert_eq!(KeyAction::Combo { mods: [0xE0, 0xE1], key: 0x04 }.record(0), [0x13, 0xE0, 0xE1, 0x04]);
        assert_eq!(KeyAction::Combo { mods: [0xE0, 0xE1], key: 0x04 }.label(), "Ctrl+Shift+A");
    }

    #[test]
    fn macro_record_carries_source_key() {
        assert_eq!(KeyAction::Macro(0).record(0x39), [0x10, 0x39, 0x00, 0]);
        let mut km = Keymap::default();
        km.set(66, KeyAction::Macro(2));
        assert_eq!(&km.pages()[4][46..50], &[0x10, 0x39, 0x02, 0]);
    }

    #[test]
    fn esc_blue_rgb_page() {
        let mut c = [[0u8; 3]; CELLS];
        c[22] = [0, 0, 0xFF];
        let pages = rgb_pages(0, &c);
        assert_eq!(pages.len(), 8);
        assert_eq!(pages[1][20], 0xFF);
        assert_eq!(pages[7][5], 0x12);
        assert_eq!(pages[0][5], 0x36);
    }
}
