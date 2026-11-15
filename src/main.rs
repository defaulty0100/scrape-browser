#![windows_subsystem = "windows"]

mod agent;
mod assets;
mod config;
mod engines;
mod favicon;
mod page;
mod scrapeconfig;
mod theme;
mod wall;
mod welcome;

use config::{Config, ItemKind, LogoPreference, SplashMode, TodoStyle};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2,
    Rect, Sense, Stroke, TextEdit, Ui, Vec2,
};
use raw_window_handle::HasWindowHandle;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use theme::{Palette, Theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Browser,
    Config,
}

#[derive(Clone, PartialEq, Eq)]
enum MenuAction {
    Config,
    Theme,
    Setup,
    NewTab,
    CloseTab,
    Reload,
    Unload,
    Incognito,
}

struct Tab {
    title: String,
    /// Empty means the native welcome page.
    url: String,
    /// None while the tab is unloaded; the url is kept and reloaded on activation.
    page: Option<page::Page>,
    titles: Arc<Mutex<String>>,
    last_active: Instant,
}

impl Tab {
    fn new() -> Self {
        Self {
            title: "New tab".into(),
            url: String::new(),
            page: None,
            titles: Arc::new(Mutex::new(String::new())),
            last_active: Instant::now(),
        }
    }
}

struct App {
    cfg: Config,
    last_theme: Theme,
    pal: Palette,
    tabs: Vec<Tab>,
    active: usize,
    url_buf: String,
    view: View,
    show_theme: bool,
    show_agent: bool,
    permissions_open: bool,
    engine_open: bool,
    menu_open: bool,
    /// Where the three-dot menu hangs from, set while the toolbar paints.
    menu_anchor: Option<Rect>,
    /// Short-lived message, e.g. an incognito warning.
    notice: Option<(String, Instant)>,
    /// One warning per incognito session, so it does not nag on every search.
    incognito_warned: bool,
    setup: bool,
    selected: Option<usize>,
    search: String,
    dirty: bool,

    parent: isize,
    page_error: Option<String>,
    pending: Option<String>,
    content_rect: Option<Rect>,
    addr_id: egui::Id,
    maximized: bool,

    agent: agent::Agent,
    logo: Option<egui::TextureHandle>,
    icons: engines::Icons,
    favicons: favicon::Favicons,
    wall: wall::Wallpaper,
    last_unload: Instant,
    welcome_at: Instant,
    /// Random offset so splash and phrase strip change on every visit.
    welcome_seed: f32,
    was_welcome: bool,
    last_clock_font: String,
}

fn parent_handle(cc: &eframe::CreationContext<'_>) -> isize {
    #[cfg(windows)]
    {
        use raw_window_handle::RawWindowHandle;
        if let Ok(handle) = cc.window_handle() {
            if let RawWindowHandle::Win32(w) = handle.as_raw() {
                return w.hwnd.get();
            }
        }
    }
    let _ = cc;
    0
}

