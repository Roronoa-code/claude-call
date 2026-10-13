//! Builds phone/ui/null.png and null-small.png (Null, Codex's pet) from the Codex app's sprite sheet.
//!
//! The sheet's art is noisy at the edges (lossy compression of a resampled render), so every frame is redrawn with
//! clean, smooth edges: its outline is traced at 4x (scaled up, softened, re-thresholded) and scaled down to the exact
//! pixel size the phone draws it at, so nothing is resampled again on the phone.
//!   cargo run --release --example null_atlas -- <null-signal-spritesheet.webp>
use image::{
    GrayImage, Rgba, RgbaImage,
    imageops::{self, FilterType},
};

/// (row in the Codex sheet, frames, px it is drawn too low): idle, waving, jumping, waiting ("?"), running (laptop), review.
const ROWS: [(u32, u32, u32); 6] = [(0, 6, 0), (3, 4, 0), (4, 5, 0), (6, 6, 0), (7, 6, 0), (8, 6, 5)];
const CELL_W: u32 = 144; // the part of each 192 x 208 cell Null uses
const CELL_H: u32 = 208;
const X0: u32 = 24;
/// Output file and physical px per sheet px (the call screen draws them 1:1).
const SIZES: [(&str, f32); 2] = [("null.png", 1.75), ("null-small.png", 1.0)];
const UP: u32 = 4;

/// Premultiplied alpha, so scaling never pulls in the colour of see-through pixels (no light fringe).
fn premultiply(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            p[c] = (p[c] as f32 * a).round() as u8;
        }
    }
}

fn unpremultiply(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            p[c] = if a > 0.0 { (p[c] as f32 / a).round().min(255.0) as u8 } else { 0 };
        }
    }
}

fn clean(cell: &RgbaImage, scale: f32) -> RgbaImage {
    let (w, h) = cell.dimensions();
    let mut pm = cell.clone();
    premultiply(&mut pm);
    let mut colour = imageops::resize(&pm, w * UP, h * UP, FilterType::CatmullRom);
    unpremultiply(&mut colour);
    let alpha = GrayImage::from_fn(w, h, |x, y| image::Luma([cell.get_pixel(x, y)[3]]));
    let alpha = imageops::blur(&imageops::resize(&alpha, w * UP, h * UP, FilterType::CatmullRom), UP as f32 * 0.7);
    for (p, a) in colour.pixels_mut().zip(alpha.pixels()) {
        let a = ((a[0] as f32 / 255.0 - 0.5) * 6.0 + 0.5).clamp(0.0, 1.0); // a crisp outline again, now smooth
        p[3] = (a * 255.0).round() as u8;
    }
    premultiply(&mut colour);
    let mut out = imageops::resize(&colour, (w as f32 * scale).round() as u32, (h as f32 * scale).round() as u32, FilterType::Lanczos3);
    unpremultiply(&mut out);
    out
}

fn main() {
    let src = std::env::args().nth(1).expect("usage: null_atlas <null-signal-spritesheet.webp>");
    let sheet = image::open(src).expect("sprite sheet").to_rgba8();
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("phone/ui");
    for (name, scale) in SIZES {
        let (cw, ch) = ((CELL_W as f32 * scale).round() as u32, (CELL_H as f32 * scale).round() as u32);
        let mut atlas = RgbaImage::from_pixel(cw * 6, ch * ROWS.len() as u32, Rgba([0, 0, 0, 0]));
        for (i, (row, frames, dy)) in ROWS.into_iter().enumerate() {
            for c in 0..frames {
                let cell = imageops::crop_imm(&sheet, c * 192 + X0, row * 208 + dy, CELL_W, CELL_H).to_image();
                imageops::replace(&mut atlas, &clean(&cell, scale), (c * cw) as i64, (i as u32 * ch) as i64);
            }
        }
        atlas.save(ui.join(name)).expect("save");
        println!("{name} {}x{} cell {cw}x{ch}", atlas.width(), atlas.height());
    }
}
