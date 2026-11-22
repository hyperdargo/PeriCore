//! Renders a 32x32 RGBA tray icon: battery percentage as large digits over a
//! colour-coded battery bar. Pure function, no OS calls.
//!
//! Like a laptop battery, it is live: the fill follows the level, a charging
//! battery animates its fill upward (driven by `phase`), a full battery on
//! power shows a check mark, and a sleeping device is drawn dimmed.

pub const SIZE: usize = 32;
/// Number of animation frames for the charging sweep.
pub const CHARGE_PHASES: u8 = 4;

/// 3x5 bitmap digits (low 3 bits per row, MSB = leftmost), drawn at 2x so
/// a device picture fits beside the number.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];
const QUESTION: [u8; 5] = [0b111, 0b001, 0b011, 0b000, 0b010];

/// Which picture to draw beside the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Unknown,
    Mouse,
    Controller,
}

/// 5x7 mouse: two buttons split by the wheel.
const MOUSE: [u8; 7] = [0b01110, 0b10101, 0b10101, 0b11111, 0b10001, 0b10001, 0b01110];
/// 7x6 gamepad: d-pad and buttons as holes, two grips.
const PAD: [u8; 6] = [0b0111110, 0b1111111, 0b1011101, 0b1110111, 0b1100011, 0b1000001];

/// 5x7 lightning bolt.
const BOLT: [u8; 7] = [0b00011, 0b00110, 0b01100, 0b11111, 0b00110, 0b01100, 0b11000];
/// 5x7 check mark.
const CHECK: [u8; 7] = [0b00000, 0b00001, 0b00010, 0b10100, 0b01000, 0b00000, 0b00000];

type Rgba = [u8; 4];
const WHITE: Rgba = [240, 240, 240, 255];
const BLACK: Rgba = [0, 0, 0, 255];
const GREY: Rgba = [150, 150, 150, 255];
const GREEN: Rgba = [46, 204, 113, 255];
const YELLOW: Rgba = [241, 196, 15, 255];
const RED: Rgba = [231, 76, 60, 255];
const CYAN: Rgba = [52, 200, 230, 255];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IconState {
    /// None = unknown / disconnected.
    pub percent: Option<u8>,
    pub charging: bool,
    /// Fully charged and still on power.
    pub full: bool,
    /// Device asleep: showing its last known level.
    pub asleep: bool,
    /// Charging animation frame, 0..CHARGE_PHASES.
    pub phase: u8,
    pub device: Device,
}

pub fn level_color(state: IconState) -> Rgba {
    match state.percent {
        None => GREY,
        Some(_) if state.full => GREEN,
        Some(_) if state.charging => CYAN,
        Some(p) if p <= 15 => RED,
        Some(p) if p <= 35 => YELLOW,
        Some(_) => GREEN,
    }
}

/// Picture colour per device type, so mice and pads differ at a glance.
fn device_color(d: Device) -> Rgba {
    match d {
        Device::Controller => [120, 150, 255, 255],
        _ => [190, 160, 255, 255],
    }
}

/// Halves brightness and opacity (sleeping device).
fn dim(c: Rgba) -> Rgba {
    [c[0] / 2 + 40, c[1] / 2 + 40, c[2] / 2 + 40, 170]
}

struct Canvas(Vec<u8>);

impl Canvas {
    fn new() -> Self {
        Self(vec![0; SIZE * SIZE * 4])
    }
    fn put(&mut self, x: usize, y: usize, c: Rgba) {
        if x < SIZE && y < SIZE {
            let i = (y * SIZE + x) * 4;
            self.0[i..i + 4].copy_from_slice(&c);
        }
    }
    fn rect(&mut self, x0: usize, y0: usize, w: usize, h: usize, c: Rgba) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                self.put(x, y, c);
            }
        }
    }
    fn glyph(&mut self, g: &[u8; 7], x0: usize, y0: usize, scale: usize, c: Rgba) {
        self.bitmap(g, 5, x0, y0, scale, c);
    }
    /// Rows of `width` bits each (MSB = leftmost pixel).
    fn bitmap(&mut self, rows: &[u8], width: usize, x0: usize, y0: usize, scale: usize, c: Rgba) {
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..width {
                if bits & (1 << (width - 1 - col)) != 0 {
                    self.rect(x0 + col * scale, y0 + row * scale, scale, scale, c);
                }
            }
        }
    }
    /// A 1px-scale glyph with a black outline, readable over any fill.
    fn badge(&mut self, g: &[u8; 7], x: usize, y: usize, c: Rgba) {
        for (dx, dy) in [(0, 1), (2, 1), (1, 0), (1, 2)] {
            self.glyph(g, x + dx - 1, y + dy - 1, 1, BLACK);
        }
        self.glyph(g, x, y, 1, c);
    }
}

