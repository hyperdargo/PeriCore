//! MAXFIT61 physical layout. Each key sits in the firmware's 6x22 matrix at
//! `row * 22 + column`; keymap (18) and per-key RGB (09) both use that index.

pub const COLS: usize = 22;
pub const CELLS: usize = 6 * COLS;

/// One physical key: label, matrix index, default HID usage, width in units.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub label: &'static str,
    pub index: usize,
    /// HID keyboard usage; 0 marks the vendor Fn key.
    pub usage: u8,
    pub width: f32,
}

const fn k(label: &'static str, row: usize, col: usize, usage: u8, width: f32) -> Key {
    Key { label, index: row * COLS + col, usage, width }
}

/// Keys grouped by visual row, left to right.
pub const ROWS: [&[Key]; 5] = [
    &[
        k("Esc", 1, 0, 0x29, 1.0), k("1", 1, 1, 0x1E, 1.0), k("2", 1, 2, 0x1F, 1.0), k("3", 1, 3, 0x20, 1.0),
        k("4", 1, 4, 0x21, 1.0), k("5", 1, 5, 0x22, 1.0), k("6", 1, 6, 0x23, 1.0), k("7", 1, 7, 0x24, 1.0),
        k("8", 1, 8, 0x25, 1.0), k("9", 1, 9, 0x26, 1.0), k("0", 1, 10, 0x27, 1.0), k("-", 1, 11, 0x2D, 1.0),
        k("=", 1, 12, 0x2E, 1.0), k("Backspace", 1, 14, 0x2A, 2.0),
    ],
    &[
        k("Tab", 2, 0, 0x2B, 1.5), k("Q", 2, 1, 0x14, 1.0), k("W", 2, 2, 0x1A, 1.0), k("E", 2, 3, 0x08, 1.0),
        k("R", 2, 4, 0x15, 1.0), k("T", 2, 5, 0x17, 1.0), k("Y", 2, 6, 0x1C, 1.0), k("U", 2, 7, 0x18, 1.0),
        k("I", 2, 8, 0x0C, 1.0), k("O", 2, 9, 0x12, 1.0), k("P", 2, 10, 0x13, 1.0), k("[", 2, 11, 0x2F, 1.0),
        k("]", 2, 12, 0x30, 1.0), k("\\", 2, 14, 0x31, 1.5),
    ],
    &[
        k("Caps", 3, 0, 0x39, 1.75), k("A", 3, 2, 0x04, 1.0), k("S", 3, 3, 0x16, 1.0), k("D", 3, 4, 0x07, 1.0),
        k("F", 3, 5, 0x09, 1.0), k("G", 3, 6, 0x0A, 1.0), k("H", 3, 7, 0x0B, 1.0), k("J", 3, 8, 0x0D, 1.0),
        k("K", 3, 9, 0x0E, 1.0), k("L", 3, 10, 0x0F, 1.0), k(";", 3, 11, 0x33, 1.0), k("'", 3, 12, 0x34, 1.0),
        k("Enter", 3, 14, 0x28, 2.25),
    ],
    &[
        k("Shift", 4, 0, 0xE1, 2.25), k("Z", 4, 2, 0x1D, 1.0), k("X", 4, 3, 0x1B, 1.0), k("C", 4, 4, 0x06, 1.0),
        k("V", 4, 5, 0x19, 1.0), k("B", 4, 6, 0x05, 1.0), k("N", 4, 7, 0x11, 1.0), k("M", 4, 8, 0x10, 1.0),
        k(",", 4, 9, 0x36, 1.0), k(".", 4, 10, 0x37, 1.0), k("/", 4, 11, 0x38, 1.0), k("Shift", 4, 14, 0xE5, 2.75),
    ],
    &[
        k("Ctrl", 5, 0, 0xE0, 1.25), k("Win", 5, 1, 0xE3, 1.25), k("Alt", 5, 2, 0xE2, 1.25),
        k("Space", 5, 6, 0x2C, 6.25), k("Alt", 5, 10, 0xE6, 1.25), k("Fn", 5, 11, 0x00, 1.25),
        k("Menu", 5, 12, 0x65, 1.25), k("Ctrl", 5, 13, 0xE4, 1.25),
    ],
];

pub fn all_keys() -> impl Iterator<Item = &'static Key> {
    ROWS.iter().flat_map(|r| r.iter())
}

pub fn key_at(index: usize) -> Option<&'static Key> {
    all_keys().find(|k| k.index == index)
}

/// Keys a remap can target: (label, HID usage), including keys the 61-key
/// board lacks physically (F-row, arrows, navigation).
pub fn usage_choices() -> Vec<(String, u8)> {
    let mut v: Vec<(String, u8)> = Vec::new();
    for (i, c) in ('A'..='Z').enumerate() {
        v.push((c.to_string(), 0x04 + i as u8));
    }
    for (i, c) in "1234567890".chars().enumerate() {
        v.push((c.to_string(), 0x1E + i as u8));
    }
    for i in 0..12u8 {
        v.push((format!("F{}", i + 1), 0x3A + i));
    }
    let named: [(&str, u8); 33] = [
        ("Enter", 0x28), ("Esc", 0x29), ("Backspace", 0x2A), ("Tab", 0x2B), ("Space", 0x2C),
        ("-", 0x2D), ("=", 0x2E), ("[", 0x2F), ("]", 0x30), ("\\", 0x31), (";", 0x33), ("'", 0x34),
        ("`", 0x35), (",", 0x36), (".", 0x37), ("/", 0x38), ("Caps Lock", 0x39),
        ("Print Screen", 0x46), ("Scroll Lock", 0x47), ("Pause", 0x48), ("Insert", 0x49), ("Home", 0x4A),
        ("Page Up", 0x4B), ("Delete", 0x4C), ("End", 0x4D), ("Page Down", 0x4E),
        ("Right", 0x4F), ("Left", 0x50), ("Down", 0x51), ("Up", 0x52), ("Menu", 0x65),
        ("Left Ctrl", 0xE0), ("Left Shift", 0xE1),
    ];
    v.extend(named.iter().map(|&(n, u)| (n.to_string(), u)));
    let mods: [(&str, u8); 6] = [
        ("Left Alt", 0xE2), ("Left Win", 0xE3), ("Right Ctrl", 0xE4), ("Right Shift", 0xE5),
        ("Right Alt", 0xE6), ("Right Win", 0xE7),
    ];
    v.extend(mods.iter().map(|&(n, u)| (n.to_string(), u)));
    v
}

pub fn usage_name(usage: u8) -> String {
    usage_choices()
        .into_iter()
        .find(|(_, u)| *u == usage)
        .map_or_else(|| format!("0x{usage:02X}"), |(n, _)| n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_61_unique_keys_in_matrix() {
        let idx: Vec<usize> = all_keys().map(|k| k.index).collect();
        assert_eq!(idx.len(), 61);
        let mut sorted = idx.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 61);
        assert!(idx.iter().all(|&i| i < CELLS));
    }

    #[test]
    fn spec_indices() {
        assert_eq!(key_at(22).unwrap().label, "Esc");
        assert_eq!(key_at(66).unwrap().usage, 0x39);
        assert_eq!(key_at(121).unwrap().label, "Fn");
        assert_eq!(key_at(123).unwrap().usage, 0xE4);
    }
}
