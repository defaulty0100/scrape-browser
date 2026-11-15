//! The page view: one WebView2 child window, positioned by egui each frame.

use eframe::egui;
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, Win32WindowHandle};
use std::num::NonZeroIsize;
use std::sync::{Arc, Mutex, OnceLock};
use wry::{Rect, WebView, WebViewBuilder, WebViewBuilderExtWindows};

struct Host(isize);

impl HasWindowHandle for Host {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

pub struct Page {
    wv: WebView,
    bounds: Option<Rect>,
    visible: bool,
}

/// Extra Chromium switches. wry replaces the environment arguments with its own,
/// so the env var `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` never reaches the browser.
static EXTRA_ARGS: OnceLock<String> = OnceLock::new();

pub fn set_browser_args(args: String) {
    let _ = EXTRA_ARGS.set(args);
}

fn browser_args() -> String {
    // wry's own defaults, which `with_additional_browser_args` would otherwise drop.
    let mut args = String::from(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
         --autoplay-policy=no-user-gesture-required",
    );
    if let Some(extra) = EXTRA_ARGS.get() {
        args.push(' ');
        args.push_str(extra);
    }
    args
}

impl Page {
    pub fn new(
        parent: isize,
        url: &str,
        bounds: Rect,
        title: Arc<Mutex<String>>,
        ctx: &egui::Context,
    ) -> Result<Self, String> {
        let ctx = ctx.clone();
        let wv = WebViewBuilder::new()
            .with_additional_browser_args(browser_args())
            .with_url(url)
            .with_bounds(bounds)
            .with_visible(true)
            .with_document_title_changed_handler(move |t| {
                if let Ok(mut slot) = title.lock() {
                    *slot = t;
                }
                ctx.request_repaint();
            })
            .build_as_child(&Host(parent))
            .map_err(|e| format!("WebView2: {e}"))?;

        Ok(Self {
            wv,
            bounds: Some(bounds),
            visible: true,
        })
    }

    pub fn navigate(&self, url: &str) {
        let _ = self.wv.load_url(url);
    }

    pub fn url(&self) -> Option<String> {
        self.wv.url().ok()
    }

    pub fn back(&self) {
        let _ = self.wv.go_back();
    }

    pub fn forward(&self) {
        let _ = self.wv.go_forward();
    }

    /// Replaces the current page with a fresh load of the same url.
    pub fn reload(&self) {
        let _ = self.wv.reload();
    }

    /// Runs JS in the page; the callback receives the JSON-encoded result.
    pub fn eval(&self, js: &str, callback: impl Fn(String) + Send + 'static) {
        let _ = self.wv.evaluate_script_with_callback(js, callback);
    }

    pub fn set_bounds(&mut self, bounds: Rect) {
        if self.bounds == Some(bounds) {
            return;
        }
        let _ = self.wv.set_bounds(bounds);
        self.bounds = Some(bounds);
    }

    pub fn set_visible(&mut self, visible: bool) {
        if self.visible == visible {
            return;
        }
        let _ = self.wv.set_visible(visible);
        self.visible = visible;
    }
}

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    use wry::dpi::{PhysicalPosition, PhysicalSize, Position, Size};
    Rect {
        position: Position::Physical(PhysicalPosition::new(x as i32, y as i32)),
        size: Size::Physical(PhysicalSize::new(w.max(0.0) as u32, h.max(0.0) as u32)),
    }
}