fn glyphs_for(state: IconState) -> Vec<&'static [u8; 5]> {
    match state.percent {
        None => vec![&QUESTION],
        Some(p) => p.min(99).to_string().bytes().map(|b| &DIGITS[(b - b'0') as usize]).collect(),
    }
}

/// Filled width of the bar's `inner` pixels. While charging, the fill sweeps
/// from the real level up to full across the animation frames.
fn fill_width(state: IconState, inner: usize) -> usize {
    let Some(p) = state.percent else { return inner };
    let level = (inner * p.min(100) as usize).div_ceil(100).max(1);
    if state.charging && !state.full {
        let phase = (state.phase % CHARGE_PHASES) as usize;
        level + (inner - level) * phase / CHARGE_PHASES as usize
    } else {
        level
    }
}

pub fn render(state: IconState) -> Vec<u8> {
    let mut c = Canvas::new();
    let tone = |col: Rgba| if state.asleep { dim(col) } else { col };
    let (white, color) = (tone(WHITE), tone(level_color(state)));

    // Everything sits on a 16x16 grid of 2x2 cells, so Windows' 16 px tray
    // size (100% scaling) is an exact half with no blur.
    const U: usize = SIZE / 16;
    let cell = |c: &mut Canvas, x: usize, y: usize, w: usize, h: usize, col: Rgba| c.rect(x * U, y * U, w * U, h * U, col);

    // Top row: device picture, then the number (3x5 digits, 1-cell gaps).
    let picture: Option<(&[u8], usize)> = match state.device {
        Device::Mouse => Some((&MOUSE, 5)),
        Device::Controller => Some((&PAD, 7)),
        Device::Unknown => None,
    };
    let glyphs = glyphs_for(state);
    let digits_w = glyphs.len() * 4 - 1;
    let pic_w = picture.map_or(0, |(_, w)| w + 1);
    let mut x = (16 - (pic_w + digits_w)) / 2;
    if let Some((rows, w)) = picture {
        c.bitmap(rows, w, x * U, 0, U, tone(device_color(state.device)));
        x += pic_w;
    }
    for g in glyphs {
        c.bitmap(g, 3, x * U, U, U, white);
        x += 4;
    }

    // Battery: outline cells x 0..15, y 9..16, nub at x 15; fill inside.
    cell(&mut c, 0, 9, 15, 1, white);
    cell(&mut c, 0, 15, 15, 1, white);
    cell(&mut c, 0, 9, 1, 7, white);
    cell(&mut c, 14, 9, 1, 7, white);
    cell(&mut c, 15, 11, 1, 3, white);
    cell(&mut c, 1, 10, fill_width(state, 13), 5, color);
    let (bx, by, bw) = (0, 19, 30); // badge origin, in pixels

    // Status badge in the middle of the bar.
    if state.percent.is_some() {
        let (gx, gy) = (bx + bw / 2 - 2, by + 2);
        if state.full {
            c.badge(&CHECK, gx, gy, WHITE);
        } else if state.charging {
            c.badge(&BOLT, gx, gy, WHITE);
        }
    }
    c.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * SIZE + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn s(percent: Option<u8>, charging: bool) -> IconState {
        IconState { percent, charging, ..Default::default() }
    }

    #[test]
    fn charging_draws_bolt() {
        let plain = render(s(Some(50), false));
        let ch = render(s(Some(50), true));
        assert_ne!(plain, ch);
        // Bolt centre row is solid white.
        assert_eq!(px(&ch, 15, 24), WHITE);
    }

    #[test]
    fn charging_fill_animates_upward() {
        let widths: Vec<usize> = (0..CHARGE_PHASES)
            .map(|phase| fill_width(IconState { phase, ..s(Some(20), true) }, 24))
            .collect();
        assert_eq!(widths[0], 5, "frame 0 shows the real level");
        assert!(widths.windows(2).all(|w| w[0] < w[1]), "{widths:?}");
        // Not charging: the fill never moves.
        assert_eq!(fill_width(IconState { phase: 3, ..s(Some(20), false) }, 24), 5);
    }

    #[test]
    fn full_shows_check_in_green() {
        let st = IconState { full: true, ..s(Some(100), false) };
        assert_eq!(level_color(st), GREEN);
        assert_ne!(render(st), render(s(Some(100), false)));
    }

    #[test]
    fn asleep_is_dimmed() {
        let st = IconState { asleep: true, ..s(Some(80), false) };
        assert!(px(&render(st), 20, 24)[3] < 255);
    }

    #[test]
    fn output_is_32x32_rgba() {
        assert_eq!(render(s(Some(50), false)).len(), 32 * 32 * 4);
    }

    #[test]
    fn colours_by_level() {
        assert_eq!(level_color(s(Some(10), false)), RED);
        assert_eq!(level_color(s(Some(30), false)), YELLOW);
        assert_eq!(level_color(s(Some(80), false)), GREEN);
        assert_eq!(level_color(s(Some(80), true)), CYAN);
        assert_eq!(level_color(s(None, false)), GREY);
    }

    #[test]
    fn fill_scales_with_percent() {
        let low = render(s(Some(10), false));
        let full = render(s(Some(100), false));
        // x=20 inside the bar: empty at 10%, filled at 100%.
        assert_eq!(px(&low, 20, 24)[3], 0);
        assert_eq!(px(&full, 20, 24), GREEN);
    }

    #[test]
    fn device_picture_changes_icon() {
        let m = IconState { device: Device::Mouse, ..s(Some(50), false) };
        let p = IconState { device: Device::Controller, ..s(Some(50), false) };
        assert_ne!(render(m), render(p));
        assert_ne!(render(m), render(s(Some(50), false)));
        // Two digits plus the widest picture still fit in 32 px.
        let _ = render(IconState { device: Device::Controller, ..s(Some(88), false) });
    }

    #[test]
    fn hundred_renders_two_digits() {
        assert_eq!(glyphs_for(s(Some(100), false)).len(), 2);
    }
}

