//! The search engines offered by the picker, with their logos embedded so they work offline.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub struct Engine {
    pub name: &'static str,
    /// Letter(s) shown only if the logo fails to decode.
    pub letter: &'static str,
    pub color: [u8; 3],
    pub icon: &'static [u8],
    pub template: &'static str,
}

pub const ENGINES: &[Engine] = &[
    Engine {
        name: "Google",
        letter: "G",
        color: [0x42, 0x85, 0xF4],
        icon: include_bytes!("../assets/engines/google.png"),
        template: "https://www.google.com/search?q={query}",
    },
    Engine {
        name: "Bing",
        letter: "b",
        color: [0x00, 0x80, 0x9D],
        icon: include_bytes!("../assets/engines/bing.png"),
        template: "https://www.bing.com/search?q={query}",
    },
    Engine {
        name: "DuckDuckGo",
        letter: "D",
        color: [0xDE, 0x58, 0x33],
        icon: include_bytes!("../assets/engines/duckduckgo.png"),
        template: "https://duckduckgo.com/?q={query}",
    },
    Engine {
        name: "Perplexity",
        letter: "P",
        color: [0x20, 0x80, 0x8D],
        icon: include_bytes!("../assets/engines/perplexity.png"),
        template: "https://www.perplexity.ai/search?q={query}",
    },
    Engine {
        name: "Yandex",
        letter: "Я",
        color: [0xFC, 0x3F, 0x1D],
        icon: include_bytes!("../assets/engines/yandex.png"),
        template: "https://yandex.com/search/?text={query}",
    },
    Engine {
        name: "Brave",
        letter: "B",
        color: [0xFB, 0x54, 0x2B],
        icon: include_bytes!("../assets/engines/brave.png"),
        template: "https://search.brave.com/search?q={query}",
    },
];

/// Decoded logos, one per entry in `ENGINES`.
pub struct Icons(Vec<Option<egui::TextureHandle>>);

impl Icons {
    pub fn load(ctx: &egui::Context) -> Self {
        Self(
            ENGINES
                .iter()
                .map(|e| {
                    let img = image::load_from_memory(e.icon).ok()?.to_rgba8();
                    let (w, h) = img.dimensions();
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        img.as_raw(),
                    );
                    Some(ctx.load_texture(e.name, color, egui::TextureOptions::LINEAR))
                })
                .collect(),
        )
    }

    fn get(&self, engine: &Engine) -> Option<&egui::TextureHandle> {
        let i = ENGINES.iter().position(|e| e.name == engine.name)?;
        self.0.get(i)?.as_ref()
    }
}

pub fn get(name: &str) -> &'static Engine {
    ENGINES
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(name))
        .unwrap_or(&ENGINES[2])
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, engine: &Engine, icons: &Icons) {
    if let Some(tex) = icons.get(engine) {
        painter.image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
        return;
    }
    let [r, g, b] = engine.color;
    painter.circle_filled(rect.center(), rect.width() / 2.0, Color32::from_rgb(r, g, b));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        engine.letter,
        FontId::proportional(rect.width() * 0.68),
        Color32::WHITE,
    );
}

/// Engine chips with names. Height follows the caller's row, returns the picked engine name.
pub fn chips(
    ui: &mut Ui,
    current: &str,
    pal: &crate::theme::Palette,
    icons: &Icons,
) -> Option<&'static str> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for engine in ENGINES {
            let selected = engine.name.eq_ignore_ascii_case(current);
            let width = chip_width(ui, engine);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
            let fill = if selected {
                pal.primary_container
            } else if resp.hovered() {
                pal.surface_container_highest
            } else {
                pal.surface_container
            };
            ui.painter()
                .rect_filled(rect, CornerRadius::same(15), fill);
            paint_icon(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(rect.left() + 16.0, rect.center().y),
                    Vec2::splat(17.0),
                ),
                engine,
                icons,
            );
            let color = if selected {
                pal.on_primary_container
            } else {
                pal.on_surface
            };
            ui.painter().text(
                Pos2::new(rect.left() + 30.0, rect.center().y),
                Align2::LEFT_CENTER,
                engine.name,
                FontId::proportional(13.0),
                color,
            );
            if resp.clicked() {
                picked = Some(engine.name);
            }
        }
    });
    picked
}

/// Width of one chip, so callers can centre the whole row.
fn chip_width(ui: &Ui, engine: &Engine) -> f32 {
    let text = ui.painter().layout_no_wrap(
        engine.name.to_owned(),
        FontId::proportional(13.0),
        Color32::WHITE,
    );
    text.size().x + 48.0
}

/// Total width of the single-row chip layout, including spacing.
pub fn row_width(ui: &Ui) -> f32 {
    let spacing = ui.spacing().item_spacing.x;
    ENGINES.iter().map(|e| chip_width(ui, e)).sum::<f32>()
        + spacing * (ENGINES.len() - 1) as f32
}

