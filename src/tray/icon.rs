//! Renders a 32x32 RGBA tray icon: battery percentage as large digits over a
//! colour-coded battery bar. Pure function, no OS calls.

pub const SIZE: usize = 32;

/// 5x7 bitmap digits, one u8 per row (low 5 bits, MSB = leftmost pixel).
const DIGITS: [[u8; 7]; 10] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
];
const QUESTION: [u8; 7] = [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04];

type Rgba = [u8; 4];
const WHITE: Rgba = [240, 240, 240, 255];
const GREY: Rgba = [150, 150, 150, 255];
const GREEN: Rgba = [46, 204, 113, 255];
const YELLOW: Rgba = [241, 196, 15, 255];
const RED: Rgba = [231, 76, 60, 255];
const CYAN: Rgba = [52, 200, 230, 255];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconState {
    /// None = unknown / disconnected.
    pub percent: Option<u8>,
    pub charging: bool,
}

pub fn level_color(state: IconState) -> Rgba {
    match state.percent {
        None => GREY,
        Some(_) if state.charging => CYAN,
        Some(p) if p <= 15 => RED,
        Some(p) if p <= 35 => YELLOW,
        Some(_) => GREEN,
    }
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
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    self.rect(x0 + col * scale, y0 + row * scale, scale, scale, c);
                }
            }
        }
    }
}

fn glyphs_for(state: IconState) -> Vec<&'static [u8; 7]> {
    match state.percent {
        None => vec![&QUESTION],
        Some(p) => p.min(99).to_string().bytes().map(|b| &DIGITS[(b - b'0') as usize]).collect(),
    }
}

pub fn render(state: IconState) -> Vec<u8> {
    let mut c = Canvas::new();
    let color = level_color(state);

    // Digits: 5x7 at scale 2 -> 10x14, 2px gap. 100% shows "99"+full bar.
    let glyphs = glyphs_for(state);
    let width = glyphs.len() * 10 + (glyphs.len() - 1) * 2;
    let mut x = (SIZE - width) / 2;
    for g in glyphs {
        c.glyph(g, x, 1, 2, WHITE);
        x += 12;
    }

    // Battery bar: outline 28x11 at (1,19), nub at right.
    let (bx, by, bw, bh) = (1, 19, 28, 11);
    c.rect(bx, by, bw, 1, WHITE);
    c.rect(bx, by + bh - 1, bw, 1, WHITE);
    c.rect(bx, by, 1, bh, WHITE);
    c.rect(bx + bw - 1, by, 1, bh, WHITE);
    c.rect(bx + bw, by + 3, 2, bh - 6, WHITE);
    let inner = bw - 4;
    let fill = state.percent.map_or(inner, |p| (inner * p.min(100) as usize).div_ceil(100).max(1));
    c.rect(bx + 2, by + 2, fill, bh - 4, color);
    c.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * SIZE + x) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn output_is_32x32_rgba() {
        assert_eq!(render(IconState { percent: Some(50), charging: false }).len(), 32 * 32 * 4);
    }

    #[test]
    fn colours_by_level() {
        let s = |p, ch| IconState { percent: p, charging: ch };
        assert_eq!(level_color(s(Some(10), false)), RED);
        assert_eq!(level_color(s(Some(30), false)), YELLOW);
        assert_eq!(level_color(s(Some(80), false)), GREEN);
        assert_eq!(level_color(s(Some(80), true)), CYAN);
        assert_eq!(level_color(s(None, false)), GREY);
    }

    #[test]
    fn fill_scales_with_percent() {
        let low = render(IconState { percent: Some(10), charging: false });
        let full = render(IconState { percent: Some(100), charging: false });
        // x=20 inside the bar: empty at 10%, filled at 100%.
        assert_eq!(px(&low, 20, 24)[3], 0);
        assert_eq!(px(&full, 20, 24), GREEN);
    }

    #[test]
    fn hundred_renders_two_digits() {
        assert_eq!(glyphs_for(IconState { percent: Some(100), charging: false }).len(), 2);
    }
}
