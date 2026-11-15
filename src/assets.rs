//! App icon and wordmark built at start-up from the gear mark in assets/.

use eframe::egui::{self, Color32, ColorImage};

/// The gear mark cropped to a square, as grayscale (dark pixels are the mark).
fn mark() -> Option<image::GrayImage> {
    let img = image::load_from_memory(include_bytes!("../assets/logo-source.jpg"))
        .ok()?
        .to_luma8();
    let (w, h) = img.dimensions();

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[0] < 200 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }

    // Square crop around the mark, small margin included.
    let side = ((max_x - min_x + 1).max(max_y - min_y + 1) as f32 * 1.06) as u32;
    let side = side.min(w).min(h);
    let cx = (min_x + max_x) / 2;
    let cy = (min_y + max_y) / 2;
    let x0 = cx.saturating_sub(side / 2).min(w - side);
    let y0 = cy.saturating_sub(side / 2).min(h - side);

    let crop = image::imageops::crop_imm(&img, x0, y0, side, side).to_image();
    Some(image::imageops::resize(
        &crop,
        200,
        200,
        image::imageops::FilterType::Lanczos3,
    ))
}

/// Rounded orange square with the white mark, at any size.
fn icon_image(side: u32) -> Option<image::RgbaImage> {
    let mark = mark()?;
    let s = side as f32;
    let radius = s * 0.21875;
    let bg = [0xD9u8, 0x77, 0x57];
    let mut img = image::RgbaImage::new(side, side);

    for (x, y, px) in img.enumerate_pixels_mut() {
        // Distance outside the rounded square, for a one-pixel anti-aliased edge.
        let dx = (radius - x as f32)
            .max(x as f32 - (s - 1.0 - radius))
            .max(0.0);
        let dy = (radius - y as f32)
            .max(y as f32 - (s - 1.0 - radius))
            .max(0.0);
        let cover = (radius - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
        *px = image::Rgba([bg[0], bg[1], bg[2], (cover * 255.0) as u8]);
    }

    let size = ((s * 0.6875) as u32).clamp(1, side);
    let mark = image::imageops::resize(
        &mark,
        size,
        size,
        image::imageops::FilterType::Lanczos3,
    );
    let off = (side - size) / 2;
    for (x, y, p) in mark.enumerate_pixels() {
        let a = 1.0 - p[0] as f32 / 255.0;
        let px = img.get_pixel_mut(off + x, off + y);
        for c in 0..3 {
            px[c] = (px[c] as f32 + (255.0 - px[c] as f32) * a) as u8;
        }
    }

    Some(img)
}

/// Rounded orange square with the white mark: window, taskbar and alt-tab icon.
pub fn icon() -> Option<egui::IconData> {
    let img = icon_image(256)?;
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: 256,
        height: 256,
    })
}

/// Regenerates `assets/icon.ico` (the exe icon) from the gear mark:
/// `cargo test --release write_icon_ico -- --ignored`
#[test]
#[ignore]
fn write_icon_ico() {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    // Uncompressed 32-bit BMP entries: Explorer, rc.exe and GDI+ all read those.
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&s| {
            let img = icon_image(s).unwrap();
            let mask_row = ((s + 31) / 32) * 4;
            let mut out = Vec::new();
            out.extend_from_slice(&40u32.to_le_bytes());
            out.extend_from_slice(&(s as i32).to_le_bytes());
            out.extend_from_slice(&((s * 2) as i32).to_le_bytes());
            out.extend_from_slice(&1u16.to_le_bytes());
            out.extend_from_slice(&32u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&(s * s * 4).to_le_bytes());
            out.extend_from_slice(&[0; 16]);
            // XOR bitmap, bottom-up BGRA.
            for y in (0..s).rev() {
                for x in 0..s {
                    let p = img.get_pixel(x, y);
                    out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
                }
            }
            // AND mask: zeros, the alpha channel does the masking.
            out.resize(out.len() + (mask_row * s) as usize, 0);
            out
        })
        .collect();

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let dim = if *s >= 256 { 0u8 } else { *s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    std::fs::write("assets/icon.ico", out).unwrap();
}

/// Gear mark as white pixels with the darkness of the source as alpha, so it can be tinted.
pub fn logo() -> Option<ColorImage> {
    let mark = mark()?;
    let pixels = mark
        .pixels()
        .map(|p| Color32::from_white_alpha(255 - p[0]))
        .collect();
    Some(ColorImage::new([200, 200], pixels))
}
