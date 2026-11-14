//! The background watermark: a line-art iced coffee (the owner's mock-up, SVG viewBox 200x300),
//! stroked at a constant screen width. Drawing it costs a few dozen path segments, but the shell
//! renders it once into a cached layer and never per frame.

mod coffee;
#[cfg(test)]
mod tests;
mod wallpaper;

pub use coffee::{coffee_rect, draw_coffee, VIEW_H, VIEW_W};
pub use wallpaper::{
    custom_wallpaper, draw_live_wallpaper, wallpaper, wallpaper_for_theme, WallpaperKind,
    WALLPAPER_LANDSCAPE, WALLPAPER_PORTRAIT,
};
