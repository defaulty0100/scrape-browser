//! Material You (MD3) dynamic colour: seed / wallpaper -> tonal palette -> egui style.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabsPosition {
    Top,
    Side,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub seed: [u8; 3],
    pub dark: bool,
    /// Corner radius in points.
    pub radius: u8,
    pub font_scale: f32,
    pub wallpaper: Option<String>,
    /// Gaussian-ish blur of the wallpaper, in "pixels" of the low-res pass.
    pub wallpaper_blur: f32,
    /// Re-derive the seed from the wallpaper on start-up.
    pub palette_from_wallpaper: bool,
    pub tabs_position: TabsPosition,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            seed: [103, 80, 164],
            dark: false,
            radius: 16,
            font_scale: 1.0,
            wallpaper: None,
            wallpaper_blur: 0.0,
            palette_from_wallpaper: false,
            tabs_position: TabsPosition::Top,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub primary: Color32,
    pub on_primary: Color32,
    pub primary_container: Color32,
    pub on_primary_container: Color32,
    pub secondary_container: Color32,
    pub on_secondary_container: Color32,
    pub surface: Color32,
    pub surface_container_low: Color32,
    pub surface_container: Color32,
    pub surface_container_high: Color32,
    pub surface_container_highest: Color32,
    pub on_surface: Color32,
    pub on_surface_variant: Color32,
    pub outline: Color32,
    pub outline_variant: Color32,
}

impl Palette {
    pub fn from_theme(theme: &Theme) -> Self {
        let seed = effective_seed(theme).unwrap_or(theme.seed);
        Self::from_seed(seed, theme.dark)
    }

    pub fn from_seed(seed: [u8; 3], dark: bool) -> Self {
        let lab = rgb_to_lab(seed);
        let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt().max(0.001);
        let hue = (lab[1] / c, lab[2] / c);
        let primary_chroma = c.clamp(20.0, 46.0);
        let tone = |l: f32, chroma: f32| tone(hue, l, chroma);

        if dark {
            Self {
                primary: tone(80.0, primary_chroma),
                on_primary: tone(20.0, primary_chroma * 0.4),
                primary_container: tone(30.0, primary_chroma * 0.55),
                on_primary_container: tone(90.0, primary_chroma * 0.45),
                secondary_container: tone(30.0, 10.0),
                on_secondary_container: tone(90.0, 8.0),
                surface: tone(6.0, 2.0),
                surface_container_low: tone(10.0, 3.0),
                surface_container: tone(12.0, 4.0),
                surface_container_high: tone(17.0, 5.0),
                surface_container_highest: tone(22.0, 6.0),
                on_surface: tone(90.0, 4.0),
                on_surface_variant: tone(80.0, 6.0),
                outline: tone(60.0, 5.0),
                outline_variant: tone(30.0, 4.0),
            }
        } else {
            Self {
                primary: tone(40.0, primary_chroma),
                on_primary: tone(100.0, 4.0),
                primary_container: tone(90.0, primary_chroma * 0.35),
                on_primary_container: tone(10.0, primary_chroma * 0.6),
                secondary_container: tone(92.0, 10.0),
                on_secondary_container: tone(10.0, 8.0),
                surface: tone(99.0, 2.0),
                surface_container_low: tone(96.0, 3.0),
                surface_container: tone(94.0, 4.0),
                surface_container_high: tone(92.0, 5.0),
                surface_container_highest: tone(90.0, 6.0),
                on_surface: tone(10.0, 5.0),
                on_surface_variant: tone(30.0, 6.0),
                outline: tone(50.0, 5.0),
                outline_variant: tone(80.0, 4.0),
            }
        }
    }
}

/// Seed colour that the theme is actually using (wallpaper wins when enabled).
pub fn effective_seed(theme: &Theme) -> Option<[u8; 3]> {
    match (&theme.wallpaper, theme.palette_from_wallpaper) {
        (Some(path), true) => wallpaper_seed(std::path::Path::new(path)),
        _ => None,
    }
}

/// Dominant, saturated colour of an image, as a seed colour.
pub fn wallpaper_seed(path: &std::path::Path) -> Option<[u8; 3]> {
    let img = image::open(path).ok()?.to_rgb8();
    let small = image::imageops::resize(&img, 48, 48, image::imageops::FilterType::Triangle);

    // 24 hue buckets, weighted by chroma so grey areas do not win.
    let mut buckets = [[0.0f32; 2]; 24];
    let mut weights = [0.0f32; 24];
    for px in small.pixels() {
        let [l, a, b] = rgb_to_lab([px[0], px[1], px[2]]);
        let c = (a * a + b * b).sqrt();
        if c < 8.0 || l < 12.0 || l > 95.0 {
            continue;
        }
        let hue = b.atan2(a).rem_euclid(std::f32::consts::TAU);
        let i = ((hue / std::f32::consts::TAU) * 24.0) as usize % 24;
        buckets[i][0] += a * c;
        buckets[i][1] += b * c;
        weights[i] += c;
    }
    let best = (0..24).max_by(|&x, &y| weights[x].total_cmp(&weights[y]))?;
    if weights[best] == 0.0 {
        return None;
    }
    let a = buckets[best][0] / weights[best];
    let b = buckets[best][1] / weights[best];
    Some(lab_to_rgb([58.0, a, b]))
}

