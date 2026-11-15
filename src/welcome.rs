//! Pieces of the welcome screen: splash phrase, clock, logo variants, Claude
//! phrase strip and the two-style todo widget. Main decides where they sit.

use crate::config::{Clock, ClockOrientation, Config, ItemKind, LogoPreference, SplashMode, TodoItem, TodoStyle, Welcome};
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Sense, Stroke, Ui,
    Vec2,
};

pub fn fade(color: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), (a * 255.0) as u8)
}

// ---------------------------------------------------------------- splash

const BASE_PHRASES: &[&str] = &[
    "a small browser that asks before it acts.",
    "what is on your mind?",
    "where shall we go today?",
    "type a thought, get a tab.",
    "one quiet tab at a time.",
];

const MORNING: &[&str] = &["morning. coffee first, then tabs.", "early bird, empty cache."];
const AFTERNOON: &[&str] = &["afternoon. mind the tabs.", "the slow part of the day."];
const EVENING: &[&str] = &["evening. close a few tabs.", "dusk browsing."];
const NIGHT: &[&str] = &["late night jam session.", "go to sleep already.", "3 a.m. is a country."];

fn time_of_day(hour: u32) -> &'static [&'static str] {
    match hour {
        5..=11 => MORNING,
        12..=17 => AFTERNOON,
        18..=22 => EVENING,
        _ => NIGHT,
    }
}

/// Phrase for this visit, picked from the visit seed so it never rotates on screen.
pub fn splash(cfg: &Config, seed: f32, hour: u32) -> Option<String> {
    let pool: Vec<String> = match cfg.splash.mode {
        SplashMode::Off => return None,
        SplashMode::Custom => cfg
            .splash
            .custom_splash
            .iter()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect(),
        SplashMode::On => time_of_day(hour)
            .iter()
            .chain(BASE_PHRASES)
            .map(|s| (*s).to_owned())
            .collect(),
    };
    if pool.is_empty() {
        return None;
    }
    Some(pool[(seed as usize) % pool.len()].clone())
}

pub fn paint_splash(
    painter: &Painter,
    rect: Rect,
    cfg: &Config,
    pal: &Palette,
    seed: f32,
    alpha: f32,
) {
    let (text, color) = match splash(cfg, seed, now().0) {
        Some(text) => (text, fade(pal.on_surface_variant, alpha)),
        None => ("splash is off".to_owned(), fade(pal.outline, alpha)),
    };
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::new((rect.height() * 0.60).max(12.5), FontFamily::Name(theme::SANS.into())),
        color,
    );
}

// ---------------------------------------------------------------- clock

#[cfg(windows)]
#[repr(C)]
struct SysTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(t: *mut SysTime);
}

/// Local wall-clock time, no timezone crate needed.
pub fn now() -> (u32, u32, u32) {
    #[cfg(windows)]
    unsafe {
        let mut t: SysTime = std::mem::zeroed();
        GetLocalTime(&mut t);
        (t.hour as u32, t.minute as u32, t.second as u32)
    }
    #[cfg(not(windows))]
    {
        let s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        (((s / 3600 + 3) % 24) as u32, ((s / 60) % 60) as u32, (s % 60) as u32)
    }
}

