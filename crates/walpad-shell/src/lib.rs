//! walpad-shell library: Feature-Sliced Design (FSD)
//! - `app`: shell composition root, event loop, navigation, and global application state
//! - `features`: vertical feature slices (UI screens, windows, models, workers, tests)
//! - `shared`: cross-feature reusable infrastructure (IPC client, X11, input, UI chrome, config)
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod features;
pub mod shared;

// Crate-root module re-exports preserving public, internal, and test paths.
pub use app::{nav, shell};
pub use features::call::{call_screen, call_window};
pub use features::device::device_screen;
pub use features::gps::gps_screen;
pub use features::lock::{lock_screen, lock_window};
pub use features::media::worker as media_worker;
pub use features::notifications::notifications_ui;
pub use features::phone::phone_ui;
pub use features::settings::settings_screen;
pub use features::volume::volume_ui;
pub use features::vpn::vpn_screen;
pub use features::windows::windows_ui;
pub use features::{
    bluetooth, clipboard, dock, home, keyboard, media, mobile, mousepad, network, panels, recents,
    seltools, store, wmbar,
};
pub use shared::config;
pub use shared::input::{edge, gesture};
pub use shared::ipc::core_client;
pub use shared::ui::{chrome, overlay, topbar};
pub use shared::x11::{cursor, ime, present, window};
