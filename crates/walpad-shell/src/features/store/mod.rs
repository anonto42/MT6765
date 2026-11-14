//! App store screen (change app-catalogue-manager), shown as the Settings > Apps page and opened
//! straight to the catalogue from the Apps panel's Tools tile. Model, layout and drawing are pure
//! (content coordinates, like settings_screen.rs); the state changes live in `App`.
mod layout;
mod model;
mod render;
#[cfg(test)]
mod tests;

pub use layout::{layout, Layout};
pub use model::{
    fmt_size, role_label, Busy, StoreEntry, StoreModel, StoreTarget, StoreView, DEFAULT_RESERVE_KB,
};
pub use render::render;