/// Working set of this process plus every WebView2 process, in MB.
fn process_memory_mb() -> u64 {
    let Ok(mut sys) = sysinfo::System::new() else {
        return 0;
    };
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let bytes: u64 = sys
        .processes()
        .values()
        .filter(|p| {
            let name = p.name().to_string_lossy().to_ascii_lowercase();
            name == "scrape.exe" || name == "msedgewebview2.exe"
        })
        .map(|p| p.memory())
        .sum();
    bytes / (1024 * 1024)
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut cfg = Config::load();
        if let Some(seed) = theme::effective_seed(&cfg.theme) {
            cfg.theme.seed = seed;
        }
        // Must be set before the first webview is created.
        if cfg.agent_cdp_port > 0 {
            crate::page::set_browser_args(format!(
                "--remote-debugging-port={}",
                cfg.agent_cdp_port
            ));
        }
        theme::install_fonts(&cc.egui_ctx, &cfg.clock.font);
        let pal = Palette::from_theme(&cfg.theme);
        theme::apply_style(&cc.egui_ctx, &pal, &cfg.theme);
        let logo = assets::logo()
            .map(|img| cc.egui_ctx.load_texture("logo", img, egui::TextureOptions::LINEAR));
        let icons = engines::Icons::load(&cc.egui_ctx);

        Self {
            last_theme: cfg.theme.clone(),
            last_clock_font: cfg.clock.font.clone(),
            cfg,
            pal,
            tabs: vec![Tab::new()],
            active: 0,
            url_buf: String::new(),
            view: View::Browser,
            show_theme: false,
            show_agent: false,
            permissions_open: false,
            engine_open: false,
            menu_open: false,
            menu_anchor: None,
            notice: None,
            incognito_warned: false,
            setup: false,
            selected: None,
            search: String::new(),
            dirty: false,
            parent: parent_handle(cc),
            page_error: None,
            pending: None,
            content_rect: None,
            addr_id: egui::Id::unique("scrape-address"),
            maximized: false,
            agent: agent::Agent::new(),
            logo,
            icons,
            favicons: favicon::Favicons::default(),
            wall: wall::Wallpaper::default(),
            last_unload: Instant::now(),
            welcome_at: Instant::now(),
            welcome_seed: random_seed(),
            was_welcome: true,
        }
    }

    fn new_tab(&mut self) {
        self.tabs.push(Tab::new());
        self.active = self.tabs.len() - 1;
        self.url_buf.clear();
        self.view = View::Browser;
        self.page_error = None;
        self.welcome_visit();
    }

    fn welcome_visit(&mut self) {
        self.welcome_at = Instant::now();
        self.welcome_seed = random_seed();
    }

    fn select_tab(&mut self, i: usize) {
        if i >= self.tabs.len() || i == self.active {
            return;
        }
        self.active = i;
        self.url_buf = self.tabs[i].url.clone();
        self.tabs[i].last_active = Instant::now();
        self.view = View::Browser;
        self.page_error = None;
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            self.tabs.push(Tab::new());
            self.active = 0;
        } else if i < self.active || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.url_buf = self.tabs[self.active].url.clone();
        self.tabs[self.active].last_active = Instant::now();
    }

    fn go(&mut self, input: &str) {
        let url = self.cfg.search(input);
        if url.is_empty() {
            return;
        }
        self.record_search(input);
        let tab = &mut self.tabs[self.active];
        tab.url = url.clone();
        tab.last_active = Instant::now();
        self.pending = Some(url.clone());
        self.url_buf = url;
        self.view = View::Browser;
        self.engine_open = false;
        self.setup = false;
        self.selected = None;
    }

    /// Keeps recent queries for the menu, unless the user opted out or is private.
    fn record_search(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() || Config::is_address(input) {
            return;
        }
        if self.cfg.incognito {
            self.warn_incognito("Incognito: search history is blocked, nothing was saved.");
            return;
        }
        if !self.cfg.collect_history {
            return;
        }
        self.cfg.history.retain(|q| q != input);
        self.cfg.history.insert(0, input.to_owned());
        self.cfg.history.truncate(100);
        self.dirty = true;
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), Instant::now()));
    }

    /// Incognito warnings are shown once per session, not on every action.
    fn warn_incognito(&mut self, msg: &str) {
        if !self.incognito_warned {
            self.incognito_warned = true;
            self.notify(msg);
        }
    }

    fn is_welcome(&self) -> bool {
        self.view == View::Browser && self.tabs[self.active].url.is_empty()
    }

    fn address_id(&self) -> egui::Id {
        if self.is_welcome() {
            self.addr_id.with("welcome")
        } else {
            self.addr_id
        }
    }

    fn hotkeys(&mut self, ctx: &egui::Context) {
        let (focus, new_tab, close_tab, agent) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::CTRL, egui::Key::L),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::T),
                i.consume_key(egui::Modifiers::CTRL, egui::Key::W),
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::A),
            )
        });
        if focus {
            let id = self.address_id();
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if new_tab {
            self.new_tab();
        }
        if close_tab {
            let active = self.active;
            self.close_tab(active);
        }
        if agent {
            self.show_agent = !self.show_agent;
            if self.show_agent && !self.cfg.agent_permissions_granted {
                self.permissions_open = true;
            }
        }
    }

    fn title_bar(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let tabs_on_top = self.cfg.theme.tabs_position == theme::TabsPosition::Top;
        let mut select = None;
        let mut close = None;
        let mut add = false;
        let mut window = None;

        egui::Panel::top("titlebar")
            .exact_size(40.0)
            .frame(Frame::new().fill(pal.surface_container_low))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let drag = ui.interact(full, egui::Id::unique("title-drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!self.maximized));
                }

                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    if tabs_on_top {
                        for i in 0..self.tabs.len() {
                            let title = self.tabs[i].title.clone();
                            let (clicked, closed) = tab_chip(
                                ui,
                                &title,
                                i == self.active,
                                &pal,
                                self.cfg.theme.radius,
                                i,
                                190.0,
                            );
                            if clicked {
                                select = Some(i);
                            }
                            if closed {
                                close = Some(i);
                            }
                        }
                        add = plus_button(ui, &pal);
                    } else {
                        mini_logo(ui, self.logo.as_ref(), &pal);
                        ui.label(
                            egui::RichText::new("Scrape")
                                .family(FontFamily::Name(theme::SERIF.into()))
                                .size(17.0)
                                .color(pal.on_surface),
                        );
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if win_button(ui, WinButton::Close, &pal, false) {
                            window = Some(2);
                        }
                        if win_button(ui, WinButton::Max, &pal, self.maximized) {
                            window = Some(1);
                        }
                        if win_button(ui, WinButton::Min, &pal, false) {
                            window = Some(0);
                        }
                    });
                });
            });

        match window {
            Some(0) => ui
                .ctx()
                .send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            Some(1) => ui
                .ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(!self.maximized)),
            Some(2) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        if let Some(i) = select {
            self.select_tab(i);
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
        if add {
            self.new_tab();
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let mut action = None;
        let mut picked: Option<&'static str> = None;
        let engine = engines::get(&self.cfg.search_engine);

        egui::Panel::top("toolbar")
            .frame(
                Frame::new()
                    .fill(pal.surface_container_low)
                    .inner_margin(Margin::symmetric(10, 7)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if chevron(ui, false, &pal) {
                        action = Some(0);
                    }
                    if chevron(ui, true, &pal) {
                        action = Some(1);
                    }
                    if reload_button(ui, &pal) {
                        action = Some(2);
                    }

                    // Right-to-left: the two icons hug the edge, the field takes the rest.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let dots = icon_button(ui, &pal, self.menu_open, paint_dots);
                        if dots.clicked() {
                            self.menu_open = !self.menu_open;
                        }
                        self.menu_anchor = Some(dots.rect);
                        if icon_button(ui, &pal, self.show_agent, |p, r, c| {
                            welcome::agent_glyph(p, r, c);
                        })
                        .clicked()
                        {
                            self.show_agent = !self.show_agent;
                            if self.show_agent && !self.cfg.agent_permissions_granted {
                                self.permissions_open = true;
                            }
                        }
                        ui.add_space(6.0);
                        let width = ui.available_width().max(80.0);
                        let (submitted, badge, _, _) = engines::search_field(
                            ui,
                            &mut self.url_buf,
                            engine,
                            &pal,
                            &self.icons,
                            self.cfg.theme.radius,
                            width,
                            34.0,
                            self.addr_id,
                            "Search or enter address",
                        );
                        if submitted {
                            action = Some(3);
                        }
                        if badge {
                            self.engine_open = !self.engine_open;
                        }
                    });
                });

                if self.engine_open && !self.is_welcome() {
                    ui.add_space(2.0);
                    picked = engines::chips(ui, &self.cfg.search_engine, &pal, &self.icons);
                }
            });

        if let Some(name) = picked {
            self.cfg.search_engine = name.to_owned();
            self.engine_open = false;
            self.dirty = true;
        }

        match action {
            Some(0) => {
                if let Some(p) = &self.tabs[self.active].page {
                    p.back();
                }
            }
            Some(1) => {
                if let Some(p) = &self.tabs[self.active].page {
                    p.forward();
                }
            }
            Some(2) => {
                let url = self.tabs[self.active].url.clone();
                match &self.tabs[self.active].page {
                    Some(p) => p.reload(),
                    None if !url.is_empty() => self.pending = Some(url),
                    None => {}
                }
            }
            Some(3) => {
                let text = self.url_buf.clone();
                self.go(&text);
            }
            _ => {}
        }
    }

    /// The three-dot menu as an overlay: opening it must not move the page.
    /// The native webview is hidden while it is open (see `sync_page`).
    fn menu_overlay(&mut self, ctx: &egui::Context) -> Option<MenuAction> {
        if !self.menu_open {
            return None;
        }
        let pal = self.pal.clone();
        let anchor = self.menu_anchor.unwrap_or_else(|| {
            Rect::from_min_size(
                Pos2::new(ctx.content_rect().right() - 48.0, 46.0),
                Vec2::ZERO,
            )
        });
        let mut action = None;
        let mut menu_rect = Rect::NOTHING;
        let t = ctx.animate_bool_with_time(egui::Id::unique("menu-anim"), true, 0.12);

        egui::Area::new(egui::Id::unique("menu-overlay"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(Pos2::new(anchor.right(), anchor.bottom() + 4.0))
            .show(ctx, |ui| {
                let frame = Frame::new()
                    .fill(pal.surface_container)
                    .stroke(Stroke::new(1.0, pal.outline_variant))
                    .corner_radius(CornerRadius::same(self.cfg.theme.radius.min(16)))
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_opacity(t);
                        ui.set_width(232.0);
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() * 0.72).max(220.0))
                            .show(ui, |ui| {
                                if menu_row(ui, &pal, "New tab   Ctrl+T") {
                                    action = Some(MenuAction::NewTab);
                                }
                                if menu_row(ui, &pal, "Reload") {
                                    action = Some(MenuAction::Reload);
                                }
                                if menu_row(ui, &pal, "Close tab   Ctrl+W") {
                                    action = Some(MenuAction::CloseTab);
                                }
                                ui.separator();
                                let incognito = if self.cfg.incognito {
                                    "Incognito mode   on"
                                } else {
                                    "Incognito mode"
                                };
                                if menu_row(ui, &pal, incognito) {
                                    action = Some(MenuAction::Incognito);
                                }
                                if menu_row(ui, &pal, "Unload background tabs") {
                                    action = Some(MenuAction::Unload);
                                }
                                ui.separator();
                                if menu_row(ui, &pal, "Theme") {
                                    action = Some(MenuAction::Theme);
                                }
                                if menu_row(ui, &pal, "scrapeconfig") {
                                    action = Some(MenuAction::Config);
                                }
                                if menu_row(
                                    ui,
                                    &pal,
                                    if self.setup {
                                        "Leave setup mode"
                                    } else {
                                        "Setup mode"
                                    },
                                ) {
                                    action = Some(MenuAction::Setup);
                                }
                            });
                    });
                menu_rect = frame.response.rect;
            });

        // Click-away and Escape close it; a click on the dots is left to the toolbar.
        let clicked = ctx.input(|i| i.pointer.any_click());
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let on_anchor = pointer.is_some_and(|p| anchor.expand(3.0).contains(p));
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            || (clicked
                && pointer.is_some_and(|p| !menu_rect.expand(2.0).contains(p) && !on_anchor))
        {
            self.menu_open = false;
        }
        action
    }

    fn menu_action(&mut self, menu: MenuAction) {
        self.menu_open = false;
        match menu {
            MenuAction::Config => {
                self.setup = false;
                self.view = if self.view == View::Config {
                    View::Browser
                } else {
                    View::Config
                };
            }
            MenuAction::Theme => self.show_theme = !self.show_theme,
            MenuAction::Setup => {
                if self.setup {
                    self.setup = false;
                    self.selected = None;
                } else {
                    self.enter_setup();
                }
            }
            MenuAction::NewTab => self.new_tab(),
            MenuAction::CloseTab => {
                let active = self.active;
                self.close_tab(active);
            }
            MenuAction::Reload => {
                let url = self.tabs[self.active].url.clone();
                match &self.tabs[self.active].page {
                    Some(p) => p.reload(),
                    None if !url.is_empty() => self.pending = Some(url),
                    None => {}
                }
            }
            MenuAction::Unload => {
                let mut count = 0;
                for (i, tab) in self.tabs.iter_mut().enumerate() {
                    if i != self.active && tab.page.take().is_some() {
                        count += 1;
                    }
                }
                self.notify(if count == 0 {
                    "No background tabs to unload.".to_owned()
                } else {
                    format!("Unloaded {count} background tab(s).")
                });
            }
            MenuAction::Incognito => {
                self.cfg.incognito = !self.cfg.incognito;
                self.incognito_warned = false;
                if self.cfg.incognito {
                    self.notify("Incognito on: no history, no page access for the agent.");
                } else {
                    self.notify("Incognito off.");
                }
            }
        }
    }

    fn notice_toast(&mut self, ctx: &egui::Context) {
        let Some((text, at)) = self.notice.clone() else {
            return;
        };
        if at.elapsed() > Duration::from_secs(5) {
            self.notice = None;
            return;
        }
        let pal = self.pal.clone();
        egui::Area::new(egui::Id::unique("notice"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 8.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(pal.primary_container)
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(text)
                                .small()
                                .color(pal.on_primary_container),
                        );
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn tabs_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let mut select = None;
        let mut close = None;
        let mut add = false;

        egui::Panel::left("tabs_rail")
            .resizable(true)
            .default_size(200.0)
            .frame(
                Frame::new()
                    .fill(pal.surface_container_low)
                    .inner_margin(Margin::symmetric(8, 8)),
            )
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Tabs").small().weak());
                for i in 0..self.tabs.len() {
                    let title = self.tabs[i].title.clone();
                    let (clicked, closed) = tab_chip(
                        ui,
                        &title,
                        i == self.active,
                        &pal,
                        self.cfg.theme.radius,
                        i,
                        f32::INFINITY,
                    );
                    if clicked {
                        select = Some(i);
                    }
                    if closed {
                        close = Some(i);
                    }
                }
                add = plus_button(ui, &pal);
            });

        if let Some(i) = select {
            self.select_tab(i);
        }
        if let Some(i) = close {
            self.close_tab(i);
        }
        if add {
            self.new_tab();
        }
    }

    fn theme_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let mut wall_pick = false;
        let mut wall_clear = false;
        let mut export = false;
        let mut import = false;

        egui::Panel::right("theme_panel")
            .resizable(true)
            .default_size(330.0)
            .frame(
                Frame::new()
                    .fill(pal.surface_container_low)
                    .inner_margin(Margin::same(14)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Theme");
                    ui.label(egui::RichText::new("Every change applies live.").small().weak());
                    ui.add_space(10.0);
                    preview_card(ui, &self.pal, &self.cfg.theme);
                    ui.add_space(14.0);

                    if ui.checkbox(&mut self.cfg.theme.dark, "Dark scheme").changed() {
                        self.dirty = true;
                    }
                    ui.horizontal(|ui| {
                        ui.label("Seed colour");
                        let mut seed = self.cfg.theme.seed;
                        if ui.color_edit_button_srgb(&mut seed).changed() {
                            self.cfg.theme.seed = seed;
                            self.cfg.theme.palette_from_wallpaper = false;
                            self.dirty = true;
                        }
                    });
                    if ui
                        .add(egui::Slider::new(&mut self.cfg.theme.radius, 0..=28).text("corner radius"))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    if ui
                        .add(
                            egui::Slider::new(&mut self.cfg.theme.font_scale, 0.7..=1.6)
                                .text("font scale"),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Pick wallpaper…").clicked() {
                            wall_pick = true;
                        }
                        if ui.button("Clear").clicked() {
                            wall_clear = true;
                        }
                    });
                    if let Some(path) = &self.cfg.theme.wallpaper {
                        if let Some(name) = std::path::Path::new(path).file_name() {
                            ui.label(egui::RichText::new(name.to_string_lossy()).small().weak());
                        }
                    }
                    if ui
                        .checkbox(
                            &mut self.cfg.theme.palette_from_wallpaper,
                            "Palette from wallpaper",
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Export .scrp").clicked() {
                            export = true;
                        }
                        if ui.button("Import .scrp").clicked() {
                            import = true;
                        }
                    });
                });
            });

        if wall_pick {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("image", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
            {
                if let Some(seed) = theme::wallpaper_seed(&path) {
                    self.cfg.theme.seed = seed;
                    self.cfg.theme.palette_from_wallpaper = true;
                }
                self.cfg.theme.wallpaper = Some(path.display().to_string());
                self.dirty = true;
            }
        }
        if wall_clear {
            self.cfg.theme.wallpaper = None;
            self.dirty = true;
        }
        if export {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("scrape.scrp")
                .add_filter("Scrape theme", &["scrp"])
                .save_file()
            {
                if let Ok(json) = serde_json::to_string_pretty(&self.cfg.theme) {
                    let _ = std::fs::write(path, json);
                }
            }
        }
        if import {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Scrape theme", &["scrp", "json"])
                .pick_file()
            {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    match serde_json::from_str::<Theme>(text.trim_start_matches('\u{feff}')) {
                        Ok(theme) => {
                            self.cfg.theme = theme;
                            self.dirty = true;
                        }
                        Err(e) => eprintln!("scrape: bad theme file: {e}"),
                    }
                }
            }
        }
    }

    fn agent_panel(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let mut send = false;
        let mut close = false;
        let busy = self.agent.busy;

        let mut open = self.show_agent;
        egui::Panel::right("agent_panel")
            .resizable(true)
            .drag_to_open(false)
            .default_size(380.0)
            .frame(
                Frame::new()
                    .fill(pal.surface_container_low)
                    .inner_margin(Margin::same(14)),
            )
            .show_collapsible(ui, &mut open, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Agent");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        if ui
                            .button("Settings")
                            .on_hover_text("Ctrl+Shift+A")
                            .clicked()
                        {
                            self.view = View::Config;
                        }
                    });
                });
                ui.label(
                    egui::RichText::new("Asks the page through your own model endpoint.")
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);

                egui::Panel::bottom("agent_input")
                    .frame(Frame::new().inner_margin(Margin::symmetric(0, 0)))
                    .show(ui, |ui| {
                        if let Some(err) = &self.agent.error {
                            ui.label(
                                egui::RichText::new(err)
                                    .small()
                                    .color(Color32::from_rgb(200, 60, 40)),
                            );
                        }
                        if self.cfg.agent_api_key.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("No API key yet.").small());
                                if ui.button("Open scrapeconfig").clicked() {
                                    self.view = View::Config;
                                }
                            });
                        }
                        ui.add_space(4.0);
                        self.agent_model_line(ui);
                        ui.add_space(4.0);
                        let reply = ui.add(
                            TextEdit::multiline(&mut self.agent.input)
                                .hint_text("Ask the agent…  Enter sends, Shift+Enter adds a line")
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        );
                        if reply.has_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                        {
                            send = !busy;
                        }
                        ui.add_space(6.0);
                        self.agent_context_bar(ui);
                    });

                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.agent.msgs.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Try: “summarise this page”, “find the pricing link and open it”.",
                                )
                                .small()
                                .color(pal.on_surface_variant),
                            );
                        }
                        for (from_user, text) in &self.agent.msgs {
                            let fill = if *from_user {
                                pal.primary_container
                            } else {
                                pal.surface_container
                            };
                            let color = if *from_user {
                                pal.on_primary_container
                            } else {
                                pal.on_surface
                            };
                            Frame::new()
                                .fill(fill)
                                .corner_radius(CornerRadius::same(self.cfg.theme.radius.min(18)))
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_max_width(ui.available_width() - 14.0);
                                    ui.label(egui::RichText::new(text).color(color));
                                });
                            ui.add_space(6.0);
                        }
                    });
            });

        // The panel also reports a dragged-shut edge, so keep the flag in sync.
        self.show_agent = open && !close;
        if send {
            if !self.cfg.agent_permissions_granted {
                self.permissions_open = true;
            } else if self.cfg.incognito {
                self.warn_incognito("Incognito: the agent cannot read pages in this mode.");
                self.agent
                    .ask(&self.cfg, "Incognito is on: no page context is available.".into());
            } else {
                let context = self.page_context();
                self.agent.ask(&self.cfg, context);
            }
        }
    }

    /// Model and activity, directly above the input.
    fn agent_model_line(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let model = if self.agent.model.is_empty() {
            self.cfg.agent_model.clone()
        } else {
            self.agent.model.clone()
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(model).small().strong());
            ui.label(egui::RichText::new("·").small().weak());
            let status = egui::RichText::new(self.agent.status.label()).small();
            ui.label(if self.agent.busy {
                status.color(pal.primary)
            } else {
                status.weak()
            });
        });
    }

    /// Context fill, under the input and as wide as the panel allows.
    fn agent_context_bar(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let (used, window) = (self.agent.used, self.cfg.agent_context_tokens.max(1));
        let fill = (used as f32 / window as f32).clamp(0.0, 1.0);
        ui.horizontal(|ui| {
            let bar_width = (ui.available_width() - 48.0).max(40.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(bar_width, 6.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(3), pal.surface_container_highest);
            if fill > 0.0 {
                p.rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(rect.width() * fill, 6.0)),
                    CornerRadius::same(3),
                    pal.primary,
                );
            }
            ui.label(egui::RichText::new(format!("{:.0}%", fill * 100.0)).small().weak())
                .on_hover_text(format!("{used} / {window} tokens of context"));
        });
    }

    fn page_context(&self) -> String {
        let tab = &self.tabs[self.active];
        if tab.url.is_empty() {
            "The user is on the new-tab page; no page is loaded.".into()
        } else {
            format!("Title: {}\nURL: {}", tab.title, tab.url)
        }
    }

    fn permission_modal(&mut self, ctx: &egui::Context) {
        let mut allow = false;
        let mut deny = false;
        let _ = egui::Modal::new(egui::Id::unique("agent-perms")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading("Let the agent use the page?");
            ui.add_space(6.0);
            ui.label(
                "The agent can read the visible text of the page, click links and fill inputs, \
                 and it sends that text to the model endpoint from your settings.",
            );
            ui.add_space(5.0);
            ui.label(egui::RichText::new("Nothing runs until you press Allow.").small().weak());
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button("Allow").clicked() {
                    allow = true;
                }
                if ui.button("Not now").clicked() {
                    deny = true;
                }
            });
        });

        if allow {
            self.cfg.agent_permissions_granted = true;
            self.permissions_open = false;
            self.dirty = true;
        } else if deny || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.permissions_open = false;
            self.show_agent = false;
        }
    }

    fn central(&mut self, ui: &mut Ui) {
        let pal = self.pal.clone();
        let mut content_rect = None;

        egui::CentralPanel::default()
            .frame(Frame::new().fill(pal.surface).inner_margin(Margin::ZERO))
            .show(ui, |ui| {
                if self.view == View::Config {
                    self.config_page(ui);
                } else if self.is_welcome() {
                    self.welcome(ui);
                } else {
                    // Small inset keeps the native page off the window edge, so the
                    // resize handles stay clickable.
                    let rect = ui.max_rect().shrink(5.0);
                    content_rect = Some(rect);
                    if self.tabs[self.active].page.is_none() {
                        let text = self
                            .page_error
                            .clone()
                            .unwrap_or_else(|| "Loading…".to_owned());
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            text,
                            FontId::proportional(15.0),
                            pal.on_surface_variant,
                        );
                    }
                }
            });

        self.content_rect = content_rect;
    }

    fn enter_setup(&mut self) {
        self.view = View::Browser;
        if !self.is_welcome() {
            self.new_tab();
        }
        self.setup = true;
        self.selected = None;
        self.welcome_at = Instant::now();
    }

    fn welcome(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.favicons.poll(&ctx);
        let pal = self.pal.clone();
        let area = ui.max_rect();
        let t = self.welcome_at.elapsed().as_secs_f32();

        if let Some((tex, size)) = self.wall.texture(&ctx, &self.cfg.theme) {
            let uv = cover_uv(size, area.size());
            ui.painter().image(tex, area, uv, Color32::WHITE);
            let scrim = if self.cfg.theme.dark { 205 } else { 185 };
            ui.painter()
                .rect_filled(area, CornerRadius::ZERO, rgba(pal.surface, scrim));
        }

        let engine_name = self.cfg.search_engine.clone();
        let engine = engines::get(&engine_name);
        let radius = self.cfg.theme.radius;
        let clock_cfg = self.cfg.clock.clone();
        let logo_pref = self.cfg.logo.preference;
        let mut changed = false;
        let mut submitted = false;
        let mut picked: Option<&'static str> = None;
        let mut search_rect: Option<Rect> = None;
        let mut go_site: Option<String> = None;
        let mut add_site = false;
        let mut search_focus = false;

        // The logo has one size on every window; only the widgets follow it. When the
        // window is too short for them, they are left out instead of shrinking the logo.
        let fit = (area.width() / 1200.0).clamp(0.55, 1.0);
        let size_of = |item: &config::WelcomeItem| {
            welcome::item_size(item.kind, logo_pref, clock_cfg.orientation, &self.cfg.welcome)
                * item.scale
                * if item.kind == ItemKind::Logo { 1.0 } else { fit }
        };
        let mut canvas = Rect::from_center_size(area.center(), Vec2::new(1200.0, 800.0) * fit);
        let top = self.cfg.welcome.items.iter().fold(f32::INFINITY, |acc, item| {
            acc.min(item.y * canvas.height() - size_of(item).y / 2.0)
        });
        let shift = (area.top() + 16.0) - (canvas.top() + top);
        if shift > 0.0 {
            canvas = canvas.translate(Vec2::new(0.0, shift));
        }

        let mut placed = Vec::with_capacity(self.cfg.welcome.items.len());
        for item in &self.cfg.welcome.items {
            let mut size = size_of(item);
            if item.kind == ItemKind::Search {
                size.y = size.y.max(38.0);
            }
            placed.push(item_rect(canvas, item.x, item.y, size));
        }
        // Widgets keep exactly the place they were given; nothing pushes anything else
        // around. Only a widget that would end below the window is left out.
        let limit = area.bottom() - 12.0;
        let hidden: Vec<bool> = placed.iter().map(|r| r.bottom() > limit).collect();

        for i in 0..self.cfg.welcome.items.len() {
            let mut item = self.cfg.welcome.items[i];
            let appear = ease((t - i as f32 * 0.045) / 0.42);
            if appear < 0.01 || hidden[i] {
                continue;
            }
            let rect = placed[i].translate(Vec2::new(0.0, (1.0 - appear) * 18.0));

            match item.kind {
                ItemKind::Clock => welcome::paint_clock(ui.painter(), rect, &clock_cfg, &pal, appear),
                ItemKind::Logo => welcome::paint_logo(
                    ui.painter(),
                    rect,
                    logo_pref,
                    self.logo.as_ref(),
                    welcome::fade(pal.on_surface, appear),
                ),
                ItemKind::Splash => welcome::paint_splash(
                    ui.painter(),
                    rect,
                    &self.cfg,
                    &pal,
                    self.welcome_seed,
                    appear,
                ),
                ItemKind::Phrases => welcome::paint_phrases(
                    ui.painter(),
                    rect,
                    &pal,
                    self.welcome_seed,
                    t,
                    appear,
                ),
                ItemKind::Todo => {
                    let todo_style = self.cfg.welcome.todo_style;
                    if welcome::todo_ui(
                        ui,
                        rect,
                        todo_style,
                        &mut self.cfg.welcome.todos,
                        &pal,
                        self.setup,
                        appear,
                    ) {
                        changed = true;
                    }
                }
                ItemKind::Sites => {
                    let sites = self.cfg.welcome.sites.clone();
                    let (rows, cols) = (self.cfg.welcome.site_rows, self.cfg.welcome.site_cols);
                    let setup = self.setup;
                    let allow = !self.cfg.incognito;
                    let favicons = &mut self.favicons;
                    match welcome::sites_ui(
                        ui,
                        rect,
                        &sites,
                        rows,
                        cols,
                        &pal,
                        setup,
                        |u| favicons.texture(&ctx, u, allow),
                    ) {
                        Some(welcome::SitesHit::Open(url)) => go_site = Some(url),
                        Some(welcome::SitesHit::Add) => add_site = true,
                        None => {}
                    }
                }
                ItemKind::Search => {
                    search_rect = Some(rect);
                    let (sub, badge, _, focus) = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                            ui.multiply_opacity(appear);
                            engines::search_field(
                                ui,
                                &mut self.url_buf,
                                engine,
                                &pal,
                                &self.icons,
                                radius,
                                rect.width(),
                                rect.height(),
                                self.addr_id.with("welcome"),
                                "",
                            )
                        })
                        .inner;
                    search_focus = focus;
                    if sub {
                        submitted = true;
                    }
                    if badge {
                        self.engine_open = !self.engine_open;
                    }
                }
            }

            if self.setup {
                let resp = ui.interact(
                    rect,
                    egui::Id::unique(("welcome-item", i)),
                    Sense::click_and_drag(),
                );
                if resp.clicked() || resp.drag_started() {
                    self.selected = Some(i);
                }
                if resp.dragged() {
                    let d = resp.drag_delta();
                    item.x = (item.x + d.x / canvas.width()).clamp(0.0, 1.0);
                    item.y = (item.y + d.y / canvas.height()).clamp(0.0, 1.0);
                    self.cfg.welcome.items[i] = item;
                    changed = true;
                }
                if self.selected == Some(i) {
                    ui.painter().rect_stroke(
                        rect.expand(6.0),
                        CornerRadius::same(10),
                        Stroke::new(1.5, pal.primary),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().text(
                        rect.left_top() + Vec2::new(0.0, -12.0),
                        Align2::LEFT_BOTTOM,
                        welcome::kind_name(item.kind),
                        FontId::proportional(11.0),
                        pal.primary,
                    );
                }
            }
        }

        // Recent searches belong to the field, not to the three-dot menu.
        if search_focus && !self.engine_open && !self.cfg.history.is_empty() {
            if let Some(field) = search_rect {
                let history: Vec<String> = self.cfg.history.iter().take(5).cloned().collect();
                let mut pick: Option<String> = None;
                let mut clear = false;
                egui::Area::new(egui::Id::unique("welcome-recent"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(Pos2::new(field.left(), field.bottom() + 6.0))
                    .show(&ctx, |ui| {
                        Frame::new()
                            .fill(pal.surface_container)
                            .stroke(Stroke::new(1.0, pal.outline_variant))
                            .corner_radius(CornerRadius::same(radius.min(16)))
                            .inner_margin(Margin::same(6))
                            .show(ui, |ui| {
                                ui.set_width(field.width() - 12.0);
                                for query in &history {
                                    if menu_row(ui, &pal, &format!("· {}", elide(query, 44))) {
                                        pick = Some(query.clone());
                                    }
                                }
                                ui.separator();
                                if menu_row(ui, &pal, "Clear history") {
                                    clear = true;
                                }
                            });
                    });
                if let Some(query) = pick {
                    self.go(&query);
                }
                if clear {
                    self.cfg.history.clear();
                    self.dirty = true;
                }
            }
        }

        if self.engine_open {
            if let Some(field) = search_rect {
                let w = (area.width() - 40.0).min(760.0);
                let total = engines::row_width(ui);
                let rows = if total <= w { 1.0 } else { 2.0 };
                let area_w = if rows == 1.0 { total } else { w };
                let chips = Rect::from_min_size(
                    Pos2::new(field.center().x - area_w / 2.0, field.bottom() + 10.0),
                    Vec2::new(area_w, rows * 38.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(chips), |ui| {
                    picked = engines::chips(ui, &self.cfg.search_engine, &pal, &self.icons);
                });
            }
        }

        if let Some(name) = picked {
            self.cfg.search_engine = name.to_owned();
            self.engine_open = false;
            self.dirty = true;
        }
        if submitted {
            let text = self.url_buf.clone();
            self.go(&text);
        }
        if let Some(url) = go_site {
            self.go(&url);
        }
        if add_site {
            let cap = welcome::site_cap(self.cfg.welcome.site_rows, self.cfg.welcome.site_cols);
            if self.cfg.welcome.sites.len() < cap {
                self.cfg.welcome.sites.push(String::new());
                self.enter_setup();
                self.selected = self
                    .cfg
                    .welcome
                    .items
                    .iter()
                    .position(|it| it.kind == ItemKind::Sites);
                changed = true;
            } else {
                self.notify(format!("{cap} sites is the limit here, remove one first."));
            }
        }
        if changed {
            self.dirty = true;
        }
        if self.setup {
            self.setup_panel(&ctx, area);
        }

        // Star, splash beat and clock seconds keep moving; the entrance runs at full rate.
        ctx.request_repaint_after(Duration::from_millis(if t < 1.2 { 16 } else { 80 }));
    }

    /// Top-right badge plus the inspector for the selected welcome element.
    fn setup_panel(&mut self, ctx: &egui::Context, area: Rect) {
        let pal = self.pal.clone();
        let mut close = false;
        let mut reset = false;
        let mut changed = false;

        egui::Area::new(egui::Id::unique("setup-panel"))
            .order(egui::Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(Pos2::new(area.right() - 12.0, area.top() + 12.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(rgba(pal.surface_container, 246))
                    .stroke(Stroke::new(1.0, pal.outline_variant))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_max_width(310.0);
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                            ui.painter().circle_filled(
                                dot.center(),
                                4.0,
                                Color32::from_rgb(0xD7, 0x19, 0x21),
                            );
                            ui.label(egui::RichText::new("SETUP MODE").small().strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("Done").clicked() {
                                    close = true;
                                }
                                if ui.button("reset").clicked() {
                                    reset = true;
                                }
                            });
                        });

                        // No ScrollArea here: an Area sizes to its content, and a
                        // ScrollArea inside one would size itself from that same rect.
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("drag anything · click to edit")
                                .small()
                                .color(pal.on_surface_variant),
                        );

                        if let Some(i) = self.selected.filter(|i| *i < self.cfg.welcome.items.len()) {
                            let mut item = self.cfg.welcome.items[i];
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(welcome::kind_name(item.kind)).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.button("remove").clicked() {
                                        self.cfg.welcome.items.remove(i);
                                        self.selected = None;
                                        changed = true;
                                    }
                                });
                            });
                            // "remove" above may have emptied the slot this frame.
                            if self.selected.is_some() && self.cfg.welcome.items.get(i).is_some() {
                                if ui
                                    .add(
                                        egui::Slider::new(&mut item.scale, 0.5..=2.0)
                                            .text("size"),
                                    )
                                    .changed()
                                {
                                    self.cfg.welcome.items[i] = item;
                                    changed = true;
                                }
                                changed |= self.item_editor(ui, i);
                            }
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.label(egui::RichText::new("add").small().weak());
                        let mut missing = false;
                        ui.horizontal_wrapped(|ui| {
                            for kind in [
                                ItemKind::Clock,
                                ItemKind::Logo,
                                ItemKind::Splash,
                                ItemKind::Search,
                                ItemKind::Phrases,
                                ItemKind::Todo,
                                ItemKind::Sites,
                            ] {
                                if self.cfg.welcome.items.iter().any(|it| it.kind == kind) {
                                    continue;
                                }
                                missing = true;
                                if ui.button(welcome::kind_name(kind)).clicked() {
                                    self.cfg.welcome.items.push(config::WelcomeItem {
                                        kind,
                                        x: 0.5,
                                        y: 0.5,
                                        scale: 1.0,
                                    });
                                    self.selected = Some(self.cfg.welcome.items.len() - 1);
                                    changed = true;
                                }
                            }
                        });
                        if !missing {
                            ui.label(
                                egui::RichText::new("every element is on the screen")
                                    .small()
                                    .color(pal.outline),
                            );
                        }
                    });
            });

        if reset {
            self.cfg.welcome = config::Welcome::default();
            self.selected = None;
            changed = true;
        }
        if close {
            self.setup = false;
            self.selected = None;
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }

    /// Controls for one welcome element, chosen by its kind.
    fn item_editor(&mut self, ui: &mut Ui, i: usize) -> bool {
        let mut changed = false;
        let kind = self.cfg.welcome.items[i].kind;
        match kind {
            ItemKind::Splash => {
                ui.horizontal(|ui| {
                    for (mode, label) in [
                        (SplashMode::Off, "off"),
                        (SplashMode::On, "on"),
                        (SplashMode::Custom, "custom"),
                    ] {
                        changed |= ui
                            .selectable_value(&mut self.cfg.splash.mode, mode, label)
                            .changed();
                    }
                });
                if self.cfg.splash.mode == SplashMode::Custom {
                    let mut text = self.cfg.splash.custom_splash.join(", ");
                    if ui
                        .add(
                            TextEdit::singleline(&mut text)
                                .hint_text("\"one\", \"two\"")
                                .desired_width(f32::INFINITY),
                        )
                        .changed()
                    {
                        self.cfg.splash.custom_splash = text
                            .split(',')
                            .map(|s| s.trim().trim_matches('"').to_owned())
                            .filter(|s| !s.is_empty())
                            .collect();
                        changed = true;
                    }
                }
            }
            ItemKind::Logo => {
                ui.horizontal(|ui| {
                    for (pref, label) in [
                        (LogoPreference::Basic, "basic"),
                        (LogoPreference::Plain, "plain"),
                        (LogoPreference::Big, "big"),
                    ] {
                        changed |= ui
                            .selectable_value(&mut self.cfg.logo.preference, pref, label)
                            .changed();
                    }
                });
            }
            ItemKind::Clock => {
                ui.horizontal(|ui| {
                    for (orientation, label) in [
                        (config::ClockOrientation::Horizontal, "00:00"),
                        (config::ClockOrientation::Vertical, "vertical"),
                    ] {
                        changed |= ui
                            .selectable_value(
                                &mut self.cfg.clock.orientation,
                                orientation,
                                label,
                            )
                            .changed();
                    }
                });
                changed |= ui
                    .checkbox(&mut self.cfg.clock.seconds, "seconds")
                    .changed();
                egui::ComboBox::from_id_salt("clock-font")
                    .selected_text(if self.cfg.clock.font.is_empty() {
                        "anthropic serif".to_owned()
                    } else {
                        self.cfg.clock.font.clone()
                    })
                    .show_ui(ui, |ui| {
                        for name in ["anthropic serif", "google sans", "geist"] {
                            changed |= ui
                                .selectable_value(
                                    &mut self.cfg.clock.font,
                                    name.to_owned(),
                                    name,
                                )
                                .changed();
                        }
                    });
                changed |= ui
                    .add(
                        TextEdit::singleline(&mut self.cfg.clock.font)
                            .hint_text("or a .ttf path")
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
            }
            ItemKind::Todo => {
                ui.horizontal(|ui| {
                    changed |= ui
                        .selectable_value(&mut self.cfg.welcome.todo_style, TodoStyle::Nothing, "Nothing")
                        .changed();
                    changed |= ui
                        .selectable_value(&mut self.cfg.welcome.todo_style, TodoStyle::Claude, "Claude")
                        .changed();
                });
                let mut remove = None;
                for (j, todo) in self.cfg.welcome.todos.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        changed |= ui.checkbox(&mut todo.done, "").changed();
                        changed |= ui
                            .add(TextEdit::singleline(&mut todo.text).desired_width(180.0))
                            .changed();
                        if ui.button("×").clicked() {
                            remove = Some(j);
                        }
                    });
                }
                if let Some(j) = remove {
                    self.cfg.welcome.todos.remove(j);
                    changed = true;
                }
                if ui.button("+ task").clicked() {
                    self.cfg.welcome.todos.push(config::TodoItem {
                        text: String::new(),
                        done: false,
                    });
                    changed = true;
                }
            }
            ItemKind::Sites => {
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.cfg.welcome.site_rows)
                                .range(1..=8)
                                .suffix(" rows"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.cfg.welcome.site_cols)
                                .range(1..=8)
                                .suffix(" cols"),
                        )
                        .changed();
                });
                let mut remove = None;
                egui::ScrollArea::vertical()
                    .id_salt("setup-sites")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (j, url) in self.cfg.welcome.sites.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .add(
                                        TextEdit::singleline(url)
                                            .hint_text("https://…")
                                            .desired_width(190.0),
                                    )
                                    .changed();
                                if ui.button("×").clicked() {
                                    remove = Some(j);
                                }
                            });
                        }
                    });
                if let Some(j) = remove {
                    self.cfg.welcome.sites.remove(j);
                    changed = true;
                }
                if ui.button("+ site").clicked() {
                    self.cfg.welcome.sites.push(String::new());
                    changed = true;
                }
            }
            ItemKind::Search | ItemKind::Phrases => {}
        }
        changed
    }

    fn config_page(&mut self, ui: &mut Ui) {
        if !self.permissions_open
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.view = View::Browser;
            return;
        }
        let rect = ui.max_rect();
        let ctx = ui.ctx().clone();
        if let Some((tex, size)) = self.wall.texture(&ctx, &self.cfg.theme) {
            let uv = cover_uv(size, rect.size());
            ui.painter().image(tex, rect, uv, Color32::WHITE);
            let s = self.pal.surface;
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, rgba(s, 228));
        }
        let mut close = false;
        Frame::new()
            .inner_margin(Margin::same(18))
            .show(ui, |ui| {
                if scrapeconfig::show(ui, &mut self.cfg, &mut self.search, &mut close) {
                    self.dirty = true;
                }
            });
        if close {
            self.view = View::Browser;
        }
    }


    /// Six invisible strips on the window edges. An undecorated winit window has no
    /// native resize border, so these hand the drag to the OS.
    fn resize_edges(&mut self, ctx: &egui::Context) {
        use egui::{CursorIcon, ResizeDirection as Dir, ViewportCommand};
        if self.maximized {
            return;
        }
        let s = ctx.content_rect();
        let b = 6.0;
        let c = 16.0;
        let (l, r, t, bo) = (s.left(), s.right(), s.top(), s.bottom());
        let handles: [(Dir, Rect, CursorIcon); 8] = [
            (
                Dir::NorthWest,
                Rect::from_min_size(Pos2::new(l, t), Vec2::splat(c)),
                CursorIcon::ResizeNwSe,
            ),
            (
                Dir::NorthEast,
                Rect::from_min_size(Pos2::new(r - c, t), Vec2::splat(c)),
                CursorIcon::ResizeNeSw,
            ),
            (
                Dir::SouthWest,
                Rect::from_min_size(Pos2::new(l, bo - c), Vec2::splat(c)),
                CursorIcon::ResizeNeSw,
            ),
            (
                Dir::SouthEast,
                Rect::from_min_size(Pos2::new(r - c, bo - c), Vec2::splat(c)),
                CursorIcon::ResizeNwSe,
            ),
            (
                Dir::West,
                Rect::from_min_max(Pos2::new(l, t + c), Pos2::new(l + b, bo - c)),
                CursorIcon::ResizeHorizontal,
            ),
            (
                Dir::East,
                Rect::from_min_max(Pos2::new(r - b, t + c), Pos2::new(r, bo - c)),
                CursorIcon::ResizeHorizontal,
            ),
            (
                Dir::North,
                Rect::from_min_max(Pos2::new(l + c, t), Pos2::new(r - c, t + b)),
                CursorIcon::ResizeVertical,
            ),
            (
                Dir::South,
                Rect::from_min_max(Pos2::new(l + c, bo - b), Pos2::new(r - c, bo)),
                CursorIcon::ResizeVertical,
            ),
        ];

        egui::Area::new(egui::Id::unique("resize-handles"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                for (dir, rect, cursor) in handles {
                    let resp = ui.interact(rect, egui::Id::unique(("resize", dir as u8)), Sense::drag());
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(cursor);
                    }
                    if resp.drag_started() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
                    }
                }
            });
    }

    /// Applies the pending navigation, then keeps the active page and the active tab in sync.
    fn sync_page(&mut self, ctx: &egui::Context) {
        let ppp = ctx.pixels_per_point();
        let active = self.active;
        let hidden = self.permissions_open || self.menu_open;

        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i != active {
                if let Some(p) = &mut tab.page {
                    p.set_visible(false);
                }
            }
        }

        if let Some(url) = self.pending.take() {
            self.tabs[active].url = url.clone();
            if let Some(p) = &self.tabs[active].page {
                p.navigate(&url);
            }
        }

        let Some(rect) = self.content_rect.filter(|_| !hidden) else {
            if let Some(p) = &mut self.tabs[active].page {
                p.set_visible(false);
            }
            return;
        };
        if self.tabs[active].url.is_empty() {
            if let Some(p) = &mut self.tabs[active].page {
                p.set_visible(false);
            }
            return;
        }

        let bounds = page::rect(
            rect.min.x * ppp,
            rect.min.y * ppp,
            rect.width() * ppp,
            rect.height() * ppp,
        );

        if self.tabs[active].page.is_none() {
            let url = self.tabs[active].url.clone();
            let titles = self.tabs[active].titles.clone();
            match page::Page::new(self.parent, &url, bounds, titles, ctx) {
                Ok(p) => {
                    self.tabs[active].page = Some(p);
                    self.page_error = None;
                }
                Err(e) => {
                    self.page_error = Some(e);
                    return;
                }
            }
        }
        if let Some(p) = &mut self.tabs[active].page {
            p.set_visible(true);
            p.set_bounds(bounds);
            if let Some(url) = p.url() {
                if !url.is_empty() && url != "about:blank" && url != self.tabs[active].url {
                    self.tabs[active].url = url.clone();
                    let focus = ctx.memory(|m| {
                        m.has_focus(self.addr_id) || m.has_focus(self.addr_id.with("welcome"))
                    });
                    if !focus {
                        self.url_buf = url;
                    }
                }
            }
        }
        let title = self.tabs[active].titles.lock().ok().map(|t| t.clone());
        if let Some(title) = title {
            if !title.is_empty() && title != self.tabs[active].title {
                self.tabs[active].title = title;
            }
        }
    }

    /// Inactivity and memory pressure both unload background webviews; the tab keeps its url.
    fn unload_tabs(&mut self, ctx: &egui::Context) {
        if self.last_unload.elapsed() < Duration::from_secs(20) {
            return;
        }
        self.last_unload = Instant::now();

        let whitelist = &self.cfg.unload_whitelist;
        let after = Duration::from_secs(self.cfg.unload_after_minutes.max(1) as u64 * 60);
        let mut unloaded = 0;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i == self.active || tab.page.is_none() {
                continue;
            }
            let allowed = tab.url.is_empty()
                || !whitelist
                    .iter()
                    .any(|d| !d.is_empty() && tab.url.contains(d.as_str()));
            if !allowed || tab.last_active.elapsed() < after {
                continue;
            }
            tab.page = None;
            unloaded += 1;
        }

        let limit = self.cfg.memory_limit_mb as u64;
        while limit > 0 && process_memory_mb() > limit {
            let mut victim: Option<(usize, Instant)> = None;
            for (i, tab) in self.tabs.iter().enumerate() {
                if i == self.active || tab.page.is_none() || tab.url.is_empty() {
                    continue;
                }
                if whitelist
                    .iter()
                    .any(|d| !d.is_empty() && tab.url.contains(d.as_str()))
                {
                    continue;
                }
                if victim
                    .map(|(_, at): (usize, Instant)| tab.last_active < at)
                    .unwrap_or(true)
                {
                    victim = Some((i, tab.last_active));
                }
            }
            let Some((i, _)) = victim else { break };
            self.tabs[i].page = None;
            unloaded += 1;
        }

        if unloaded > 0 {
            ctx.request_repaint();
        }
    }

    fn run_tool(&mut self, call: agent::ToolCall) {
        let ok = |msg: &str| Ok(msg.to_owned());
        if self.cfg.incognito && call.name != "open_url" {
            self.warn_incognito("Incognito: the agent tried to read the page; blocked.");
            self.agent
                .tool_done(Err("incognito: page tools are disabled".into()));
            return;
        }
        if call.name == "open_url" {
            let url = self.cfg.search(call.args["url"].as_str().unwrap_or(""));
            if url.is_empty() {
                self.agent.tool_done(Err("no url given".into()));
            } else {
                self.go(&url);
                self.agent.tool_done(ok(&format!("navigated to {url}")));
            }
            return;
        }
        let Some(js) = agent::script(&call) else {
            self.agent
                .tool_done(Err(format!("unknown tool {}", call.name)));
            return;
        };
        let Some(page) = &self.tabs[self.active].page else {
            self.agent
                .tool_done(Err("no page is open; use open_url first".into()));
            return;
        };
        let tx = self.agent.tool_sender();
        page.eval(&js, move |raw| {
            let _ = tx.send(Ok(agent::unwrap_result(raw)));
        });
    }

    fn flush(&mut self, ctx: &egui::Context) {
        if self.cfg.clock.font != self.last_clock_font {
            self.last_clock_font = self.cfg.clock.font.clone();
            theme::install_fonts(ctx, &self.cfg.clock.font);
            self.dirty = true;
        }
        if self.cfg.theme != self.last_theme {
            self.pal = Palette::from_theme(&self.cfg.theme);
            theme::apply_style(ctx, &self.pal, &self.cfg.theme);
            self.last_theme = self.cfg.theme.clone();
            self.dirty = true;
        }
        if self.dirty {
            self.cfg.save();
            self.dirty = false;
        }
    }
}

