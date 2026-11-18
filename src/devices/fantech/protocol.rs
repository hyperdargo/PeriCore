//! Fantech MAXFIT61 (VID 0416 / PID C345, XS61BRGB firmware) packet builders.
//! See docs/fantech-maxfit61-protocol.md. Packets are 64 bytes written with
//! hid_write: [report_id=01, opcode, selector, 0, page, count, data...].
//! No checksum.

pub const REPORT_ID: u8 = 0x01;
pub const LEN: usize = 64;

pub mod op {
    pub const LIGHT_CAPS: u8 = 0x0A;
    pub const IDENTIFY: u8 = 0x0D;
    pub const SET_LIGHT: u8 = 0x07;
    pub const RESET: u8 = 0x14;
}

pub fn simple(opcode: u8) -> [u8; LEN] {
    let mut p = [0u8; LEN];
    p[0] = REPORT_ID;
    p[1] = opcode;
    p
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KbLighting {
    /// Effect id as reported by the LIGHT_CAPS query.
    pub effect: u8,
    pub brightness: u8,
    pub speed: u8,
    pub color: [u8; 3],
    pub background: [u8; 3],
    pub direction: u8,
    /// 1 = colourful/rainbow (firmware ignores colour), 0 = use `color`.
    pub rainbow: bool,
    pub on: bool,
}

impl Default for KbLighting {
    fn default() -> Self {
        Self {
            effect: 1,
            brightness: 4,
            speed: 2,
            color: [255, 255, 255],
            background: [0, 0, 0],
            direction: 0,
            rainbow: false,
            on: true,
        }
    }
}

impl KbLighting {
    /// Opcode 07 packet; field positions confirmed from FANTECH.exe 0x00700E60.
    pub fn packet(&self) -> [u8; LEN] {
        let mut p = simple(op::SET_LIGHT);
        p[5] = 0x0E;
        p[6] = self.effect;
        p[7] = self.brightness;
        p[8] = self.speed;
        let mode_type = if self.effect == 0 { 0 } else { self.rainbow as u8 };
        // Vendor rule (0x00700F67..0x00701103): effects outside COLOR_ALWAYS
        // with mode type 1 send FF 00 00 / 00 00 00 instead of the colours.
        let (fg, bg) = if mode_type == 1 && !COLOR_ALWAYS.contains(&self.effect) {
            ([0xFF, 0, 0], [0, 0, 0])
        } else {
            (self.color, self.background)
        };
        p[9..12].copy_from_slice(&fg);
        p[12..15].copy_from_slice(&bg);
        p[15] = self.direction;
        p[16] = mode_type;
        p[17] = self.on as u8;
        p
    }
}

/// Effect types whose RGB bytes are always sent as-is by the vendor app.
const COLOR_ALWAYS: [u8; 7] = [0x05, 0x06, 0x07, 0x08, 0x09, 0x0B, 0x0C];

/// Parsed LIGHT_CAPS reply: effect ids then sentinel A5/5A, speed max, brightness max.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightCaps {
    pub effects: Vec<u8>,
    pub speed_max: u8,
    pub brightness_max: u8,
}

pub fn parse_caps(reply: &[u8]) -> Option<LightCaps> {
    if reply.get(1) != Some(&op::LIGHT_CAPS) {
        return None;
    }
    // Effect ids, then the two-byte terminator A5 5A, then speed max, brightness max.
    let data = reply.get(6..)?;
    // Real V1.03.07 firmware sends the pair; fall back to a lone sentinel.
    let (end, skip) = match data.windows(2).position(|w| w == [0xA5, 0x5A]) {
        Some(i) => (i, 2),
        None => (data.iter().position(|&b| b == 0xA5 || b == 0x5A)?, 1),
    };
    Some(LightCaps {
        effects: data[..end].to_vec(),
        speed_max: *data.get(end + skip)?,
        brightness_max: *data.get(end + skip + 1)?,
    })
}

/// IDENTIFY reply carries ASCII text from offset 6.
pub fn parse_identify(reply: &[u8]) -> Option<String> {
    if reply.get(1) != Some(&op::IDENTIFY) {
        return None;
    }
    let text: String = reply
        .get(6..)?
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| b as char)
        .collect();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_packet_layout() {
        let l = KbLighting { effect: 3, brightness: 4, speed: 1, color: [255, 0, 0], ..Default::default() };
        let p = l.packet();
        assert_eq!(&p[..9], &[1, 7, 0, 0, 0, 0x0E, 3, 4, 1]);
        assert_eq!(&p[9..12], &[255, 0, 0]);
        assert_eq!(p[17], 1);
        assert!(p[18..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rainbow_on_plain_effect_forces_vendor_colours() {
        let l = KbLighting { effect: 2, rainbow: true, color: [1, 2, 3], background: [4, 5, 6], ..Default::default() };
        assert_eq!(&l.packet()[9..15], &[0xFF, 0, 0, 0, 0, 0]);
        let l = KbLighting { effect: 0x09, rainbow: true, color: [1, 2, 3], ..Default::default() };
        assert_eq!(&l.packet()[9..12], &[1, 2, 3]);
    }

    #[test]
    fn caps_single_sentinel_fallback() {
        let mut r = [0u8; 64];
        r[1] = 0x0A;
        r[6..11].copy_from_slice(&[1, 2, 0xA5, 4, 3]);
        let c = parse_caps(&r).unwrap();
        assert_eq!((c.effects.len(), c.speed_max, c.brightness_max), (2, 4, 3));
    }

    #[test]
    fn off_effect_clears_mode_type() {
        let l = KbLighting { effect: 0, rainbow: true, ..Default::default() };
        assert_eq!(l.packet()[16], 0);
    }

    #[test]
    fn caps_parse() {
        let mut r = [0u8; 64];
        r[1] = 0x0A;
        // Real MAXFIT61 reply (V1.03.07).
        r[5] = 0x12;
        r[6..24].copy_from_slice(&[2, 1, 3, 9, 4, 0, 14, 15, 6, 7, 11, 12, 16, 8, 0xA5, 0x5A, 4, 3]);
        let c = parse_caps(&r).unwrap();
        assert_eq!(c.effects, vec![2, 1, 3, 9, 4, 0, 14, 15, 6, 7, 11, 12, 16, 8]);
        assert_eq!((c.speed_max, c.brightness_max), (4, 3));
    }

    #[test]
    fn caps_rejects_other_opcode() {
        assert!(parse_caps(&[0, 0x0D, 0, 0, 0, 0, 0xA5, 1, 1]).is_none());
    }

    #[test]
    fn identify_parse() {
        let mut r = [0u8; 64];
        r[1] = 0x0D;
        r[6..14].copy_from_slice(b"XS61,V13");
        assert_eq!(parse_identify(&r).as_deref(), Some("XS61,V13"));
    }
}
