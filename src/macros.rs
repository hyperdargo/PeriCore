//! Device-independent macros plus encoders for the two onboard formats:
//! RY mice (`16` pages, 256-byte image) and the MAXFIT61 (`19` pages).

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const STORE: &str = "macros";
/// RY editor limit (the image is 256 bytes, the vendor stops at 248).
pub const RY_MAX_BYTES: usize = 248;
/// MAXFIT61 serializer limit (41 four-byte events).
pub const KB_MAX_EVENTS: usize = 41;
/// MAXFIT61 has ten macro slots.
pub const KB_SLOTS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Input {
    /// HID keyboard usage.
    Key(u8),
    Mouse(MouseBtn),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseBtn {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub input: Input,
    pub down: bool,
    /// Pause after this event, in milliseconds.
    pub delay_ms: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    Times(u16),
    /// Press once to start, again to stop.
    Toggle,
    /// Plays while the button/key is held.
    WhileHeld,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub events: Vec<Event>,
    pub repeat: Repeat,
}

impl Macro {
    pub fn new(name: &str) -> Self {
        Self { name: name.into(), events: Vec::new(), repeat: Repeat::Times(1) }
    }

    /// Bytes this macro takes in the RY image (header + events).
    pub fn ry_len(&self) -> usize {
        2 + self.events.iter().map(|e| if (1..=127).contains(&e.delay_ms) { 2 } else { 4 }).sum::<usize>()
    }

    pub fn fits_ry(&self) -> bool {
        self.ry_len() <= RY_MAX_BYTES
    }

    pub fn fits_kb(&self) -> bool {
        self.events.len() <= KB_MAX_EVENTS
    }
}

// ---------- shared library ----------

/// RY mice allow up to 50 macro indices; the library is capped there.
pub const LIBRARY_MAX: usize = 50;

fn lib_cell() -> &'static Mutex<Vec<Macro>> {
    static LIB: OnceLock<Mutex<Vec<Macro>>> = OnceLock::new();
    LIB.get_or_init(|| Mutex::new(crate::store::load(STORE).unwrap_or_default()))
}

/// The user's macro library (shared by the mouse and keyboard panels).
/// A macro's position is its onboard slot/index on both devices.
pub fn library() -> MutexGuard<'static, Vec<Macro>> {
    lib_cell().lock().unwrap_or_else(|e| e.into_inner())
}

pub fn save_library(lib: &[Macro]) -> anyhow::Result<()> {
    crate::store::save(STORE, &lib.to_vec())
}

/// Display label for an input.
pub fn input_label(i: Input) -> String {
    match i {
        Input::Key(u) => crate::devices::fantech::layout::usage_name(u),
        Input::Mouse(MouseBtn::Left) => "Left click".into(),
        Input::Mouse(MouseBtn::Right) => "Right click".into(),
        Input::Mouse(MouseBtn::Middle) => "Middle click".into(),
    }
}

// ---------- RY mouse ----------

fn ry_code(i: Input) -> u8 {
    match i {
        Input::Key(u) => u,
        Input::Mouse(MouseBtn::Left) => 0xF0,
        Input::Mouse(MouseBtn::Right) => 0xF1,
        Input::Mouse(MouseBtn::Middle) => 0xF2,
    }
}

/// The 256-byte RY macro image.
pub fn ry_image(m: &Macro) -> [u8; 256] {
    let mut img = [0u8; 256];
    let count = match m.repeat {
        Repeat::Times(n) => n.max(1),
        _ => 1,
    };
    img[..2].copy_from_slice(&count.to_le_bytes());
    let mut pos = 2;
    for e in &m.events {
        let dir = if e.down { 0x80 } else { 0 };
        let bytes: Vec<u8> = if (1..=127).contains(&e.delay_ms) {
            vec![ry_code(e.input), dir | e.delay_ms as u8]
        } else {
            let [lo, hi] = e.delay_ms.to_le_bytes();
            vec![ry_code(e.input), dir, lo, hi]
        };
        if pos + bytes.len() > RY_MAX_BYTES {
            break;
        }
        img[pos..pos + bytes.len()].copy_from_slice(&bytes);
        pos += bytes.len();
    }
    img
}

/// RY `16` write payloads (checksum is added by the channel at byte 7).
pub fn ry_pages(index: u8, m: &Macro) -> Vec<Vec<u8>> {
    let img = ry_image(m);
    let last_nz = img.iter().rposition(|&b| b != 0).unwrap_or(0);
    let last_page = last_nz / 56;
    let total = (56 * last_page + (last_nz % 56) + 1) as u8;
    (0..=last_page)
        .map(|page| {
            let mut p = vec![0x16, index, page as u8, total, u8::from(page == last_page), 0, 0, 0];
            p.extend_from_slice(&img[56 * page..56 * page + 56]);
            p
        })
        .collect()
}