impl eframe::App for App {
    /// Paints the sliver of window that egui panels do not cover.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.pal.surface_container_low.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(self.maximized));
        self.hotkeys(&ctx);

        let welcome = self.is_welcome();
        if welcome && !self.was_welcome {
            self.welcome_visit();
        }
        self.was_welcome = welcome;

        self.title_bar(ui);
        if self.cfg.theme.tabs_position == theme::TabsPosition::Side {
            self.tabs_panel(ui);
        }
        self.toolbar(ui);
        if self.show_theme {
            self.theme_panel(ui);
        }
        // Always shown: the panel animates itself open and shut.
        self.agent_panel(ui);
        self.central(ui);
        if let Some(menu) = self.menu_overlay(&ctx) {
            self.menu_action(menu);
        }
        self.notice_toast(&ctx);
        if self.permissions_open {
            self.permission_modal(&ctx);
        }

        self.favicons.poll(&ctx);
        let (tools, agent_changed) = self.agent.poll();
        for call in tools {
            self.run_tool(call);
        }
        if agent_changed {
            ctx.request_repaint();
        }
        self.sync_page(&ctx);
        self.resize_edges(&ctx);
        self.unload_tabs(&ctx);
        self.flush(&ctx);

        // Keeps the unload timer alive while the window is idle.
        ctx.request_repaint_after(Duration::from_secs(20));
    }
}