pub fn paint_clock(painter: &Painter, rect: Rect, cfg: &Clock, pal: &Palette, alpha: f32) {
    let (h, m, s) = now();
    let family = theme::clock_family(&cfg.font);
    let color = fade(pal.on_surface, alpha);
    let colon = fade(pal.on_surface_variant, alpha);
    let nums: Vec<String> = if cfg.seconds {
        vec![format!("{h:02}"), format!("{m:02}"), format!("{s:02}")]
    } else {
        vec![format!("{h:02}"), format!("{m:02}")]
    };

    match cfg.orientation {
        ClockOrientation::Horizontal => {
            let text = nums.join(":");
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::new(rect.height() * 0.8, family),
                color,
            );
        }
        ClockOrientation::Vertical => {
            // Numbers stacked with a colon between each pair.
            let gaps = nums.len() as f32 - 1.0;
            let unit = rect.height() / (nums.len() as f32 + gaps * 0.5);
            let mut y = rect.top();
            for (i, num) in nums.iter().enumerate() {
                painter.text(
                    Pos2::new(rect.center().x, y + unit / 2.0),
                    Align2::CENTER_CENTER,
                    num,
                    FontId::new(unit * 0.95, family.clone()),
                    color,
                );
                y += unit;
                if i + 1 < nums.len() {
                    // Two dots side by side: a colon rotated with the stacked digits.
                    let r = (unit * 0.05).max(1.6);
                    let dx = (unit * 0.15).max(4.0);
                    let cy = y + unit * 0.25;
                    for side in [-1.0, 1.0] {
                        painter.circle_filled(
                            Pos2::new(rect.center().x + side * dx, cy),
                            r,
                            colon,
                        );
                    }
                    y += unit * 0.5;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- logo

/// "Scrape" in the three requested shapes: gear-R, plain text, gear above text.
pub fn paint_logo(
    painter: &Painter,
    rect: Rect,
    pref: LogoPreference,
    logo: Option<&egui::TextureHandle>,
    color: Color32,
) {
    let serif = FontFamily::Name(theme::SERIF.into());
    match pref {
        LogoPreference::Plain => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Scrape",
                FontId::new(rect.height() * 0.62, serif),
                color,
            );
        }
        LogoPreference::Basic => {
            let size = rect.height() * 0.62;
            let font = FontId::new(size, serif);
            let left = painter.layout_no_wrap("Sc".to_owned(), font.clone(), color);
            let right = painter.layout_no_wrap("ape".to_owned(), font.clone(), color);
            let gear = size * 0.66;
            let gap = size * 0.03;
            let total = left.size().x + right.size().x + gear + gap * 2.0;
            let x0 = rect.center().x - total / 2.0;
            let cy = rect.center().y;
            painter.galley(Pos2::new(x0, cy - left.size().y / 2.0), left.clone(), color);
            paint_gear(
                painter,
                Rect::from_center_size(
                    Pos2::new(x0 + left.size().x + gap + gear / 2.0, cy + size * 0.07),
                    Vec2::splat(gear),
                ),
                logo,
                color,
            );
            painter.galley(
                Pos2::new(
                    x0 + left.size().x + gap + gear + gap,
                    cy - right.size().y / 2.0,
                ),
                right,
                color,
            );
        }
        LogoPreference::Big => {
            let gear = rect.height() * 0.52;
            paint_gear(
                painter,
                Rect::from_min_size(
                    Pos2::new(rect.center().x - gear / 2.0, rect.top()),
                    Vec2::splat(gear),
                ),
                logo,
                color,
            );
            painter.text(
                Pos2::new(rect.center().x, rect.top() + gear + rect.height() * 0.04),
                Align2::CENTER_TOP,
                "Scrape",
                FontId::new(rect.height() * 0.30, serif),
                color,
            );
        }
    }
}

fn paint_gear(
    painter: &Painter,
    rect: Rect,
    logo: Option<&egui::TextureHandle>,
    color: Color32,
) {
    match logo {
        Some(logo) => {
            painter.image(
                logo.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                color,
            );
        }
        None => {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "r",
                FontId::new(rect.height(), FontFamily::Name(theme::SERIF.into())),
                color,
            );
        }
    }
}

// ---------------------------------------------------------------- phrases

const STRIP: &[&str] = &[
    "thinking",
    "baking",
    "ultrathink",
    "noodling",
    "brewing",
    "percolating",
];

const CLAUDE_ORANGE: Color32 = Color32::from_rgb(0xD9, 0x77, 0x57);

pub fn paint_phrases(
    painter: &Painter,
    rect: Rect,
    pal: &Palette,
    seed: f32,
    t: f32,
    alpha: f32,
) {
    // One phrase per visit; only the star keeps moving.
    let idx = (seed as usize) % STRIP.len();
    let a = alpha;

    painter.rect_filled(
        rect,
        CornerRadius::same((rect.height() / 2.0) as u8),
        fade(pal.surface_container_high, a),
    );

    let font = FontId::new(
        (rect.height() * 0.33).max(9.5),
        FontFamily::Name(theme::SANS.into()),
    );
    let galley = painter.layout_no_wrap(
        STRIP[idx].to_owned(),
        font,
        fade(pal.on_surface_variant, a),
    );
    let star_r = rect.height() * 0.16;
    let gap = rect.height() * 0.25;
    let total = star_r * 2.0 + gap + galley.size().x;
    let x0 = rect.center().x - total / 2.0;
    let cy = rect.center().y;
    star(
        painter,
        Pos2::new(x0 + star_r, cy),
        star_r,
        t * 1.6,
        fade(CLAUDE_ORANGE, a),
    );
    painter.galley(
        Pos2::new(x0 + star_r * 2.0 + gap, cy - galley.size().y / 2.0),
        galley,
        fade(pal.on_surface_variant, a),
    );
}

/// Claude's mark: a sunburst of thin spokes of uneven length, spinning.
pub fn star(painter: &Painter, center: Pos2, radius: f32, angle: f32, color: Color32) {
    let width = (radius * 0.22).clamp(1.0, 2.6);
    let stroke = Stroke::new(width, color);
    let spokes = 12;
    for k in 0..spokes {
        let a = angle + k as f32 * std::f32::consts::TAU / spokes as f32;
        // Uneven reach, like the original: long ray, short ray, long ray.
        let r = radius * if k % 3 == 1 { 0.66 } else { 1.0 };
        let dir = Vec2::angled(a);
        painter.line_segment([center - dir * r, center + dir * r], stroke);
    }
}

/// Agent mark: a circle with two pill eyes.
pub fn agent_glyph(painter: &Painter, rect: Rect, color: Color32) {
    let r = rect.width().min(rect.height()) * 0.42;
    let c = rect.center();
    painter.circle_stroke(c, r, Stroke::new((r * 0.16).max(1.2), color));
    let eye_w = (r * 0.30).max(1.8);
    let half = (r * 0.22).max(1.0);
    for side in [-1.0f32, 1.0] {
        let x = c.x + side * r * 0.36;
        painter.line_segment(
            [Pos2::new(x, c.y - half), Pos2::new(x, c.y + half)],
            Stroke::new(eye_w, color),
        );
    }
}

// ---------------------------------------------------------------- todo

/// Draws the card and returns true when a checkbox was toggled.
pub fn todo_ui(
    ui: &mut Ui,
    rect: Rect,
    style: TodoStyle,
    todos: &mut [TodoItem],
    pal: &Palette,
    setup: bool,
    alpha: f32,
) -> bool {
    let dark = pal.surface.r() < 128;
    let (card, border, title, body, dim, accent) = match (style, dark) {
        (TodoStyle::Nothing, _) => (
            Color32::from_rgb(0x0E, 0x0E, 0x0E),
            Some(Color32::from_rgb(0x2C, 0x2C, 0x2C)),
            Color32::WHITE,
            Color32::from_rgb(0xE8, 0xE8, 0xE8),
            Color32::from_rgb(0x6E, 0x6E, 0x6E),
            Color32::from_rgb(0xD7, 0x19, 0x21),
        ),
        (TodoStyle::Claude, true) => (
            Color32::from_rgb(0x26, 0x26, 0x24),
            None,
            Color32::from_rgb(0xED, 0xE9, 0xDE),
            Color32::from_rgb(0xD9, 0xD5, 0xCB),
            Color32::from_rgb(0x7F, 0x7A, 0x6E),
            CLAUDE_ORANGE,
        ),
        (TodoStyle::Claude, false) => (
            Color32::from_rgb(0xF2, 0xEF, 0xE8),
            None,
            Color32::from_rgb(0x3D, 0x39, 0x29),
            Color32::from_rgb(0x4A, 0x46, 0x3C),
            Color32::from_rgb(0xA3, 0x9E, 0x92),
            CLAUDE_ORANGE,
        ),
    };

    // Everything inside the card follows its height, so the layout scales with the window.
    let k = rect.height() / 240.0;
    let pad = 16.0 * k;
    let header_font = match style {
        TodoStyle::Nothing => FontId::new(15.0 * k, FontFamily::Name(theme::NDOT.into())),
        TodoStyle::Claude => FontId::new(20.0 * k, FontFamily::Name(theme::SERIF.into())),
    };
    let body_font = match style {
        TodoStyle::Nothing => FontId::new(12.5 * k, FontFamily::Name(theme::NDOT.into())),
        TodoStyle::Claude => FontId::new(13.5 * k, FontFamily::Proportional),
    };
    let open = todos.iter().filter(|t| !t.done).count();

    let mut toggled = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.multiply_opacity(alpha);
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same((18.0 * k) as u8), card);
        if let Some(border) = border {
            p.rect_stroke(
                rect,
                CornerRadius::same((18.0 * k) as u8),
                Stroke::new(1.0, border),
                egui::StrokeKind::Inside,
            );
        }

        let heading = match style {
            TodoStyle::Nothing => "TODO",
            TodoStyle::Claude => "Today",
        };
        p.text(
            Pos2::new(rect.left() + pad, rect.top() + pad - 2.0),
            Align2::LEFT_TOP,
            heading,
            header_font,
            title,
        );
        if style == TodoStyle::Nothing {
            p.circle_filled(
                Pos2::new(rect.left() + pad + 48.0 * k, rect.top() + pad + 6.0 * k),
                4.0 * k,
                accent,
            );
        }
        p.text(
            Pos2::new(rect.right() - pad, rect.top() + pad + 4.0 * k),
            Align2::RIGHT_TOP,
            format!("{open} open"),
            FontId::new(11.0 * k, FontFamily::Proportional),
            dim,
        );

        let inner = Rect::from_min_max(
            Pos2::new(rect.left() + pad - 4.0 * k, rect.top() + pad + 34.0 * k),
            Pos2::new(rect.right() - pad + 4.0 * k, rect.bottom() - pad + 4.0 * k),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 3.0 * k;
                    if todos.is_empty() {
                        ui.label(
                            egui::RichText::new("nothing here yet")
                                .font(body_font.clone())
                                .color(dim),
                        );
                    }
                    for (i, item) in todos.iter_mut().enumerate() {
                        let width = ui.available_width();
                        let (row, resp) =
                            ui.allocate_exact_size(Vec2::new(width, 27.0 * k), Sense::click());
                        let center = Pos2::new(row.left() + 8.0 * k, row.center().y);
                        if item.done {
                            ui.painter().circle_filled(center, 6.5 * k, accent);
                        } else {
                            ui.painter().circle_stroke(
                                center,
                                6.5 * k,
                                Stroke::new(1.4 * k, if resp.hovered() { accent } else { dim }),
                            );
                        }
                        let galley = ui.painter().layout_no_wrap(
                            item.text.clone(),
                            body_font.clone(),
                            if item.done { dim } else { body },
                        );
                        let pos = Pos2::new(
                            row.left() + 24.0 * k,
                            row.center().y - galley.size().y / 2.0,
                        );
                        ui.painter().galley(pos, galley.clone(), if item.done { dim } else { body });
                        if item.done {
                            ui.painter().line_segment(
                                [
                                    Pos2::new(pos.x, row.center().y),
                                    Pos2::new(pos.x + galley.size().x, row.center().y),
                                ],
                                Stroke::new(1.0, dim),
                            );
                        }
                        if resp.clicked() && !setup {
                            item.done = !item.done;
                            toggled = true;
                        }
                        let _ = i;
                    }
                });
        });
    });
    toggled
}