/// Button action bytes binding an RY macro.
pub fn ry_binding(index: u8, repeat: Repeat) -> [u8; 4] {
    let mode = match repeat {
        Repeat::Times(_) => 0,
        Repeat::Toggle => 1,
        Repeat::WhileHeld => 2,
    };
    [0x09, mode, index, 0]
}

// ---------- MAXFIT61 keyboard ----------

fn kb_event(e: &Event) -> [u8; 4] {
    let (code, mouse) = match e.input {
        Input::Key(u) => (u, false),
        Input::Mouse(MouseBtn::Left) => (0x01, true),
        Input::Mouse(MouseBtn::Right) => (0x02, true),
        Input::Mouse(MouseBtn::Middle) => (0x04, true),
    };
    let flag = (if e.down { 0x10 } else { 0 }) | u8::from(mouse);
    let [hi, lo] = e.delay_ms.to_be_bytes();
    [code, flag, hi, lo]
}

/// The five `19` reports (64 bytes, report id 01) for one keyboard slot.
pub fn kb_pages(slot: u8, m: &Macro) -> Vec<[u8; 64]> {
    let events: Vec<u8> = m.events.iter().take(KB_MAX_EVENTS).flat_map(kb_event).collect();
    let (mode, repeat) = match m.repeat {
        Repeat::Times(1) => (0, 1),
        Repeat::Times(n) => (1, n.min(255) as u8),
        Repeat::Toggle => (2, 1),
        Repeat::WhileHeld => (3, 1),
    };
    // Metadata: mode, event bytes (= count * 4), repeat count.
    let mut stream = vec![0, mode, 0, events.len() as u8, 0, repeat, 0, 0];
    stream.extend_from_slice(&events);
    stream.resize(4 * 58 + 24, 0);
    let mut out = Vec::new();
    let mut off = 0;
    for page in 0..5u8 {
        let n = if page == 4 { 24 } else { 58 };
        let mut p = [0u8; 64];
        p[0] = 0x01;
        p[1] = 0x19;
        p[2] = slot;
        p[4] = page;
        p[5] = n as u8;
        p[6..6 + n].copy_from_slice(&stream[off..off + n]);
        off += n;
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Checksum;

    fn a_tap() -> Macro {
        Macro {
            name: "A".into(),
            events: vec![
                Event { input: Input::Key(0x04), down: true, delay_ms: 50 },
                Event { input: Input::Key(0x04), down: false, delay_ms: 0 },
            ],
            repeat: Repeat::Times(1),
        }
    }

    #[test]
    fn ry_matches_spec_example() {
        let pages = ry_pages(0, &a_tap());
        assert_eq!(pages.len(), 1);
        let p = Checksum::Bit7.apply(&pages[0]);
        assert_eq!(&p[..14], &[0x16, 0, 0, 0x05, 0x01, 0, 0, 0xE3, 0x01, 0, 0x04, 0xB2, 0x04, 0x00]);
        assert_eq!(ry_binding(0, Repeat::Times(1)), [0x09, 0, 0, 0]);
    }

    #[test]
    fn ry_long_delay_and_multi_page() {
        let mut m = a_tap();
        m.events[0].delay_ms = 500;
        let img = ry_image(&m);
        assert_eq!(&img[2..6], &[0x04, 0x80, 0xF4, 0x01]);
        let big = Macro { events: vec![m.events[0]; 40], ..m };
        let pages = ry_pages(3, &big);
        assert!(pages.len() > 2);
        assert_eq!(pages.last().unwrap()[4], 1);
        assert!(big.fits_ry());
    }

    #[test]
    fn kb_layout_matches_spec() {
        let mut m = a_tap();
        m.events[1].delay_ms = 100;
        let pages = kb_pages(2, &m);
        assert_eq!(pages.len(), 5);
        let p0 = pages[0];
        assert_eq!(&p0[..6], &[0x01, 0x19, 0x02, 0x00, 0x00, 0x3A]);
        // metadata: mode once -> 00, 2 events -> 8 bytes, repeat 1
        assert_eq!(&p0[6..14], &[0, 0, 0, 8, 0, 1, 0, 0]);
        assert_eq!(&p0[14..22], &[0x04, 0x10, 0x00, 0x32, 0x04, 0x00, 0x00, 0x64]);
        assert_eq!(pages[4][5], 0x18);
    }
}