fn rgba(c: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Changes on every visit, so splash and phrase strip pick a fresh line.
fn random_seed() -> f32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000_000) as f32 / 1000.0
}

/// Centre a welcome element at a fraction of the area, never letting it leave the area.
fn item_rect(area: Rect, x: f32, y: f32, size: Vec2) -> Rect {
    let size = Vec2::new(size.x.min(area.width()), size.y.min(area.height()));
    let cx = (area.left() + x * area.width())
        .clamp(area.left() + size.x / 2.0, area.right() - size.x / 2.0);
    let cy = (area.top() + y * area.height())
        .clamp(area.top() + size.y / 2.0, area.bottom() - size.y / 2.0);
    Rect::from_center_size(Pos2::new(cx, cy), size)
}

fn icon_button(
    ui: &mut Ui,
    pal: &Palette,
    active: bool,
    draw: impl Fn(&egui::Painter, Rect, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if active || resp.hovered() {
        let fill = if active {
            pal.primary_container
        } else {
            pal.surface_container_high
        };
        ui.painter().rect_filled(rect, CornerRadius::same(15), fill);
    }
    let color = if active {
        pal.on_primary_container
    } else if resp.hovered() {
        pal.on_surface
    } else {
        pal.on_surface_variant
    };
    draw(ui.painter(), rect, color);
    resp
}

fn paint_dots(p: &egui::Painter, rect: Rect, color: Color32) {
    for i in -1..=1 {
        p.circle_filled(rect.center() + Vec2::new(i as f32 * 5.0, 0.0), 1.7, color);
    }
}

fn elide(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

fn menu_row(ui: &mut Ui, pal: &Palette, label: &str) -> bool {    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), pal.surface_container_high);
    }
    ui.painter().text(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        if resp.hovered() {
            pal.on_surface
        } else {
            pal.on_surface_variant
        },
    );
    resp.clicked()
}