/// Named families: the wordmark serif, the two clock sans faces, the dot-matrix
/// stand-in for Nothing's NDot, and a user supplied clock face.
pub const SERIF: &str = "scrape-serif";
pub const SANS: &str = "scrape-sans";
pub const GEIST: &str = "scrape-geist";
pub const NDOT: &str = "scrape-ndot";
pub const CLOCK_CUSTOM: &str = "scrape-clock";

/// `scrape.clock.font`: one of the three built-in names, or a font file path.
pub fn clock_family(spec: &str) -> FontFamily {
    let s = spec.trim().to_ascii_lowercase();
    let family = if s.contains("geist") {
        GEIST
    } else if s.contains("google") {
        SANS
    } else if s.contains("anthropic") || s.is_empty() {
        SERIF
    } else if s.ends_with(".ttf") || s.ends_with(".otf") || s.contains('\\') || s.contains('/') {
        CLOCK_CUSTOM
    } else {
        SERIF
    };
    FontFamily::Name(family.into())
}

fn load_face(fonts: &mut FontDefinitions, name: &str, paths: &[&str]) -> Option<Vec<String>> {
    let bytes = paths.iter().find_map(|p| std::fs::read(p).ok())?;
    fonts
        .font_data
        .insert(name.to_owned(), Arc::new(FontData::from_owned(bytes)));
    Some(vec![name.to_owned()])
}

/// Load Segoe UI (or rough equivalents) so the UI uses system fonts by default.
pub fn install_fonts(ctx: &egui::Context, clock_font: &str) {
    let candidates: &[(&str, &str)] = &[
        ("Segoe UI", r"C:\Windows\Fonts\segoeui.ttf"),
        ("Segoe UI Semibold", r"C:\Windows\Fonts\seguisb.ttf"),
        ("Roboto", "/System/Library/Fonts/SFNS.ttf"),
        ("DejaVu Sans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
    ];
    let mut fonts = FontDefinitions::default();
    let mut loaded = 0;
    for (name, path) in candidates {
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts
            .font_data
            .insert((*name).to_owned(), Arc::new(FontData::from_owned(bytes)));
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(loaded, (*name).to_owned());
        loaded += 1;
    }

    // Anthropic Serif, Geist, NDot and Google Sans ship inside assets/fonts, so the UI
    // looks the same on a machine that has none of them installed.
    for (name, bytes) in [
        (
            "Scrape Serif",
            include_bytes!("../assets/fonts/AnthropicSerif-Display-Regular.otf").as_slice(),
        ),
        (
            "Scrape Serif Text",
            include_bytes!("../assets/fonts/AnthropicSerif-Text-Regular.otf").as_slice(),
        ),
        (
            "Scrape Geist",
            include_bytes!("../assets/fonts/Geist-Regular.ttf").as_slice(),
        ),
        (
            "Scrape Geist Medium",
            include_bytes!("../assets/fonts/Geist-Medium.ttf").as_slice(),
        ),
        (
            "Scrape Ndot",
            include_bytes!("../assets/fonts/ndot-47.otf").as_slice(),
        ),
        (
            "Scrape Sans",
            include_bytes!("../assets/fonts/GoogleSans-Regular.ttf").as_slice(),
        ),
        (
            "Scrape Sans Medium",
            include_bytes!("../assets/fonts/GoogleSans-Medium.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    let ui = "Segoe UI".to_owned();
    let serif = vec![
        "Scrape Serif".to_owned(),
        "Scrape Serif Text".to_owned(),
        ui.clone(),
    ];
    let geist = vec![
        "Scrape Geist".to_owned(),
        "Scrape Geist Medium".to_owned(),
        ui.clone(),
    ];
    let ndot = vec!["Scrape Ndot".to_owned(), ui.clone()];
    let sans = vec![
        "Scrape Sans".to_owned(),
        "Scrape Sans Medium".to_owned(),
        ui.clone(),
    ];

    let spec = clock_font.trim();
    let custom = if spec.ends_with(".ttf") || spec.ends_with(".otf") {
        load_face(&mut fonts, "Scrape Clock", &[spec]).unwrap_or_else(|| serif.clone())
    } else {
        serif.clone()
    };

    fonts.families.insert(FontFamily::Name(SERIF.into()), serif);
    fonts.families.insert(FontFamily::Name(SANS.into()), sans);
    fonts.families.insert(FontFamily::Name(GEIST.into()), geist);
    fonts.families.insert(FontFamily::Name(NDOT.into()), ndot);
    fonts
        .families
        .insert(FontFamily::Name(CLOCK_CUSTOM.into()), custom);

    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context, pal: &Palette, theme: &Theme) {
    let r = theme.radius as u8;
    let mut style = egui::Style::default();
    let mut v = if theme.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };

    v.panel_fill = pal.surface;
    v.window_fill = pal.surface_container_low;
    v.extreme_bg_color = pal.surface_container_high;
    v.faint_bg_color = pal.surface_container_low;
    v.override_text_color = Some(pal.on_surface);
    v.hyperlink_color = pal.primary;
    v.selection.bg_fill = pal.primary_container;
    v.selection.stroke = Stroke::new(1.0, pal.on_primary_container);
    v.window_corner_radius = CornerRadius::same(r);
    v.menu_corner_radius = CornerRadius::same(r);
    v.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(if theme.dark { 80 } else { 26 }),
    };
    v.popup_shadow = v.window_shadow;

    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(r.min(20));
    }
    v.widgets.noninteractive.bg_fill = pal.surface_container;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, pal.outline_variant);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, pal.on_surface_variant);
    v.widgets.inactive.weak_bg_fill = pal.surface_container_high;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, pal.outline_variant);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, pal.on_surface);
    v.widgets.hovered.weak_bg_fill = pal.surface_container_highest;
    v.widgets.hovered.bg_fill = pal.surface_container_highest;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, pal.outline);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, pal.on_surface);
    v.widgets.active.weak_bg_fill = pal.secondary_container;
    v.widgets.active.bg_fill = pal.secondary_container;
    v.widgets.active.fg_stroke = Stroke::new(1.5, pal.on_secondary_container);
    v.widgets.open.weak_bg_fill = pal.surface_container_highest;

    style.visuals = v;
    style.animation_time = 0.22;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 7.0);
    style.spacing.interact_size.y = 32.0;

    let s = theme.font_scale;
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(22.0 * s)),
        (TextStyle::Body, FontId::proportional(14.0 * s)),
        (TextStyle::Button, FontId::proportional(14.0 * s)),
        (TextStyle::Small, FontId::proportional(12.0 * s)),
        (TextStyle::Monospace, FontId::monospace(13.0 * s)),
    ]
    .into();

    ctx.set_global_style(style);
}