/// `cargo test preview_icons -- --ignored` with PERICORE_ICON_DIR set writes sample PNGs.
#[cfg(test)]
mod preview {
    use super::*;

    #[test]
    #[ignore]
    fn preview_icons() {
        let Ok(dir) = std::env::var("PERICORE_ICON_DIR") else { return };
        let cases = [
            ("mouse_86", IconState { percent: Some(86), device: Device::Mouse, ..Default::default() }),
            ("mouse_12", IconState { percent: Some(12), device: Device::Mouse, ..Default::default() }),
            ("mouse_asleep", IconState { percent: Some(48), device: Device::Mouse, asleep: true, ..Default::default() }),
            ("pad_62", IconState { percent: Some(62), device: Device::Controller, ..Default::default() }),
            ("pad_charging", IconState { percent: Some(35), device: Device::Controller, charging: true, phase: 2, ..Default::default() }),
            ("pad_full", IconState { percent: Some(100), device: Device::Controller, full: true, ..Default::default() }),
        ];
        for (name, st) in cases {
            let img = image::RgbaImage::from_raw(SIZE as u32, SIZE as u32, render(st)).unwrap();
            let big = image::imageops::resize(&img, 128, 128, image::imageops::FilterType::Nearest);
            big.save(format!("{dir}/{name}.png")).unwrap();
            let small = image::imageops::resize(&img, 16, 16, image::imageops::FilterType::Triangle);
            image::imageops::resize(&small, 128, 128, image::imageops::FilterType::Nearest).save(format!("{dir}/{name}_16px.png")).unwrap();
        }
    }
}