fn cover_uv(image: Vec2, area: Vec2) -> Rect {
    let image_ratio = image.x / image.y.max(1.0);
    let area_ratio = area.x / area.y.max(1.0);
    if image_ratio > area_ratio {
        let w = area_ratio / image_ratio;
        Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0))
    } else {
        let h = image_ratio / area_ratio;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - h) / 2.0), Pos2::new(1.0, (1.0 + h) / 2.0))
    }
}

enum WinButton {
    Min,
    Max,
    Close,
}

fn mini_logo(ui: &mut Ui, logo: Option<&egui::TextureHandle>, pal: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
    if let Some(logo) = logo {
        ui.painter().image(
            logo.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            pal.on_surface,
        );
    }
}

fn win_button(ui: &mut Ui, kind: WinButton, pal: &Palette, maximized: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 40.0), Sense::click());
    let close = matches!(kind, WinButton::Close);
    if resp.hovered() {
        let fill = if close {
            Color32::from_rgb(196, 43, 28)
        } else {
            pal.surface_container_high
        };
        ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
    }
    let color = if close && resp.hovered() {
        Color32::WHITE
    } else {
        pal.on_surface_variant
    };
    let stroke = Stroke::new(1.3, color);
    let c = rect.center();
    let p = ui.painter();
    match kind {
        WinButton::Min => {
            p.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
        }
        WinButton::Max => {
            let r = Rect::from_center_size(c, Vec2::splat(9.0));
            p.rect_stroke(r, CornerRadius::same(1), stroke, egui::StrokeKind::Inside);
            if maximized {
                // Restore glyph: the front square is clipped by the back one.
                let back = r.translate(Vec2::new(2.0, -2.0));
                p.line_segment([back.left_top(), back.right_top()], stroke);
                p.line_segment([back.right_top(), back.right_bottom()], stroke);
            }
        }
        WinButton::Close => {
            p.line_segment(
                [c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)],
                stroke,
            );
            p.line_segment(
                [c + Vec2::new(5.0, -5.0), c + Vec2::new(-5.0, 5.0)],
                stroke,
            );
        }
    }
    resp.clicked()
}

