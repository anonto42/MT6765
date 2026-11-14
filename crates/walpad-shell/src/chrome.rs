//! The frame around every screen: the sidebar dock (left), the header (clock + quick-settings
//! pill) and the background layer. The background (charcoal + coffee watermark + the sidebar at
//! rest) is rendered once into a cached canvas and copied into each frame with one memcpy; it is
//! rebuilt only when its key changes (size, pinned apps, active destination, panel open).
use crate::app::Target;
use walpad_ui::art::{coffee_rect, draw_coffee};
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Canvas, Rect};

/// Index of each sidebar destination in `Tab::ALL` (`Target::Nav`).
mod header;
mod sidebar;
#[cfg(test)]
mod tests;

pub use header::render_header;
use sidebar::draw_item;
pub use sidebar::{
    active_target, pinned_items, sidebar_layout, SideItem, SidebarLayout, NAV_APPS, NAV_HOME,
    NAV_SETTINGS,
};

/// What the cached background depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeKey {
    pub w: i32,
    pub h: i32,
    pub sidebar: SidebarLayout,
    pub active: Option<Target>,
    /// Full-screen page: the page surface is part of the cached layer.
    pub page: bool,
    pub theme: ThemeMode,
    pub wallpaper: walpad_ui::art::WallpaperKind,
    pub wallpaper_path: String,
}

/// The wallpaper (bottom-aligned: the shell shrinks above the dock strip), the sidebar edge and its
/// buttons at rest.  Without a wallpaper (decode failed) plain charcoal with the line-art cup.
pub fn render_background(p: &mut Painter, frame: &Frame, key: &ChromeKey, wall: Option<&Canvas>) {
    render_base(p, frame, key, wall, 0);
    render_items(p, key);
}

/// Everything of the background except the sidebar buttons.
fn render_base(
    p: &mut Painter,
    frame: &Frame,
    key: &ChromeKey,
    wall: Option<&Canvas>,
    phase_ms: u64,
) {
    if key.wallpaper.is_live() && !key.page {
        walpad_ui::art::draw_live_wallpaper(p.cv, wall, key.wallpaper, phase_ms, key.theme);
    } else {
        p.cv.fill(bg());
        match wall {
            Some(w) => p.cv.blit(w, 0, frame.h - w.height()),
            None => draw_coffee(p.cv, coffee_rect(frame.content), 3.0, watermark()),
        }
    }
    if key.page {
        p.page_surface(frame.content);
    }
    let sb = frame.sidebar;
    p.cv.rect(Rect::new(sb.right() - 1, sb.y, 1, sb.h), divider());
    p.divider(sb.x + (sb.w - 40) / 2, key.sidebar.divider_y, 40);
}

fn render_items(p: &mut Painter, key: &ChromeKey) {
    for it in &key.sidebar.items {
        draw_item(p, it, false, key.active == Some(it.target));
    }
}

struct Cached {
    key: ChromeKey,
    cv: Canvas,
    /// The sidebar strip before its buttons were drawn (x 0, y 0, sidebar width): a pressed
    /// button is drawn over this, never over the resting one (translucent fills would stack) and
    /// never over a square of plain colour (the round toggle would show a box).
    bare_sidebar: Option<Canvas>,
}

struct WallCache {
    landscape: bool,
    theme: ThemeMode,
    kind: walpad_ui::art::WallpaperKind,
    path: String,
    cv: Option<Canvas>,
}

/// The cached background layer.
#[derive(Default)]
pub struct Chrome {
    cache: Option<Cached>,
    /// Decoded static wallpaper base (for Default / LiveCoffee / Custom).
    wall: Option<WallCache>,
    /// Times the layer was rebuilt (tests check it is not per frame).
    pub builds: u64,
}

impl Chrome {
    /// Copy the background for `key` into `p.cv`, rebuilding the cache when the key changed.
    pub fn paint(&mut self, p: &mut Painter, frame: &Frame, key: ChromeKey, phase_ms: u64) {
        let stale = self.cache.as_ref().is_none_or(|c| c.key != key);
        let landscape = frame.w > frame.h;
        if self.wall.as_ref().is_none_or(|w| {
            w.landscape != landscape
                || w.theme != key.theme
                || w.kind != key.wallpaper
                || w.path != key.wallpaper_path
        }) {
            use walpad_ui::art::{custom_wallpaper, wallpaper_for_theme, WallpaperKind};
            let cv = match key.wallpaper {
                WallpaperKind::Default | WallpaperKind::LiveCoffee => {
                    wallpaper_for_theme(frame.w, frame.h, key.theme)
                }
                WallpaperKind::Custom => custom_wallpaper(&key.wallpaper_path, frame.w, frame.h)
                    .or_else(|| wallpaper_for_theme(frame.w, frame.h, key.theme)),
                WallpaperKind::LiveAurora | WallpaperKind::LiveStars => None,
            };
            self.wall = Some(WallCache {
                landscape,
                theme: key.theme,
                kind: key.wallpaper,
                path: key.wallpaper_path.clone(),
                cv,
            });
        }
        let wall = self.wall.as_ref().and_then(|w| w.cv.as_ref());
        if key.wallpaper.is_live() && !key.page {
            // Live wallpaper animates every tick on Home: draw straight into `p.cv` without
            // churning the static `self.cache`.
            render_base(p, frame, &key, wall, phase_ms);
            render_items(p, &key);
            self.cache = None;
            return;
        }
        if stale {
            match Canvas::new(frame.w as u32, frame.h as u32, bg()) {
                Some(mut cv) => {
                    let mut bp = Painter {
                        cv: &mut cv,
                        fonts: &mut *p.fonts,
                    };
                    render_base(&mut bp, frame, &key, wall, 0);
                    let sb = frame.sidebar;
                    let bare_sidebar = bp.cv.crop(Rect::new(0, 0, sb.right(), frame.h));
                    render_items(&mut bp, &key);
                    self.builds += 1;
                    self.cache = Some(Cached {
                        key: key.clone(),
                        cv,
                        bare_sidebar,
                    });
                }
                None => self.cache = None,
            }
        }
        match &self.cache {
            Some(c) if p.cv.copy_from(&c.cv) => {}
            // no cache (allocation failed) or a canvas of another size: draw it directly
            _ => render_background(p, frame, &key, wall),
        }
    }

    /// Restore `r` from the cached background (the header and banner strip after scrolled
    /// content was drawn under them). Falls back to a plain fill.
    pub fn restore(&self, cv: &mut Canvas, r: Rect) {
        match &self.cache {
            Some(c) if cv.copy_rect_from(&c.cv, r) => {}
            _ => cv.rect(r, bg()),
        }
    }

    /// Draw the pressed sidebar button (the only per-frame sidebar work): its rect is first put
    /// back to the bare background (wallpaper, no button), then the button is drawn pressed. The
    /// round toggle's pressed state is a circle; its bounding-box corners keep the background.
    pub fn render_pressed(&self, p: &mut Painter, layout: &SidebarLayout, pressed: Option<Target>) {
        let Some(it) = layout.items.iter().find(|it| Some(it.target) == pressed) else {
            return;
        };
        // without a cache the frame was drawn directly and the resting button stays under the
        // pressed one (only the tint and the white outline are added: still round)
        if let Some(bare) = self.cache.as_ref().and_then(|c| c.bare_sidebar.as_ref()) {
            p.cv.copy_rect_from_at(bare, (0, 0), it.rect);
        }
        draw_item(p, it, true, false);
    }
}
