//! scrapeconfig — a friendlier about:config. Flat, searchable, grouped rows.

use crate::config::{
    ClockOrientation, Config, LogoPreference, SplashMode, TodoStyle, Welcome,
};
use eframe::egui::{self, Layout, TextEdit, Ui};

struct RowDef {
    group: &'static str,
    key: &'static str,
    desc: &'static str,
}

const ROWS: &[RowDef] = &[
    RowDef { group: "Tabs & layout", key: "scrape.tabs.position", desc: "Top strip or left side rail." },
    RowDef { group: "Tabs & layout", key: "scrape.theme.radius", desc: "Corner radius of every surface, in points." },
    RowDef { group: "Tabs & layout", key: "scrape.theme.font_scale", desc: "Font scale for the whole UI." },
    RowDef { group: "Appearance", key: "scrape.theme.seed", desc: "Seed colour of the MD3 palette." },
    RowDef { group: "Appearance", key: "scrape.theme.dark", desc: "Dark or light scheme." },
    RowDef { group: "Appearance", key: "scrape.theme.wallpaper", desc: "Local image or animated GIF/WebP used as wallpaper." },
    RowDef { group: "Appearance", key: "scrape.theme.wallpaper.blur", desc: "Blur radius applied to the wallpaper." },
    RowDef { group: "Appearance", key: "scrape.theme.palette_from_wallpaper", desc: "Derive the seed from the wallpaper on start-up." },
    RowDef { group: "Welcome screen", key: "scrape.splash.mode", desc: "Off, built-in rotating phrases, or your own list." },
    RowDef { group: "Welcome screen", key: "scrape.splash.custom_splash", desc: "Comma separated phrases for Custom mode." },
    RowDef { group: "Welcome screen", key: "scrape.logo.preference", desc: "basic (gear-R), plain (text only) or big (gear above text)." },
    RowDef { group: "Welcome screen", key: "scrape.clock.font", desc: "anthropic serif, google sans, geist, or a .ttf path." },
    RowDef { group: "Welcome screen", key: "scrape.clock.orientation", desc: "Horizontal 00:00 or stacked digits." },
    RowDef { group: "Welcome screen", key: "scrape.clock.seconds", desc: "Show hours:minutes:seconds instead of hours:minutes." },
    RowDef { group: "Welcome screen", key: "scrape.welcome.todo_style", desc: "Nothing (black, red, NDot) or Claude (warm grey, orange)." },
    RowDef { group: "Welcome screen", key: "scrape.welcome.layout", desc: "Where the welcome elements sit. Reset brings back the default." },
    RowDef { group: "Memory", key: "scrape.tabs.unload_after_minutes", desc: "Inactivity before a background tab is unloaded." },
    RowDef { group: "Memory", key: "scrape.tabs.unload_whitelist", desc: "Comma separated domains that are never unloaded." },
    RowDef { group: "Memory", key: "scrape.tabs.memory_limit_mb", desc: "Process memory ceiling that unloads background tabs." },
    RowDef { group: "Search", key: "scrape.search.engine", desc: "Engine used when the address bar holds a query, not a url." },
    RowDef { group: "AI agent", key: "scrape.agent.base_url", desc: "OpenAI-compatible endpoint." },
    RowDef { group: "AI agent", key: "scrape.agent.model", desc: "Model name sent with each request." },
    RowDef { group: "AI agent", key: "scrape.agent.api_key", desc: "Your own key. Stored as plain text in config.json." },
    RowDef { group: "AI agent", key: "scrape.agent.permissions_granted", desc: "Allows the agent to read and act on pages." },
    RowDef { group: "AI agent", key: "scrape.agent.cdp_port", desc: "Remote debugging port for external tools. 0 keeps it off." },
    RowDef { group: "AI agent", key: "scrape.agent.context_tokens", desc: "Context window of the model, shown as the fill bar under the chat." },
    RowDef { group: "Privacy", key: "scrape.history.collect", desc: "Keep recent searches for the menu. Off stores nothing." },
    RowDef { group: "Privacy", key: "scrape.history.clear", desc: "Delete every recorded search now." },
];

