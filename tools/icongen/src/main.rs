//! Turns the artwork in assets/pericore-source.webp into assets/pericore.ico
//! (16-256 px), assets/pericore.png (256 px) and assets/pericore-64.rgba.
//! Run from the repo root:
//!   cargo run --release --manifest-path tools/icongen/Cargo.toml
//!
//! The artwork is a rounded square on a light glow. The square is found by
//! its dark pixels, cropped, and given clean transparent rounded corners.
//! 16-32 px use a close crop of the centre "P" so they stay readable.

use image::{imageops::FilterType, Rgba, RgbaImage};
use std::fs::File;

const SOURCE: &str = "assets/pericore-source.webp";
const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];
/// Corner radius as a fraction of the side.
const RADIUS: f32 = 0.22;
/// Fraction of the side kept around the centre for the tiny sizes.
const SMALL_CROP: f32 = 0.5;

fn is_dark(p: &Rgba<u8>) -> bool {
    let [r, g, b, a] = p.0;
    a > 200 && (u32::from(r) + u32::from(g) + u32::from(b)) < 3 * 110
}

/// Bounding box of the dark rounded square: rows/columns that are mostly dark.
fn square_bounds(img: &RgbaImage) -> (u32, u32, u32) {
    let (w, h) = img.dimensions();
    let dark_cols: Vec<u32> = (0..w).filter(|&x| (0..h).filter(|&y| is_dark(img.get_pixel(x, y))).count() as u32 > h / 2).collect();
    let dark_rows: Vec<u32> = (0..h).filter(|&y| (0..w).filter(|&x| is_dark(img.get_pixel(x, y))).count() as u32 > w / 2).collect();
    let (x0, x1) = (dark_cols[0], *dark_cols.last().unwrap());
    let (y0, y1) = (dark_rows[0], *dark_rows.last().unwrap());
    let side = (x1 - x0).min(y1 - y0);
    // Centre a square on the detected box.
    let cx = (x0 + x1) / 2;
    let cy = (y0 + y1) / 2;
    (cx - side / 2, cy - side / 2, side)
}

/// Coverage (0..1) of pixel centre (x, y) inside a rounded square of side n.
fn coverage(x: f32, y: f32, n: f32) -> f32 {
    let r = RADIUS * n;
    let dx = (r - x).max(x - (n - r)).max(0.0);
    let dy = (r - y).max(y - (n - r)).max(0.0);
    let dist = (dx * dx + dy * dy).sqrt() - r;
    (0.5 - dist).clamp(0.0, 1.0)
}

fn rounded(mut img: RgbaImage) -> RgbaImage {
    let n = img.width() as f32;
    for (x, y, p) in img.enumerate_pixels_mut() {
        let c = coverage(x as f32 + 0.5, y as f32 + 0.5, n);
        p.0[3] = (f32::from(p.0[3]) * c).round() as u8;
    }
    img
}

fn render(square: &RgbaImage, size: u32) -> RgbaImage {
    let src = if size <= 32 {
        let side = (square.width() as f32 * SMALL_CROP) as u32;
        let off = (square.width() - side) / 2;
        image::imageops::crop_imm(square, off, off, side, side).to_image()
    } else {
        square.clone()
    };
    // Resize first, then cut the corners at the final size for crisp edges.
    rounded(image::imageops::resize(&src, size, size, FilterType::Lanczos3))
}

fn main() {
    let img = image::open(SOURCE).expect("run from the repo root").to_rgba8();
    let (x, y, side) = square_bounds(&img);
    // Trim a little so the glow at the edge never shows.
    let inset = side / 100;
    let square = image::imageops::crop_imm(&img, x + inset, y + inset, side - 2 * inset, side - 2 * inset).to_image();

    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let icon = ico::IconImage::from_rgba_data(size, size, render(&square, size).into_raw());
        dir.add_entry(ico::IconDirEntry::encode(&icon).expect("encode icon"));
    }
    dir.write(File::create("assets/pericore.ico").expect("create ico")).expect("write ico");
    render(&square, 256).save("assets/pericore.png").expect("write png");
    // Raw straight-alpha RGBA for the window icon (no decoder needed at runtime).
    std::fs::write("assets/pericore-64.rgba", render(&square, 64).into_raw()).expect("write rgba");
    println!("square at ({x}, {y}) side {side}; wrote assets/pericore.ico, pericore.png, pericore-64.rgba");
}