// ---------------------------------------------------------------- sites

const SITE_CELL: f32 = 54.0;
const SITE_GAP: f32 = 8.0;
const SITE_PAD: f32 = 12.0;

/// Grid size for the configured rows and columns. One cell is one favourite.
pub fn sites_size(rows: u8, cols: u8) -> Vec2 {
    let r = rows.clamp(1, 8) as f32;
    let c = cols.clamp(1, 8) as f32;
    Vec2::new(
        c * SITE_CELL + (c - 1.0) * SITE_GAP + SITE_PAD * 2.0,
        r * SITE_CELL + (r - 1.0) * SITE_GAP + SITE_PAD * 2.0,
    )
}

fn site_label(url: &str) -> String {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(url);
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    let mut i = 0;
    if parts.first() == Some(&"www") {
        i = 1;
    }
    // Language and mobile prefixes: en.wikipedia.org, m.youtube.com.
    if parts.get(i).is_some_and(|p| p.len() <= 2) && parts.len() > i + 2 {
        i += 1;
    }
    parts.get(i).copied().unwrap_or(host).to_owned()
}

/// Sites that fit in the grid: every cell is a site but the last one is the add button.
pub fn site_cap(rows: u8, cols: u8) -> usize {
    (rows.clamp(1, 8) as usize * cols.clamp(1, 8) as usize)
        .saturating_sub(1)
        .max(1)
}

