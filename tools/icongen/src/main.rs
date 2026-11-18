//! Renders assets/pericore.svg to assets/pericore.ico (16-256 px) and
//! assets/pericore.png (256 px). Run from the repo root:
//!   cargo run --manifest-path tools/icongen/Cargo.toml

use resvg::{tiny_skia, usvg};
use std::fs::File;

const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

fn render(tree: &usvg::Tree, size: u32) -> tiny_skia::Pixmap {
    let mut pix = tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let scale = size as f32 / tree.size().width();
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pix.as_mut());
    pix
}

/// tiny-skia stores premultiplied alpha; ICO/PNG want straight alpha.
fn unpremultiply(pix: &tiny_skia::Pixmap) -> Vec<u8> {
    pix.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

fn main() {
    let svg = std::fs::read("assets/pericore.svg").expect("run from repo root");
    let tree = usvg::Tree::from_data(&svg, &usvg::Options::default()).expect("valid svg");

    let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let rgba = unpremultiply(&render(&tree, size));
        let img = ico::IconImage::from_rgba_data(size, size, rgba);
        dir.add_entry(ico::IconDirEntry::encode(&img).expect("encode icon"));
    }
    dir.write(File::create("assets/pericore.ico").expect("create ico")).expect("write ico");

    render(&tree, 256).save_png("assets/pericore.png").expect("write png");
    // Raw straight-alpha RGBA for the window icon (no PNG decoder needed at runtime).
    std::fs::write("assets/pericore-64.rgba", unpremultiply(&render(&tree, 64))).expect("write rgba");
    println!("wrote assets/pericore.ico, pericore.png, pericore-64.rgba");
}
