//! RY mouse button table: GET `D0 [profile]` returns 16 four-byte action
//! slots; SET `50 [profile] [slot] … ck` writes one slot (action at bytes 8..12).

pub const GET_KEYS: u8 = 0xD0;
pub const SET_KEY: u8 = 0x50;
pub const SLOTS: usize = 16;

/// Remappable physical buttons on the AJ159/AJ179: (slot, label).
pub const BUTTONS: [(usize, &str); 7] = [
    (0, "Left button"),
    (1, "Right button"),
    (2, "Middle button"),
    (4, "Back (side 1)"),
    (3, "Forward (side 2)"),
    (14, "Wheel up"),
    (15, "Wheel down"),
];

/// Mouse codes used by action type 01.
pub mod code {
    pub const LEFT: u8 = 0xF0;
    pub const RIGHT: u8 = 0xF1;
    pub const MIDDLE: u8 = 0xF2;
    pub const BACK: u8 = 0xF3;
    pub const FORWARD: u8 = 0xF4;
    pub const WHEEL: u8 = 0xF5;
}

pub const MEDIA: [(&str, u16); 7] = [
    ("Play / Pause", 0xCD),
    ("Next track", 0xB5),
    ("Previous track", 0xB6),
    ("Stop", 0xB7),
    ("Mute", 0xE2),
    ("Volume up", 0xE9),
    ("Volume down", 0xEA),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiOp {
    Loop,
    Up,
    Down,
    Sniper,
}

impl DpiOp {
    pub const ALL: [DpiOp; 4] = [DpiOp::Loop, DpiOp::Up, DpiOp::Down, DpiOp::Sniper];
    fn code(self) -> u8 {
        match self {
            Self::Loop => 0,
            Self::Up => 1,
            Self::Down => 2,
            Self::Sniper => 4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Loop => "DPI cycle",
            Self::Up => "DPI up",
            Self::Down => "DPI down",
            Self::Sniper => "Sniper (hold for low DPI)",
        }
    }
}

/// What one button slot does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    Disabled,
    /// Mouse code (F0..F4) or wheel (F5 with +1/-1).
    Mouse { code: u8, param: u8 },
    /// Keyboard key with an optional modifier (HID usage E0..E7, or 0).
    Key { modifier: u8, key: u8 },
    Media(u16),
    Dpi(DpiOp),
    /// Rapid fire: repeats `code` every `interval_ms`, `count` times.
    Fire { code: u8, interval_ms: u8, count: u8 },
    /// Plays onboard macro `index`; mode 0 = repeat (count in macro), 1 = toggle, 2 = while held.
    Macro { mode: u8, index: u8 },
    /// Anything we don't model; written back unchanged.
    Raw([u8; 4]),
}

impl ButtonAction {
    pub fn encode(self) -> [u8; 4] {
        match self {
            Self::Disabled => [0, 0, 0, 0],
            Self::Mouse { code, param } => [0x01, 0, code, param],
            Self::Key { modifier, key } => [0x00, modifier, key, 0],
            Self::Media(c) => [0x03, 0, c as u8, (c >> 8) as u8],
            Self::Dpi(op) => [0x14, 0, op.code(), 0],
            Self::Fire { code, interval_ms, count } => [0x0B, code, interval_ms, count],
            Self::Macro { mode, index } => [0x09, mode, index, 0],
            Self::Raw(r) => r,
        }
    }

    pub fn decode(r: [u8; 4]) -> Self {
        match r {
            [0, 0, 0, 0] => Self::Disabled,
            [0x00, m, k, 0] if k != 0 => Self::Key { modifier: m, key: k },
            [0x01, 0, c, p] => Self::Mouse { code: c, param: p },
            [0x03, 0, lo, hi] => Self::Media(u16::from_le_bytes([lo, hi])),
            [0x14, 0, op, 0] => match op {
                0 => Self::Dpi(DpiOp::Loop),
                1 => Self::Dpi(DpiOp::Up),
                2 => Self::Dpi(DpiOp::Down),
                4 => Self::Dpi(DpiOp::Sniper),
                _ => Self::Raw(r),
            },
            [0x0B, c, i, n] => Self::Fire { code: c, interval_ms: i, count: n },
            [0x09, m, i, 0] => Self::Macro { mode: m, index: i },
            _ => Self::Raw(r),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Disabled => "Disabled".into(),
            Self::Mouse { code, param } => mouse_label(code, param).into(),
            Self::Key { modifier, key } => {
                let k = crate::devices::fantech::layout::usage_name(key);
                match modifier {
                    0 => k,
                    0xE0 | 0xE4 => format!("Ctrl+{k}"),
                    0xE1 | 0xE5 => format!("Shift+{k}"),
                    0xE2 | 0xE6 => format!("Alt+{k}"),
                    _ => format!("Win+{k}"),
                }
            }
            Self::Media(c) => MEDIA.iter().find(|m| m.1 == c).map_or_else(|| format!("Media 0x{c:03X}"), |m| m.0.into()),
            Self::Dpi(op) => op.label().into(),
            Self::Fire { interval_ms, count, .. } => format!("Rapid fire ({count}× every {interval_ms} ms)"),
            Self::Macro { index, .. } => {
                let lib = crate::macros::library();
                lib.get(index as usize).map_or_else(|| format!("Macro {}", index + 1), |m| format!("Macro: {}", m.name))
            }
            Self::Raw(r) => format!("Custom {:02X} {:02X} {:02X} {:02X}", r[0], r[1], r[2], r[3]),
        }
    }

