//! Wallpaper texture: static images plus animated GIF/WebP, with an optional blur.

use crate::theme::Theme;
use eframe::egui::{self, TextureHandle, TextureId, Vec2};
use image::AnimationDecoder;
use std::io::BufReader;
use std::time::{Duration, Instant};

struct Frame {
    tex: TextureHandle,
    at: Duration,
}

pub struct Wallpaper {
    key: String,
    blur: f32,
    frames: Vec<Frame>,
    total: Duration,
    started: Instant,
}

impl Default for Wallpaper {
    fn default() -> Self {
        Self {
            key: String::new(),
            blur: 0.0,
            frames: Vec::new(),
            total: Duration::ZERO,
            started: Instant::now(),
        }
    }
}

impl Wallpaper {
    /// Current frame (if any) and its size. Animated files ask for another repaint.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        theme: &Theme,
    ) -> Option<(TextureId, Vec2)> {
        let path = theme.wallpaper.as_ref()?;
        if self.key != *path || self.blur != theme.wallpaper_blur {
            self.load(ctx, path, theme.wallpaper_blur);
        }
        let first = self.frames.first()?;
        if self.frames.len() < 2 || self.total.is_zero() {
            return Some((first.tex.id(), first.tex.size_vec2()));
        }

        let total = self.total.as_millis().max(1) as u64;
        let elapsed = self.started.elapsed().as_millis() as u64 % total;
        let now = Duration::from_millis(elapsed);
        let idx = self
            .frames
            .iter()
            .rposition(|f| f.at <= now)
            .unwrap_or_default();
        let frame = &self.frames[idx];
        let next = self.frames.get(idx + 1).map(|f| f.at).unwrap_or(self.total);
        ctx.request_repaint_after((next - now).clamp(Duration::from_millis(5), Duration::from_millis(50)));
        Some((frame.tex.id(), frame.tex.size_vec2()))
    }

    fn load(&mut self, ctx: &egui::Context, path: &str, radius: f32) {
        self.key = path.to_owned();
        self.blur = radius;
        self.frames.clear();
        self.total = Duration::ZERO;
        self.started = Instant::now();

        let Some(frames) = decode(path) else { return };
        let mut at = Duration::ZERO;
        for (i, (img, delay)) in frames.into_iter().enumerate() {
            let img = blur(fit(img), radius);
            let color =
                egui::ColorImage::from_rgba_unmultiplied(
                    [img.width() as usize, img.height() as usize],
                    img.as_raw(),
                );
            let tex = ctx.load_texture(
                format!("wallpaper-{i}"),
                color,
                egui::TextureOptions::LINEAR,
            );
            self.frames.push(Frame { tex, at });
            at += Duration::from_millis(delay as u64);
        }
        self.total = at;
    }
}

/// Decoded frames with their delay in milliseconds. Static files give one frame.
fn decode(path: &str) -> Option<Vec<(image::RgbaImage, u32)>> {
    let low = path.to_ascii_lowercase();
    let mut out: Vec<(image::RgbaImage, u32)> = Vec::new();

    if low.ends_with(".gif") || low.ends_with(".webp") {
        let file = std::fs::File::open(path).ok()?;
        let reader = BufReader::new(file);
        if low.ends_with(".gif") {
            let dec = image::codecs::gif::GifDecoder::new(reader).ok()?;
            for frame in dec.into_frames().take(240) {
                let Ok(frame) = frame else { continue };
                let (num, den) = frame.delay().numer_denom_ms();
                out.push((frame.into_buffer(), num / den.max(1)));
            }
        } else {
            let dec = image::codecs::webp::WebPDecoder::new(reader).ok()?;
            for frame in dec.into_frames().take(240) {
                let Ok(frame) = frame else { continue };
                let (num, den) = frame.delay().numer_denom_ms();
                out.push((frame.into_buffer(), num / den.max(1)));
            }
        }
    }

    if out.is_empty() {
        out.push((image::open(path).ok()?.to_rgba8(), 0));
    }
    Some(out)
}

/// Keeps a 4K wallpaper from eating texture memory.
fn fit(img: image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let scale = (1400.0 / w as f32).min(1.0);
    if scale >= 1.0 {
        return img;
    }
    image::imageops::resize(
        &img,
        (w as f32 * scale) as u32,
        (h as f32 * scale) as u32,
        image::imageops::FilterType::Triangle,
    )
}

/// Cheap blur: shrink, grow back, repeat once for larger radii.
fn blur(img: image::RgbaImage, amount: f32) -> image::RgbaImage {
    if amount < 0.5 {
        return img;
    }
    let factor = (1.0 + amount * 0.25).min(24.0);
    let w = ((img.width() as f32 / factor) as u32).max(1);
    let h = ((img.height() as f32 / factor) as u32).max(1);
    let mut out = img;
    for _ in 0..2 {
        let small = image::imageops::resize(&out, w, h, image::imageops::FilterType::Triangle);
        out = image::imageops::resize(
            &small,
            out.width(),
            out.height(),
            image::imageops::FilterType::Triangle,
        );
    }
    out
}