fn tab_chip(
    ui: &mut Ui,
    title: &str,
    active: bool,
    pal: &Palette,
    radius: u8,
    id: usize,
    max_width: f32,
) -> (bool, bool) {
    let color = if active {
        pal.on_primary_container
    } else {
        pal.on_surface_variant
    };
    let mut short = title.to_owned();
    let budget = if max_width.is_infinite() {
        ui.available_width()
    } else {
        max_width
    };
    let max_text = (budget.min(400.0) - 44.0).max(40.0);
    let mut galley = ui
        .painter()
        .layout_no_wrap(short.clone(), FontId::proportional(13.0), color);
    while galley.size().x > max_text && short.chars().count() > 1 {
        short = short.chars().take(short.chars().count() - 2).collect::<String>() + "…";
        galley = ui
            .painter()
            .layout_no_wrap(short.clone(), FontId::proportional(13.0), color);
    }
    let width = if max_width.is_infinite() {
        budget
    } else {
        (galley.size().x + 44.0).clamp(96.0, max_width)
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());

    let fill = if active {
        pal.primary_container
    } else if resp.hovered() {
        pal.surface_container_high
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius.min(15)), fill);
    let text_pos = Pos2::new(rect.left() + 12.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(text_pos, galley, color);

    let close_rect = Rect::from_center_size(
        Pos2::new(rect.right() - 14.0, rect.center().y),
        Vec2::splat(20.0),
    );
    let close_resp = ui.interact(close_rect, ui.scope_id().with(("tab-close", id)), Sense::click());
    if active || close_resp.hovered() {
        paint_close(ui.painter(), close_rect.shrink(5.0), color);
    }
    (resp.clicked() && !close_resp.hovered(), close_resp.clicked())
}