    pub fn is_left_click(self) -> bool {
        matches!(self, Self::Mouse { code: code::LEFT, .. })
    }
}

pub fn mouse_label(code: u8, param: u8) -> &'static str {
    match (code, param) {
        (code::LEFT, _) => "Left click",
        (code::RIGHT, _) => "Right click",
        (code::MIDDLE, _) => "Middle click",
        (code::BACK, _) => "Back",
        (code::FORWARD, _) => "Forward",
        (code::WHEEL, 0x01) => "Scroll up",
        (code::WHEEL, _) => "Scroll down",
        _ => "Mouse action",
    }
}

/// Factory table for the AJ159/AJ179 APEX.
pub fn defaults() -> [[u8; 4]; SLOTS] {
    let mut t = [[0u8; 4]; SLOTS];
    t[0] = [1, 0, code::LEFT, 0];
    t[1] = [1, 0, code::RIGHT, 0];
    t[2] = [1, 0, code::MIDDLE, 0];
    t[3] = [1, 0, code::FORWARD, 0];
    t[4] = [1, 0, code::BACK, 0];
    t[5] = [0x14, 0, 0, 0];
    t[12] = [1, 0, 0xF9, 0x01];
    t[13] = [1, 0, 0xF9, 0xFF];
    t[14] = [1, 0, code::WHEEL, 0x01];
    t[15] = [1, 0, code::WHEEL, 0xFF];
    t
}

pub fn parse_table(reply: &[u8; 64]) -> [[u8; 4]; SLOTS] {
    let mut t = [[0u8; 4]; SLOTS];
    for (i, slot) in t.iter_mut().enumerate() {
        slot.copy_from_slice(&reply[i * 4..i * 4 + 4]);
    }
    t
}

/// SET payload (checksum applied by the channel at byte 7).
pub fn set_payload(profile: u8, slot: usize, action: ButtonAction) -> Vec<u8> {
    let mut p = vec![SET_KEY, profile, slot as u8, 0, 0, 0, 0, 0];
    p.extend_from_slice(&action.encode());
    p
}

/// True if, after changing `slot` to `action`, some button still left-clicks.
pub fn keeps_left_click(table: &[[u8; 4]; SLOTS], slot: usize, action: ButtonAction) -> bool {
    BUTTONS.iter().any(|&(s, _)| {
        let a = if s == slot { action } else { ButtonAction::decode(table[s]) };
        a.is_left_click()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Checksum;

    #[test]
    fn spec_example_side1_to_a() {
        let p = Checksum::Bit7.apply(&set_payload(0, 4, ButtonAction::Key { modifier: 0, key: 0x04 }));
        assert_eq!(&p[..12], &[0x50, 0, 4, 0, 0, 0, 0, 0xAB, 0, 0, 0x04, 0]);
    }

    #[test]
    fn spec_example_restore_and_macro_slot() {
        let p = Checksum::Bit7.apply(&set_payload(0, 4, ButtonAction::decode(defaults()[4])));
        assert_eq!(&p[..12], &[0x50, 0, 4, 0, 0, 0, 0, 0xAB, 0x01, 0, 0xF3, 0]);
    }

    #[test]
    fn decode_roundtrip_defaults() {
        for r in defaults() {
            assert_eq!(ButtonAction::decode(r).encode(), r);
        }
        assert_eq!(ButtonAction::decode([0x03, 0, 0xCD, 0]), ButtonAction::Media(0xCD));
        assert_eq!(ButtonAction::decode([0x14, 0, 0, 0]).label(), "DPI cycle");
    }

    #[test]
    fn left_click_guard() {
        let t = defaults();
        assert!(!keeps_left_click(&t, 0, ButtonAction::Disabled));
        let mut t2 = t;
        t2[4] = [1, 0, code::LEFT, 0];
        assert!(keeps_left_click(&t2, 0, ButtonAction::Disabled));
    }
}