/// One rounded search field: engine badge on the left, text input filling the rest.
/// Returns (submitted, badge clicked, field rect).
pub fn search_field(
    ui: &mut Ui,
    buf: &mut String,
    engine: &Engine,
    pal: &crate::theme::Palette,
    icons: &Icons,
    radius: u8,
    width: f32,
    height: f32,
    id: egui::Id,
    hint: &str,
) -> (bool, bool, Rect, bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let pill = CornerRadius::same(radius.min((height / 2.0) as u8));
    ui.painter().rect_filled(rect, pill, pal.surface_container_high);
    ui.painter().rect_stroke(
        rect,
        pill,
        Stroke::new(1.0, pal.outline_variant),
        egui::StrokeKind::Inside,
    );

    let badge = Pos2::new(rect.left() + height / 2.0, rect.center().y);
    let badge_rect = Rect::from_center_size(badge, Vec2::splat(height - 6.0));
    let badge_resp = ui.interact(badge_rect, id.with("engine"), Sense::click());
    if badge_resp.hovered() {
        ui.painter()
            .circle_filled(badge, height * 0.30, pal.surface_container_highest);
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(badge, Vec2::splat(height * 0.52)),
        engine,
        icons,
    );

    let text_rect = Rect::from_min_max(
        Pos2::new(rect.left() + height, rect.top() + 3.0),
        Pos2::new(rect.right() - 12.0, rect.bottom() - 3.0),
    );
    // The row gets shorter with the window but keeps enough height for the text, so a
    // small window shows a tall narrow field instead of a thin wide one.
    let ratio = 0.34;
    let mut size = (height * ratio).clamp(10.0, 15.0);
    if !hint.is_empty() {
        let probe = egui::FontId::new(size, egui::FontFamily::Proportional);
        let hint_width = ui
            .painter()
            .layout_no_wrap(hint.to_owned(), probe, Color32::PLACEHOLDER)
            .size()
            .x;
        if hint_width > text_rect.width() {
            size = (size * text_rect.width() / hint_width).max(7.0);
        }
    }
    // With no hint the welcome row types its own phrases, like someone writing.
    let animated = hint.is_empty();
    let font = egui::FontId::new(size, egui::FontFamily::Proportional);
    let resp = ui
        .scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
            ui.style_mut().text_styles.insert(egui::TextStyle::Body, font);
            ui.add_sized(
                text_rect.size(),
                egui::TextEdit::singleline(buf)
                    .id(id)
                    .frame(egui::Frame::new())
                    .hint_text(if animated { "" } else { hint })
                    .align(egui::Align2::LEFT_CENTER),
            )
        })
        .inner;
    if buf.is_empty() {
        let text = if animated {
            typed_phrase(ui.input(|i| i.time))
        } else {
            hint.to_owned()
        };
        if animated && !text.is_empty() {
            ui.painter().text(
                Pos2::new(text_rect.left() + 2.0, text_rect.center().y),
                Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(size.max(12.5)),
                pal.outline,
            );
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    let submitted = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    (submitted, badge_resp.clicked(), rect, resp.has_focus())
}

/// Phrases the welcome search row types and erases, fixed in code. They are their own
/// set, so the row never echoes the splash line under the logo.
const PHRASES: [&str; 8] = [
    "ask me anything",
    "search the docs",
    "go down a rabbit hole",
    "how does this work?",
    "find me a good read",
    "compare two things",
    "what broke this time?",
    "one more tab won't hurt",
];

/// The part of a phrase visible at `time`, as if typed and then deleted.
fn typed_phrase(time: f64) -> String {
    let (type_s, del_s, hold, gap) = (0.045, 0.022, 1.6, 0.45);
    let len = |p: &str| p.chars().count() as f64;
    let span = |p: &str| len(p) * (type_s + del_s) + hold + gap;
    let total: f64 = PHRASES.iter().map(|p| span(p)).sum();
    let mut t = time % total;
    for phrase in PHRASES {
        let s = span(phrase);
        if t >= s {
            t -= s;
            continue;
        }
        let n = len(phrase);
        let typed = (t / type_s) as usize;
        if (typed as f64) < n {
            return phrase.chars().take(typed).collect();
        }
        if t < n * type_s + hold {
            return phrase.to_owned();
        }
        let gone = ((t - n * type_s - hold) / del_s) as usize;
        return phrase
            .chars()
            .take(n as usize - gone.min(n as usize))
            .collect();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> f64 {
        s.chars().count() as f64
    }

    #[test]
    fn typed_phrase_types_then_erases() {
        let first = PHRASES[0];
        assert!(typed_phrase(0.0).is_empty());
        // Just past typing the first phrase it is shown whole, then deletion starts.
        let typed = chars(first) * 0.045 + 0.01;
        assert_eq!(typed_phrase(typed), first);
        let erased = chars(first) * 0.045 + 1.6 + chars(first) * 0.022 - 0.01;
        assert_eq!(typed_phrase(erased), first.chars().take(1).collect::<String>());
        // Any moment shows a prefix of some phrase, never garbage.
        assert!(PHRASES.iter().any(|p| p.starts_with(&typed_phrase(12.34))));
    }
}