pub fn show(ui: &mut Ui, cfg: &mut Config, search: &mut String, close: &mut bool) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.heading("scrapeconfig");
        ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("✕")
                .on_hover_text("Close (Esc)")
                .clicked()
            {
                *close = true;
            }
            ui.add(
                TextEdit::singleline(search)
                    .hint_text("search settings")
                    .desired_width(260.0),
            );
        });
    });
    ui.add_space(2.0);
    ui.label(egui::RichText::new(&Config::path().display().to_string()).small());
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut group = "";
            for row in ROWS {
                if !matches(search, row.key, row.desc) {
                    continue;
                }
                if row.group != group {
                    group = row.group;
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(group).strong());
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(row.key).monospace());
                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        // Keeps controls clear of the floating scrollbar.
                        ui.add_space(14.0);
                        changed |= editor(ui, row.key, cfg);
                    });
                });
                ui.label(egui::RichText::new(row.desc).small().weak());
                ui.add_space(2.0);
            }
            ui.add_space(20.0);
        });

    changed
}

fn matches(search: &str, key: &str, desc: &str) -> bool {
    let q = search.trim().to_lowercase();
    q.is_empty()
        || key.to_lowercase().contains(&q)
        || desc.to_lowercase().contains(&q)
}

fn editor(ui: &mut Ui, key: &str, cfg: &mut Config) -> bool {
    match key {
        "scrape.tabs.position" => {
            let mut changed = false;
            changed |= ui
                .radio_value(&mut cfg.theme.tabs_position, crate::theme::TabsPosition::Side, "Side")
                .changed();
            changed |= ui
                .radio_value(&mut cfg.theme.tabs_position, crate::theme::TabsPosition::Top, "Top")
                .changed();
            changed
        }
        "scrape.theme.seed" => {
            let mut seed = cfg.theme.seed;
            let changed = ui.color_edit_button_srgb(&mut seed).changed();
            if changed {
                cfg.theme.seed = seed;
                cfg.theme.palette_from_wallpaper = false;
            }
            changed
        }
        "scrape.theme.dark" => ui.checkbox(&mut cfg.theme.dark, "dark").changed(),
        "scrape.theme.radius" => ui
            .add(egui::DragValue::new(&mut cfg.theme.radius).range(0..=28).suffix(" px"))
            .changed(),
        "scrape.theme.font_scale" => ui
            .add(
                egui::DragValue::new(&mut cfg.theme.font_scale)
                    .range(0.7..=1.6)
                    .speed(0.01),
            )
            .changed(),
        "scrape.theme.wallpaper" => {
            let label = cfg
                .theme
                .wallpaper
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "none".into());
            let mut changed = false;
            if ui.button("Clear").clicked() {
                cfg.theme.wallpaper = None;
                changed = true;
            }
            if ui.button("Pick…").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("image", &["png", "jpg", "jpeg", "webp", "bmp", "gif"])
                    .pick_file()
                {
                    cfg.theme.wallpaper = Some(path.display().to_string());
                    changed = true;
                }
            }
            ui.label(egui::RichText::new(label).small());
            changed
        }
        "scrape.theme.wallpaper.blur" => ui
            .add(
                egui::DragValue::new(&mut cfg.theme.wallpaper_blur)
                    .range(0.0..=40.0)
                    .speed(0.5)
                    .suffix(" px"),
            )
            .changed(),
        "scrape.theme.palette_from_wallpaper" => ui
            .checkbox(&mut cfg.theme.palette_from_wallpaper, "yes")
            .changed(),
        "scrape.splash.mode" => {
            let mut changed = false;
            for (mode, label) in [
                (SplashMode::Off, "Off"),
                (SplashMode::On, "On"),
                (SplashMode::Custom, "Custom"),
            ] {
                changed |= ui
                    .selectable_value(&mut cfg.splash.mode, mode, label)
                    .changed();
            }
            changed
        }
        "scrape.splash.custom_splash" => {
            let mut text = cfg.splash.custom_splash.join(", ");
            if ui
                .add(
                    TextEdit::singleline(&mut text)
                        .hint_text("\"one\", \"two\"")
                        .desired_width(320.0),
                )
                .changed()
            {
                cfg.splash.custom_splash = text
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                true
            } else {
                false
            }
        }
        "scrape.logo.preference" => {
            let mut changed = false;
            for (pref, label) in [
                (LogoPreference::Basic, "basic"),
                (LogoPreference::Plain, "plain"),
                (LogoPreference::Big, "big"),
            ] {
                changed |= ui
                    .selectable_value(&mut cfg.logo.preference, pref, label)
                    .changed();
            }
            changed
        }
        "scrape.clock.font" => ui
            .add(
                TextEdit::singleline(&mut cfg.clock.font)
                    .hint_text("anthropic serif · google sans · geist")
                    .desired_width(320.0),
            )
            .changed(),
        "scrape.clock.orientation" => {
            let mut changed = false;
            for (orientation, label) in [
                (ClockOrientation::Horizontal, "Horizontal"),
                (ClockOrientation::Vertical, "Vertical"),
            ] {
                changed |= ui
                    .selectable_value(&mut cfg.clock.orientation, orientation, label)
                    .changed();
            }
            changed
        }
        "scrape.clock.seconds" => ui.checkbox(&mut cfg.clock.seconds, "show").changed(),
        "scrape.welcome.todo_style" => {
            let mut changed = false;
            for (style, label) in [
                (TodoStyle::Nothing, "Nothing"),
                (TodoStyle::Claude, "Claude"),
            ] {
                changed |= ui
                    .selectable_value(&mut cfg.welcome.todo_style, style, label)
                    .changed();
            }
            changed
        }
        "scrape.welcome.layout" => {
            if ui.button("reset layout").clicked() {
                cfg.welcome = Welcome::default();
                true
            } else {
                false
            }
        }
        "scrape.tabs.unload_after_minutes" => ui
            .add(
                egui::DragValue::new(&mut cfg.unload_after_minutes)
                    .range(1..=240)
                    .suffix(" min"),
            )
            .changed(),
        "scrape.tabs.unload_whitelist" => {
            let mut text = cfg.unload_whitelist.join(", ");
            let resp = ui.add(TextEdit::singleline(&mut text).desired_width(320.0));
            if resp.changed() {
                cfg.unload_whitelist = text
                    .split(',')
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
                    .collect();
                true
            } else {
                false
            }
        }
        "scrape.tabs.memory_limit_mb" => ui
            .add(
                egui::DragValue::new(&mut cfg.memory_limit_mb)
                    .range(128..=8192)
                    .suffix(" MB"),
            )
            .changed(),
        "scrape.agent.base_url" => ui
            .add(TextEdit::singleline(&mut cfg.agent_base_url).desired_width(320.0))
            .changed(),
        "scrape.agent.model" => ui
            .add(TextEdit::singleline(&mut cfg.agent_model).desired_width(220.0))
            .changed(),
        "scrape.agent.api_key" => ui
            .add(
                TextEdit::singleline(&mut cfg.agent_api_key)
                    .password(true)
                    .desired_width(220.0),
            )
            .changed(),
        "scrape.search.engine" => {
            let mut changed = false;
            egui::ComboBox::from_id_salt("search-engine")
                .selected_text(cfg.search_engine.clone())
                .show_ui(ui, |ui| {
                    for engine in crate::engines::ENGINES {
                        changed |= ui
                            .selectable_value(
                                &mut cfg.search_engine,
                                engine.name.to_owned(),
                                engine.name,
                            )
                            .changed();
                    }
                });
            changed
        }
        "scrape.agent.permissions_granted" => ui
            .checkbox(&mut cfg.agent_permissions_granted, "allowed")
            .changed(),
        "scrape.agent.cdp_port" => ui
            .add(
                egui::DragValue::new(&mut cfg.agent_cdp_port)
                    .range(0..=65535)
                    .suffix(" port"),
            )
            .changed(),
        "scrape.agent.context_tokens" => ui
            .add(
                egui::DragValue::new(&mut cfg.agent_context_tokens)
                    .range(2_000..=2_000_000)
                    .speed(1000.0)
                    .suffix(" tokens"),
            )
            .changed(),
        "scrape.history.collect" => ui.checkbox(&mut cfg.collect_history, "collect").changed(),
        "scrape.history.clear" => {
            let clicked = ui.button(format!("Clear ({} saved)", cfg.history.len())).clicked();
            if clicked {
                cfg.history.clear();
            }
            clicked
        }
        _ => false,
    }
}