/// What the favourites widget was asked to do this frame.
pub enum SitesHit {
    Open(String),
    Add,
}

/// Favourite tiles in a scrollable grid. One cell is always the add button, so the
/// widget holds at most `rows * cols - 1` sites.
pub fn sites_ui(
    ui: &mut Ui,
    rect: Rect,
    sites: &[String],
    rows: u8,
    cols: u8,
    pal: &Palette,
    setup: bool,
    mut icon: impl FnMut(&str) -> Option<egui::TextureId>,
) -> Option<SitesHit> {
    let base = sites_size(rows, cols);
    let k = (rect.height() / base.y.max(1.0)).max(0.01);
    let pad = SITE_PAD * k;
    let gap = SITE_GAP * k;
    let cell = SITE_CELL * k;
    let radius = (14.0 * k).clamp(3.0, 18.0) as u8;
    let cols = cols.clamp(1, 8) as usize;
    let cap = site_cap(rows as u8, cols as u8);
    let mut clicked = None;

    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(radius), pal.surface_container_low);
        p.rect_stroke(
            rect,
            CornerRadius::same(radius),
            Stroke::new(1.0, pal.outline_variant),
            egui::StrokeKind::Inside,
        );
        ui.set_clip_rect(rect);

        let inner = rect.shrink(pad);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("welcome-sites")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(gap, gap);
                    let cells: Vec<Option<&String>> = sites
                        .iter()
                        .filter(|url| !url.trim().is_empty())
                        .take(cap)
                        .map(Some)
                        .chain(std::iter::once(None))
                        .collect();
                    for chunk in cells.chunks(cols) {
                        ui.horizontal(|ui| {
                            for cell_ref in chunk {
                                let (tile, resp) = ui.allocate_exact_size(
                                    Vec2::splat(cell),
                                    if setup { Sense::hover() } else { Sense::click() },
                                );
                                let Some(url) = cell_ref else {
                                    let icon_rect = Rect::from_center_size(
                                        Pos2::new(tile.center().x, tile.top() + cell * 0.38),
                                        Vec2::splat(cell * 0.44),
                                    );
                                    ui.painter().rect_filled(
                                        icon_rect,
                                        CornerRadius::same((icon_rect.height() * 0.28) as u8),
                                        pal.surface_container_high,
                                    );
                                    let c = icon_rect.center();
                                    let d = icon_rect.height() * 0.22;
                                    let stroke = Stroke::new((cell * 0.04).max(1.2), pal.on_surface_variant);
                                    ui.painter().line_segment([c - Vec2::new(d, 0.0), c + Vec2::new(d, 0.0)], stroke);
                                    ui.painter().line_segment([c - Vec2::new(0.0, d), c + Vec2::new(0.0, d)], stroke);
                                    if resp.hovered() && !setup {
                                        ui.painter().rect_filled(
                                            tile,
                                            CornerRadius::same((8.0 * k) as u8),
                                            pal.surface_container_high,
                                        );
                                    }
                                    if resp.clicked() {
                                        clicked = Some(SitesHit::Add);
                                    }
                                    if !setup {
                                        resp.on_hover_text("add site");
                                    }
                                    continue;
                                };
                                if resp.hovered() && !setup {
                                    ui.painter().rect_filled(
                                        tile,
                                        CornerRadius::same((8.0 * k) as u8),
                                        pal.surface_container_high,
                                    );
                                }
                                let icon_rect = Rect::from_center_size(
                                    Pos2::new(tile.center().x, tile.top() + cell * 0.38),
                                    Vec2::splat(cell * 0.44),
                                );
                                match icon(url) {
                                    Some(id) => {
                                        ui.painter().image(
                                            id,
                                            icon_rect,
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                            Color32::WHITE,
                                        );
                                    }
                                    None => {
                                        let label = site_label(url);
                                        let letter = label
                                            .chars()
                                            .next()
                                            .unwrap_or('?')
                                            .to_ascii_uppercase();
                                        ui.painter().rect_filled(
                                            icon_rect,
                                            CornerRadius::same((icon_rect.height() * 0.28) as u8),
                                            pal.primary_container,
                                        );
                                        ui.painter().text(
                                            icon_rect.center(),
                                            Align2::CENTER_CENTER,
                                            letter,
                                            FontId::proportional(icon_rect.height() * 0.55),
                                            pal.on_primary_container,
                                        );
                                    }
                                }
                                let font = FontId::proportional((cell * 0.17).max(7.0));
                                let max_w = cell * 0.95;
                                let base = site_label(url);
                                let mut keep = base.chars().count();
                                let mut galley = ui.painter().layout_no_wrap(
                                    base.clone(),
                                    font.clone(),
                                    pal.on_surface_variant,
                                );
                                while galley.size().x > max_w && keep > 1 {
                                    keep -= 1;
                                    let shown =
                                        base.chars().take(keep).collect::<String>() + "…";
                                    galley = ui.painter().layout_no_wrap(
                                        shown,
                                        font.clone(),
                                        pal.on_surface_variant,
                                    );
                                }
                                ui.painter().galley(
                                    Pos2::new(
                                        tile.center().x - galley.size().x / 2.0,
                                        tile.bottom() - cell * 0.12 - galley.size().y,
                                    ),
                                    galley,
                                    pal.on_surface_variant,
                                );
                                if resp.clicked() {
                                    clicked = Some(SitesHit::Open(url.to_string()));
                                }
                                if !setup {
                                    resp.on_hover_text(*url);
                                }
                            }
                        });
                    }
                });
        });
    });
    clicked
}