fn paint_close(painter: &egui::Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    let (a, b) = (rect.left_top(), rect.right_bottom());
    painter.line_segment([a, b], stroke);
    painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
}

fn plus_button(ui: &mut Ui, pal: &Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let color = if resp.hovered() {
        pal.on_surface
    } else {
        pal.on_surface_variant
    };
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(15), pal.surface_container_high);
    }
    let c = rect.center();
    let stroke = Stroke::new(1.5, color);
    ui.painter()
        .line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
    ui.painter()
        .line_segment([c + Vec2::new(0.0, -5.0), c + Vec2::new(0.0, 5.0)], stroke);
    resp.clicked()
}

fn chevron(ui: &mut Ui, right: bool, pal: &Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(15), pal.surface_container_high);
    }
    let c = rect.center();
    let stroke = Stroke::new(1.6, pal.on_surface_variant);
    let d = if right { 1.0 } else { -1.0 };
    ui.painter().line_segment(
        [c + Vec2::new(-2.5 * d, -4.5), c + Vec2::new(2.0 * d, 0.0)],
        stroke,
    );
    ui.painter().line_segment(
        [c + Vec2::new(2.0 * d, 0.0), c + Vec2::new(-2.5 * d, 4.5)],
        stroke,
    );
    resp.clicked()
}

