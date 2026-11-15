//! One JSON file holds every setting: scrapeconfig edits it, .scrp exports the theme part.

use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplashMode {
    Off,
    On,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogoPreference {
    Basic,
    Plain,
    Big,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStyle {
    Nothing,
    #[default]
    Claude,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Splash {
    pub mode: SplashMode,
    /// Used when mode is Custom; one phrase per entry.
    pub custom_splash: Vec<String>,
}

impl Default for Splash {
    fn default() -> Self {
        Self {
            mode: SplashMode::On,
            custom_splash: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Logo {
    pub preference: LogoPreference,
}

impl Default for Logo {
    fn default() -> Self {
        Self {
            preference: LogoPreference::Basic,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clock {
    /// "anthropic serif" | "google sans" | "geist" | a .ttf path.
    pub font: String,
    pub orientation: ClockOrientation,
    pub seconds: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            font: String::new(),
            orientation: ClockOrientation::Horizontal,
            seconds: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemKind {
    Clock,
    Logo,
    Splash,
    Search,
    Phrases,
    Todo,
    Sites,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WelcomeItem {
    pub kind: ItemKind,
    /// Centre, as a fraction of the welcome area.
    pub x: f32,
    pub y: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TodoItem {
    pub text: String,
    pub done: bool,
}

/// Free-form welcome screen: which pieces exist and where they sit.
/// Field defaults are per-field so an old config keeps its layout and still
/// gains the sites widget once, via `sites_migrated`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    #[serde(default)]
    pub items: Vec<WelcomeItem>,
    #[serde(default = "default_todos")]
    pub todos: Vec<TodoItem>,
    #[serde(default)]
    pub todo_style: TodoStyle,
    /// Favourite sites, one url each, shown by the sites widget.
    #[serde(default = "default_sites")]
    pub sites: Vec<String>,
    /// Grid capacity of the sites widget, set separately so there are no presets.
    #[serde(default = "one")]
    pub site_rows: u8,
    #[serde(default = "eight")]
    pub site_cols: u8,
    /// One-time flag: adds the sites widget to layouts saved before it existed.
    #[serde(default)]
    pub sites_migrated: bool,
}

fn one() -> u8 {
    1
}

fn eight() -> u8 {
    8
}

fn default_todos() -> Vec<TodoItem> {
    vec![
        TodoItem {
            text: "pick a wallpaper".into(),
            done: false,
        },
        TodoItem {
            text: "write the first todo".into(),
            done: false,
        },
    ]
}

fn default_sites() -> Vec<String> {
    vec![
        "https://www.google.com".into(),
        "https://www.youtube.com".into(),
        "https://github.com".into(),
        "https://www.reddit.com".into(),
        "https://en.wikipedia.org".into(),
        "https://duckduckgo.com".into(),
    ]
}

impl Default for Welcome {
    fn default() -> Self {
        let item = |kind, x, y| WelcomeItem {
            kind,
            x,
            y,
            scale: 1.0,
        };
        Self {
            items: vec![
                item(ItemKind::Logo, 0.5, 0.26),
                item(ItemKind::Splash, 0.5, 0.345),
                item(ItemKind::Search, 0.5, 0.425),
                item(ItemKind::Sites, 0.5, 0.53),
            ],
            todos: default_todos(),
            todo_style: TodoStyle::Claude,
            sites: default_sites(),
            site_rows: 1,
            site_cols: 8,
            sites_migrated: true,
        }
    }
}

impl Welcome {
    /// Old configs get the sites widget once, under the search field. If the user
    /// removes it later the flag keeps it removed.
    pub fn migrate(&mut self) {
        if self.sites_migrated {
            return;
        }
        self.sites_migrated = true;
        if !self.items.iter().any(|i| i.kind == ItemKind::Sites) {
            self.items.push(WelcomeItem {
                kind: ItemKind::Sites,
                x: 0.5,
                y: 0.53,
                scale: 1.0,
            });
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    /// Minutes of inactivity before a background tab is unloaded.
    pub unload_after_minutes: u32,
    /// Domains that are never unloaded, e.g. "mail.google.com".
    pub unload_whitelist: Vec<String>,
    pub memory_limit_mb: u32,
    /// Name from engines::ENGINES.
    pub search_engine: String,
    pub agent_base_url: String,
    pub agent_model: String,
    pub agent_api_key: String,
    /// Set once the user allows the agent to read and act on pages.
    pub agent_permissions_granted: bool,
    /// Remote debugging port for Playwright-style tools; 0 keeps it off.
    pub agent_cdp_port: u16,
    /// Context window of the model, the denominator of the fill bar under the chat.
    pub agent_context_tokens: u32,
    /// Keep recent searches for the menu. Off stores nothing.
    pub collect_history: bool,
    /// Newest first, capped on write.
    pub history: Vec<String>,
    /// Session-only private mode: no history, no page access for the agent.
    #[serde(skip)]
    pub incognito: bool,
    pub splash: Splash,
    pub logo: Logo,
    pub clock: Clock,
    pub welcome: Welcome,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            unload_after_minutes: 15,
            unload_whitelist: Vec::new(),
            memory_limit_mb: 800,
            search_engine: "DuckDuckGo".into(),
            agent_base_url: "https://api.deepseek.com".into(),
            agent_model: "deepseek-chat".into(),
            agent_api_key: String::new(),
            agent_permissions_granted: false,
            agent_cdp_port: 0,
            agent_context_tokens: 128_000,
            collect_history: true,
            history: Vec::new(),
            incognito: false,
            splash: Splash::default(),
            logo: Logo::default(),
            clock: Clock::default(),
            welcome: Welcome::default(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let dir = std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Scrape");
        dir.join("config.json")
    }

    pub fn load() -> Self {
        let mut cfg: Self = std::fs::read_to_string(Self::path())
            .ok()
            .map(|s| s.trim_start_matches('\u{feff}').to_owned())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        cfg.welcome.migrate();
        cfg
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    /// A bare host or scheme URL, as opposed to a search query.
    pub fn is_address(input: &str) -> bool {
        let input = input.trim();
        !input.contains(' ')
            && (input.contains('.') || input.starts_with("localhost") || input.contains(':'))
    }

    /// Address bar input -> url. Empty stays empty, which shows the welcome page.
    pub fn search(&self, input: &str) -> String {
        let input = input.trim();
        if input.is_empty() {
            return String::new();
        }
        if input.contains("://") {
            return input.to_owned();
        }
        if Self::is_address(input) {
            return format!("https://{input}");
        }
        crate::engines::get(&self.search_engine)
            .template
            .replace("{query}", &percent_encode(input))
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_urls_and_queries() {
        let cfg = Config::default();
        assert_eq!(cfg.search("example.com"), "https://example.com");
        assert_eq!(cfg.search("https://a.b/c"), "https://a.b/c");
        assert_eq!(cfg.search("rust 1.99"), "https://duckduckgo.com/?q=rust+1.99");
        assert_eq!(cfg.search(""), "");
    }

    #[test]
    fn picked_engine_changes_the_template() {
        let mut cfg = Config::default();
        cfg.search_engine = "Yandex".into();
        assert_eq!(cfg.search("rust"), "https://yandex.com/search/?text=rust");
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let cfg: Config = serde_json::from_str(r#"{"memory_limit_mb": 64}"#).unwrap();
        assert_eq!(cfg.memory_limit_mb, 64);
        assert_eq!(cfg.unload_after_minutes, 15);
        assert_eq!(cfg.search_engine, "DuckDuckGo");
        assert_eq!(cfg.theme.radius, 16);
        assert_eq!(cfg.splash.mode, SplashMode::On);
        assert_eq!(cfg.logo.preference, LogoPreference::Basic);
        assert_eq!(cfg.clock.orientation, ClockOrientation::Horizontal);
        assert_eq!(cfg.welcome.items.len(), 4);
        assert!(cfg.welcome.items.iter().any(|i| i.kind == ItemKind::Search));
        assert!(cfg.welcome.items.iter().any(|i| i.kind == ItemKind::Sites));
        assert!(!cfg.welcome.items.iter().any(|i| i.kind == ItemKind::Clock));
        assert!(!cfg.welcome.items.iter().any(|i| i.kind == ItemKind::Todo));
        assert!(!cfg.welcome.items.iter().any(|i| i.kind == ItemKind::Phrases));
        assert_eq!(cfg.welcome.sites.len(), 6);
        assert_eq!((cfg.welcome.site_rows, cfg.welcome.site_cols), (1, 8));
        assert!(cfg.collect_history);
        assert!(!cfg.incognito);
    }

    #[test]
    fn old_welcome_gets_sites_once() {
        let mut w: Welcome = serde_json::from_str(r#"{"items":[]}"#).unwrap();
        w.migrate();
        assert!(w.items.iter().any(|i| i.kind == ItemKind::Sites));
        w.items.retain(|i| i.kind != ItemKind::Sites);
        w.migrate();
        assert!(!w.items.iter().any(|i| i.kind == ItemKind::Sites));
    }

    #[test]
    fn incognito_is_never_persisted() {
        let mut cfg = Config::default();
        cfg.incognito = true;
        let back: Config = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert!(!back.incognito);
        assert!(Config::is_address("example.com"));
        assert!(!Config::is_address("rust borrow checker"));
    }

    #[test]
    fn welcome_settings_round_trip() {
        let json = r#"{
            "splash": {"mode": "Custom", "custom_splash": ["one", "two"]},
            "logo": {"preference": "Big"},
            "clock": {"font": "geist", "orientation": "Vertical", "seconds": true},
            "welcome": {"todo_style": "Nothing", "todos": [{"text": "x", "done": true}]}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.splash.mode, SplashMode::Custom);
        assert_eq!(cfg.splash.custom_splash, ["one", "two"]);
        assert_eq!(cfg.logo.preference, LogoPreference::Big);
        assert_eq!(cfg.clock.orientation, ClockOrientation::Vertical);
        assert!(cfg.clock.seconds);
        assert_eq!(cfg.welcome.todo_style, TodoStyle::Nothing);
        assert!(cfg.welcome.todos[0].done);
        let back: Config = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back.splash, cfg.splash);
        assert_eq!(back.welcome, cfg.welcome);
    }
}
