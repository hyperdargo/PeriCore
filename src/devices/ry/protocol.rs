//! Rongyuan ("RY") firmware protocol: command ids and byte-level encoders.
//! Offsets come from the AJAZZ Driver 2.1.94 mouse class. Settings blocks are
//! edited in place (read -> modify -> write) so bytes we don't model survive.

pub mod cmd {
    pub const GET_DANGLE_ID: u8 = 0xF1;
    /// Receiver status: both batteries, no checksum.
    pub const RECEIVER_STATUS: u8 = 0xF7;
    pub const GET_BATTERY: u8 = 0x82;
    pub const MOUSE_SET_OPTION0: u8 = 0x53;
    pub const MOUSE_GET_OPTION0: u8 = 0xD3;
    pub const MOUSE_SET_OPTION1: u8 = 0x54;
    pub const MOUSE_GET_OPTION1: u8 = 0xD4;
}

const MAX_SPEED: u8 = 4;
const OPT_NORMAL: u8 = 7;
const OPT_DAZZLE: u8 = 8;

// ---------------------------------------------------------------- polling rate

pub const POLL_RATES: [u32; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

pub fn rate_to_code(hz: u32) -> u8 {
    match hz {
        8000 => 0x81,
        4000 => 0x82,
        2000 => 0x84,
        1000 => 0x01,
        500 => 0x02,
        250 => 0x04,
        125 => 0x08,
        _ => 0,
    }
}

pub fn code_to_rate(code: u8) -> Option<u32> {
    match code {
        0x81 => Some(8000),
        0x82 => Some(4000),
        0x84 => Some(2000),
        0x01 => Some(1000),
        0x02 => Some(500),
        0x04 => Some(250),
        0x08 => Some(125),
        _ => None,
    }
}

// ---------------------------------------------------------------- lighting

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightEffect {
    Off,
    Static,
    Breathing,
    Neon,
    Wave,
    Dazzling,
    Laser,
}

impl LightEffect {
    pub const ALL: [LightEffect; 7] = [
        Self::Off,
        Self::Static,
        Self::Breathing,
        Self::Neon,
        Self::Wave,
        Self::Dazzling,
        Self::Laser,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Static => "Static",
            Self::Breathing => "Breathing",
            Self::Neon => "Neon",
            Self::Wave => "Wave",
            Self::Dazzling => "Dazzling",
            Self::Laser => "Laser",
        }
    }

    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(c: u8) -> Option<Self> {
        Self::ALL.get(c as usize).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    pub effect: LightEffect,
    /// 0..=4
    pub brightness: u8,
    /// 0..=4 (higher = faster)
    pub speed: u8,
    /// Cycle through colours instead of using `rgb`.
    pub rainbow: bool,
    pub rgb: [u8; 3],
}

impl Default for Lighting {
    fn default() -> Self {
        Self { effect: LightEffect::Static, brightness: 4, speed: 2, rainbow: false, rgb: [255, 0, 0] }
    }
}

impl Lighting {
    pub fn encode(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        let opt = if self.rainbow { OPT_DAZZLE } else { OPT_NORMAL };
        b[0] = self.effect.code();
        b[1] = MAX_SPEED - self.speed.min(MAX_SPEED);
        b[2] = self.brightness.min(4);
        b[3] = match self.effect {
            LightEffect::Off => OPT_NORMAL,
            LightEffect::Neon => OPT_NORMAL,
            _ => opt,
        };
        if self.effect == LightEffect::Off {
            b[1] = MAX_SPEED;
            b[2] = 0;
        }
        b[4..7].copy_from_slice(&self.rgb);
        b
    }

    pub fn decode(b: &[u8]) -> Option<Self> {
        let effect = LightEffect::from_code(*b.first()?)?;
        Some(Self {
            effect,
            speed: MAX_SPEED.saturating_sub(b[1]),
            brightness: b[2].min(4),
            rainbow: b[3] & 0x0F == OPT_DAZZLE,
            rgb: [b[4], b[5], b[6]],
        })
    }
}

// ---------------------------------------------------------------- option block 0

/// General mouse settings block (GET 0xD3 / SET 0x53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseOption0(pub [u8; 64]);

mod o0 {
    pub const PROFILE: usize = 8;
    pub const RATE: usize = 9;
    pub const DEBOUNCE: usize = 10;
    pub const FLAGS: usize = 12;
    pub const LIGHT: usize = 24;
    pub const SLEEP: usize = 40;
    pub const LIFT_CUTOFF: usize = 52;
    pub const ANGLE_SNAP: usize = 53;
}

#[allow(dead_code)] // full flag set documented for future UI options
pub mod flag {
    pub const LIGHT_OFF: u16 = 1 << 0;
    pub const WHEEL_LIGHT_OFF: u16 = 1 << 1;
    pub const MOTION_SYNC: u16 = 1 << 2;
    pub const LED_SELECT: u16 = 1 << 3;
    pub const POWER_SAVE: u16 = 1 << 4;
}

impl MouseOption0 {
    pub fn poll_rate(&self) -> Option<u32> {
        code_to_rate(self.0[o0::RATE])
    }
    pub fn with_poll_rate(&self, hz: u32) -> Self {
        self.with_byte(o0::RATE, rate_to_code(hz))
    }
    pub fn debounce_ms(&self) -> u8 {
        self.0[o0::DEBOUNCE]
    }
    pub fn with_debounce_ms(&self, ms: u8) -> Self {
        self.with_byte(o0::DEBOUNCE, ms)
    }
    pub fn flags(&self) -> u16 {
        u16::from_le_bytes([self.0[o0::FLAGS], self.0[o0::FLAGS + 1]])
    }
    pub fn has_flag(&self, f: u16) -> bool {
        self.flags() & f != 0
    }
    pub fn with_flag(&self, f: u16, on: bool) -> Self {
        let v = if on { self.flags() | f } else { self.flags() & !f };
        let [lo, hi] = v.to_le_bytes();
        self.with_byte(o0::FLAGS, lo).with_byte(o0::FLAGS + 1, hi)
    }
    pub fn lighting(&self) -> Option<Lighting> {
        Lighting::decode(&self.0[o0::LIGHT..o0::LIGHT + 8])
    }
    /// Writes effect/speed/brightness/colour; keeps the option byte's upper
    /// nibble (wave direction etc.) and the reserved 8th byte as read.
    pub fn with_lighting(&self, l: &Lighting) -> Self {
        let mut b = self.0;
        let enc = l.encode();
        let o = o0::LIGHT;
        b[o..o + 3].copy_from_slice(&enc[..3]);
        b[o + 3] = (self.0[o + 3] & 0xF0) | (enc[3] & 0x0F);
        b[o + 4..o + 7].copy_from_slice(&enc[4..7]);
        Self(b)
    }
    /// 2.4G idle sleep, seconds.
    pub fn sleep_secs(&self) -> u16 {
        u16::from_le_bytes([self.0[o0::SLEEP + 4], self.0[o0::SLEEP + 5]])
    }
    pub fn with_sleep_secs(&self, s: u16) -> Self {
        let [lo, hi] = s.to_le_bytes();
        self.with_byte(o0::SLEEP + 4, lo).with_byte(o0::SLEEP + 5, hi)
    }
    pub fn lift_cutoff(&self) -> u8 {
        self.0[o0::LIFT_CUTOFF]
    }
    pub fn with_lift_cutoff(&self, v: u8) -> Self {
        self.with_byte(o0::LIFT_CUTOFF, v)
    }
    pub fn angle_snap(&self) -> bool {
        self.0[o0::ANGLE_SNAP] != 0
    }
    pub fn with_angle_snap(&self, on: bool) -> Self {
        self.with_byte(o0::ANGLE_SNAP, on as u8)
    }

    /// Payload for SET: reuse the block, swap the command byte.
    pub fn to_set_payload(&self) -> [u8; 64] {
        let mut b = self.0;
        b[0] = cmd::MOUSE_SET_OPTION0;
        b[o0::PROFILE] = self.0[o0::PROFILE];
        b
    }

    fn with_byte(&self, i: usize, v: u8) -> Self {
        let mut b = self.0;
        b[i] = v;
        Self(b)
    }
}

// ---------------------------------------------------------------- option block 1 (DPI)

pub const MAX_DPI_STAGES: usize = 8;

/// DPI stages block (GET 0xD4 / SET 0x54).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseOption1(pub [u8; 64]);

impl MouseOption1 {
    pub fn current_stage(&self) -> u8 {
        self.0[2]
    }
    pub fn stage_count(&self) -> usize {
        (self.0[3] as usize).clamp(1, MAX_DPI_STAGES)
    }
    pub fn stages(&self) -> Vec<u16> {
        (0..self.stage_count())
            .map(|i| u16::from_le_bytes([self.0[8 + 2 * i], self.0[9 + 2 * i]]))
            .collect()
    }
    pub fn stage_color(&self, i: usize) -> [u8; 3] {
        let o = 40 + 3 * i;
        [self.0[o], self.0[o + 1], self.0[o + 2]]
    }

    pub fn with_stages(&self, current: u8, stages: &[u16], colors: &[[u8; 3]]) -> Self {
        let mut b = self.0;
        let n = stages.len().min(MAX_DPI_STAGES);
        b[2] = current.min(n.saturating_sub(1) as u8);
        b[3] = n as u8;
        for (i, dpi) in stages.iter().take(n).enumerate() {
            let [lo, hi] = dpi.to_le_bytes();
            b[8 + 2 * i] = lo;
            b[9 + 2 * i] = hi;
        }
        for (i, c) in colors.iter().take(MAX_DPI_STAGES).enumerate() {
            b[40 + 3 * i..43 + 3 * i].copy_from_slice(c);
        }
        Self(b)
    }

    pub fn to_set_payload(&self, profile: u8) -> [u8; 64] {
        let mut b = self.0;
        b[0] = cmd::MOUSE_SET_OPTION1;
        b[1] = profile;
        b
    }
}

// ---------------------------------------------------------------- battery

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Discharging,
    Charging,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryInfo {
    pub percent: u8,
    pub state: ChargeState,
}

/// Parses a GET_BATTERY reply: [cmd, percent, state(1=charging,2=full), lowpower].
pub fn parse_battery(reply: &[u8]) -> Option<BatteryInfo> {
    if reply.len() < 3 || reply[0] != cmd::GET_BATTERY || reply[1] > 100 {
        return None;
    }
    let state = match reply[2] {
        1 => ChargeState::Charging,
        2 => ChargeState::Full,
        _ => ChargeState::Discharging,
    };
    Some(BatteryInfo { percent: reply[1], state })
}

/// Parses the receiver status reply to F7 (docs/ajazz-dangle-protocol.md):
/// r[1] = keyboard battery, r[2] = mouse battery. The receiver doesn't report
/// charging here, so state is always Discharging.
pub fn parse_receiver_status(reply: &[u8], mouse: bool) -> Option<BatteryInfo> {
    let level = *reply.get(if mouse { 2 } else { 1 })?;
    (1..=100)
        .contains(&level)
        .then_some(BatteryInfo { percent: level, state: ChargeState::Discharging })
}

/// Parses the receiver id reply: [0xF1, id_lo, id_hi].
pub fn parse_dangle_id(reply: &[u8]) -> Option<u16> {
    (reply.len() >= 3 && reply[0] == cmd::GET_DANGLE_ID)
        .then(|| u16::from_le_bytes([reply[1], reply[2]]))
        .filter(|&id| id != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_codes_roundtrip() {
        for hz in POLL_RATES {
            assert_eq!(code_to_rate(rate_to_code(hz)), Some(hz));
        }
        assert_eq!(rate_to_code(1000), 1);
        assert_eq!(rate_to_code(8000), 0x81);
    }

    #[test]
    fn lighting_static_matches_vendor_layout() {
        let l = Lighting { effect: LightEffect::Static, brightness: 4, speed: 0, rainbow: false, rgb: [1, 2, 3] };
        assert_eq!(l.encode(), [1, 4, 4, 7, 1, 2, 3, 0]);
    }

    #[test]
    fn lighting_breathing_rainbow() {
        let l = Lighting { effect: LightEffect::Breathing, brightness: 2, speed: 3, rainbow: true, rgb: [0; 3] };
        assert_eq!(l.encode()[..4], [2, 1, 2, 8]);
    }

    #[test]
    fn lighting_roundtrip() {
        let l = Lighting { effect: LightEffect::Wave, brightness: 3, speed: 1, rainbow: true, rgb: [9, 8, 7] };
        assert_eq!(Lighting::decode(&l.encode()), Some(l));
    }

    #[test]
    fn option0_edits_preserve_other_bytes() {
        let mut raw = [0xAAu8; 64];
        raw[9] = 1;
        let o = MouseOption0(raw).with_poll_rate(4000).with_flag(flag::MOTION_SYNC, true);
        assert_eq!(o.poll_rate(), Some(4000));
        assert!(o.has_flag(flag::MOTION_SYNC));
        assert_eq!(o.0[20], 0xAA);
        assert_eq!(o.to_set_payload()[0], cmd::MOUSE_SET_OPTION0);
    }

    #[test]
    fn with_lighting_keeps_unmodeled_bits() {
        let mut raw = [0u8; 64];
        raw[27] = 0x30; // wave direction nibble
        raw[31] = 0x5A; // reserved
        let l = Lighting { effect: LightEffect::Static, brightness: 4, speed: 0, rainbow: false, rgb: [1, 2, 3] };
        let o = MouseOption0(raw).with_lighting(&l);
        assert_eq!(o.0[27], 0x37);
        assert_eq!(o.0[31], 0x5A);
        assert_eq!(&o.0[24..27], &[1, 4, 4]);
    }

    #[test]
    fn battery_rejects_stale_reply() {
        assert!(parse_battery(&[0xF1, 0xDF, 0x06, 0]).is_none());
    }

    #[test]
    fn option0_flag_clear() {
        let o = MouseOption0([0xFF; 64]).with_flag(flag::LIGHT_OFF, false);
        assert!(!o.has_flag(flag::LIGHT_OFF));
        assert!(o.has_flag(flag::POWER_SAVE));
    }

    #[test]
    fn option1_dpi_roundtrip() {
        let o = MouseOption1([0; 64]).with_stages(1, &[400, 800, 1600, 26000], &[[255, 0, 0]]);
        assert_eq!(o.stages(), vec![400, 800, 1600, 26000]);
        assert_eq!(o.current_stage(), 1);
        assert_eq!(o.stage_color(0), [255, 0, 0]);
        let p = o.to_set_payload(2);
        assert_eq!((p[0], p[1]), (cmd::MOUSE_SET_OPTION1, 2));
    }

    #[test]
    fn battery_parse() {
        let b = parse_battery(&[0x82, 76, 1, 0]).unwrap();
        assert_eq!(b, BatteryInfo { percent: 76, state: ChargeState::Charging });
        assert!(parse_battery(&[0x82, 200, 0]).is_none());
    }

    #[test]
    fn receiver_status_parse() {
        // Captured from a real AJ159 APEX receiver.
        let r = [0x00, 0x00, 0x5B, 0x01, 0x01, 0x01, 0x02];
        assert_eq!(parse_receiver_status(&r, true).unwrap().percent, 91);
        assert!(parse_receiver_status(&r, false).is_none());
        assert!(parse_receiver_status(&[0, 0, 0xC8], true).is_none());
    }

    #[test]
    fn dangle_id_parse() {
        assert_eq!(parse_dangle_id(&[0xF1, 0x2D, 0x09]), Some(2349));
        assert_eq!(parse_dangle_id(&[0x00, 0x2D, 0x09]), None);
    }
}
