//! walpad-ui: Hexagonal UI toolkit for WalPad Shell (pure Rust, software rendered).
//! - `domain`: pure design tokens, layout geometry, vector icons, and generative art
//! - `adapters`: tiny-skia canvas, fontdue text renderer, widget painters, and snapshot harness
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod adapters;
pub mod domain;

pub use adapters::{canvas, components, edit_bar, snapshot, text};
pub use domain::{art, icons, layout, tokens};

pub use canvas::{Canvas, Rect};
pub use icons::Icon;
pub use text::{Align, Face, Fonts, Style};
pub use tokens::{Color, Tint};
