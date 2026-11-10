//! walpad-ui: tokens, canvas (tiny-skia), text (fontdue + glyph cache), vector icons, widgets
//! and the snapshot harness for WalPad Shell. Pure Rust, software rendered, no GPU, no image assets.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod art;
pub mod canvas;
pub mod components;
pub mod edit_bar;
pub mod icons;
pub mod layout;
pub mod snapshot;
pub mod text;
pub mod tokens;

pub use canvas::{Canvas, Rect};
pub use icons::Icon;
pub use text::{Align, Face, Fonts, Style};
pub use tokens::{Color, Tint};
