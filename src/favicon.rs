//! Favicons for the sites widget: disk cache first, then a quiet fetch on a
//! background thread. Incognito passes `allow_network = false` and only ever
//! reads what is already cached.

use eframe::egui::{self, ColorImage};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

pub struct Favicons {
    map: HashMap<String, Option<egui::TextureHandle>>,
    pending: HashSet<String>,
    tx: Sender<(String, Option<Vec<u8>>)>,
    rx: Receiver<(String, Option<Vec<u8>>)>,
}

impl Default for Favicons {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            map: HashMap::new(),
            pending: HashSet::new(),
            tx,
            rx,
        }
    }
}

impl Favicons {
    /// Picks up finished fetches and turns them into textures.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((host, bytes)) = self.rx.try_recv() {
            self.pending.remove(&host);
            let texture = bytes.and_then(|b| decode(&b, &host, ctx));
            self.map.insert(host, texture);
        }
    }

    /// Texture for a site, starting a fetch once. None means "not yet".
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        url: &str,
        allow_network: bool,
    ) -> Option<egui::TextureId> {
        let host = host(url)?;
        if let Some(entry) = self.map.get(&host) {
            return entry.as_ref().map(|t| t.id());
        }
        if self.pending.contains(&host) {
            ctx.request_repaint_after(Duration::from_millis(150));
            return None;
        }

        if let Some(bytes) = read_cache(&host) {
            match decode(&bytes, &host, ctx) {
                Some(texture) => {
                    let id = texture.id();
                    self.map.insert(host, Some(texture));
                    return Some(id);
                }
                // A broken cache entry would otherwise be retried every frame.
                None => {
                    let _ = std::fs::remove_file(cache_path(&host));
                }
            }
        }

        if allow_network {
            let tx = self.tx.clone();
            let name = host.clone();
            std::thread::spawn(move || {
                let bytes = fetch(&name);
                if let Some(b) = &bytes {
                    write_cache(&name, b);
                }
                let _ = tx.send((name, bytes));
            });
            self.pending.insert(host);
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        None
    }
}

fn host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest
        .split(['/', '?', '#'])
        .next()?
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .trim()
        .to_ascii_lowercase();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn cache_dir() -> PathBuf {
    crate::config::Config::path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("favicons")
}

fn cache_path(host: &str) -> PathBuf {
    let safe: String = host
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache_dir().join(format!("{safe}.img"))
}

fn read_cache(host: &str) -> Option<Vec<u8>> {
    std::fs::read(cache_path(host)).ok()
}

fn write_cache(host: &str, bytes: &[u8]) {
    let path = cache_path(host);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, bytes);
}

fn fetch(host: &str) -> Option<Vec<u8>> {
    let http = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent();
    // DuckDuckGo's icon service is the most reliable; Google's is the fallback.
    for url in [
        format!("https://icons.duckduckgo.com/ip3/{host}.ico"),
        format!("https://www.google.com/s2/favicons?domain={host}&sz=64"),
    ] {
        if let Ok(mut resp) = http
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (Scrape)")
            .call()
        {
            if let Ok(bytes) = resp.body_mut().read_to_vec() {
                if !bytes.is_empty() {
                    return Some(bytes);
                }
            }
        }
    }
    None
}

fn decode(bytes: &[u8], host: &str, ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            eprintln!("scrape: favicon {host}: {e}");
            return None;
        }
    };
    let (w, h) = img.dimensions();
    let img = if w > 128 || h > 128 {
        image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let (w, h) = img.dimensions();
    Some(ctx.load_texture(
        format!("favicon-{host}"),
        ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}