/// Circular arrow: a gap at the top, arrowhead on the leading end.
fn reload_button(ui: &mut Ui, pal: &Palette) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(15), pal.surface_container_high);
    }
    let c = rect.center();
    let stroke = Stroke::new(1.6, pal.on_surface_variant);
    let r = 5.5;
    let start = -0.75f32;
    let sweep = std::f32::consts::TAU - 1.1;
    let steps = 28;
    let point = |a: f32| c + Vec2::new(a.cos(), a.sin()) * r;
    let arc: Vec<Pos2> = (0..=steps)
        .map(|i| point(start + sweep * i as f32 / steps as f32))
        .collect();
    ui.painter().add(egui::Shape::line(arc, stroke));

    let end = start + sweep;
    let tip = point(end);
    let tangent = Vec2::new(-end.sin(), end.cos());
    let radial = Vec2::new(end.cos(), end.sin());
    for side in [-1.0f32, 1.0] {
        let barb = (-tangent + radial * side * 0.7).normalized();
        ui.painter().line_segment([tip, tip + barb * 5.4], stroke);
    }
    resp.clicked()
}

fn preview_card(ui: &mut Ui, pal: &Palette, theme: &Theme) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 100.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(
        rect,
        CornerRadius::same(theme.radius as u8),
        pal.surface_container,
    );

    let chip = Rect::from_min_size(rect.min + Vec2::new(10.0, 10.0), Vec2::new(74.0, 20.0));
    p.rect_filled(chip, CornerRadius::same(10), pal.primary_container);
    p.text(
        chip.center(),
        Align2::CENTER_CENTER,
        "Tab",
        FontId::proportional(11.0),
        pal.on_primary_container,
    );

    let bar = Rect::from_min_size(rect.min + Vec2::new(10.0, 38.0), Vec2::new(width - 20.0, 26.0));
    p.rect_filled(bar, CornerRadius::same(13), pal.surface_container_high);
    p.text(
        bar.left_center() + Vec2::new(10.0, 0.0),
        Align2::LEFT_CENTER,
        "scrape://newtab",
        FontId::proportional(11.0),
        pal.on_surface_variant,
    );

    let btn = Rect::from_min_size(rect.min + Vec2::new(10.0, 72.0), Vec2::new(92.0, 20.0));
    p.rect_filled(btn, CornerRadius::same(10), pal.primary);
    p.text(
        btn.center(),
        Align2::CENTER_CENTER,
        "Action",
        FontId::proportional(11.0),
        pal.on_primary,
    );
    p.text(
        rect.min + Vec2::new(114.0, 82.0),
        Align2::LEFT_CENTER,
        "Body text",
        FontId::proportional(12.0),
        pal.on_surface,
    );
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([700.0, 420.0])
        .with_title("Scrape")
        .with_decorations(false);
    if let Some(icon) = assets::icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Scrape",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