/// CIELAB tone: L is MD3's "tone", (a,b) are the seed hue scaled to `chroma`.
fn tone(hue: (f32, f32), l: f32, chroma: f32) -> Color32 {
    let mut c = chroma;
    // Reduce chroma until the colour fits in sRGB.
    for _ in 0..10 {
        let lab = [l, hue.0 * c, hue.1 * c];
        if lab_to_linear(lab).iter().all(|x| *x >= -0.001 && *x <= 1.001) {
            break;
        }
        c *= 0.72;
    }
    let rgb = lab_to_rgb([l, hue.0 * c, hue.1 * c]);
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn f_inv(t: f32) -> f32 {
    if t * t * t > 0.008856 {
        t * t * t
    } else {
        (t - 16.0 / 116.0) / 7.787
    }
}

fn f_fwd(t: f32) -> f32 {
    if t > 0.008856 {
        t.cbrt()
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

fn lab_to_linear([l, a, b]: [f32; 3]) -> [f32; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let x = 0.95047 * f_inv(fx);
    let y = f_inv(fy);
    let z = 1.08883 * f_inv(fz);
    [
        3.2404542 * x - 1.5371385 * y - 0.4985314 * z,
        -0.9692660 * x + 1.8760108 * y + 0.0415560 * z,
        0.0556434 * x - 0.2040259 * y + 1.0572252 * z,
    ]
}

fn lab_to_rgb(lab: [f32; 3]) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (i, lin) in lab_to_linear(lab).iter().enumerate() {
        out[i] = (lin_to_srgb(lin.clamp(0.0, 1.0)) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn rgb_to_lab(rgb: [u8; 3]) -> [f32; 3] {
    let r = srgb_to_lin(rgb[0] as f32 / 255.0);
    let g = srgb_to_lin(rgb[1] as f32 / 255.0);
    let b = srgb_to_lin(rgb[2] as f32 / 255.0);
    let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
    let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
    let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
    let (fx, fy, fz) = (f_fwd(x / 0.95047), f_fwd(y), f_fwd(z / 1.08883));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn srgb_to_lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn lin_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_round_trip() {
        for seed in [[0u8, 0, 0], [255, 255, 255], [103, 80, 164], [230, 90, 40]] {
            let back = lab_to_rgb(rgb_to_lab(seed));
            for i in 0..3 {
                assert!(
                    (back[i] as i32 - seed[i] as i32).abs() <= 2,
                    "{seed:?} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn palette_has_contrast() {
        for dark in [false, true] {
            let p = Palette::from_seed([103, 80, 164], dark);
            let lum = |c: Color32| {
                0.2126 * srgb_to_lin(c.r() as f32 / 255.0)
                    + 0.7152 * srgb_to_lin(c.g() as f32 / 255.0)
                    + 0.0722 * srgb_to_lin(c.b() as f32 / 255.0)
            };
            let (fg, bg) = (lum(p.on_surface), lum(p.surface));
            assert!((fg - bg).abs() > 0.5, "text/surface too close: {fg} {bg}");
        }
    }
}