// ---------------------------------------------------------------- layout

pub fn item_size(
    kind: ItemKind,
    logo: LogoPreference,
    clock: ClockOrientation,
    welcome: &Welcome,
) -> Vec2 {
    match kind {
        ItemKind::Clock => match clock {
            ClockOrientation::Horizontal => Vec2::new(360.0, 120.0),
            ClockOrientation::Vertical => Vec2::new(140.0, 320.0),
        },
        ItemKind::Logo => match logo {
            LogoPreference::Big => Vec2::new(300.0, 260.0),
            _ => Vec2::new(420.0, 110.0),
        },
        ItemKind::Splash => Vec2::new(560.0, 30.0),
        ItemKind::Search => Vec2::new(560.0, 62.0),
        ItemKind::Phrases => Vec2::new(210.0, 40.0),
        ItemKind::Todo => Vec2::new(300.0, 240.0),
        ItemKind::Sites => sites_size(welcome.site_rows, welcome.site_cols),
    }
}

pub fn kind_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Clock => "clock",
        ItemKind::Logo => "logo",
        ItemKind::Splash => "splash",
        ItemKind::Search => "search",
        ItemKind::Phrases => "phrases",
        ItemKind::Todo => "todo",
        ItemKind::Sites => "sites",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_cell_is_the_add_button() {
        assert_eq!(site_cap(1, 8), 7);
        assert_eq!(site_cap(2, 4), 7);
        assert_eq!(site_cap(1, 1), 1);
    }
}
