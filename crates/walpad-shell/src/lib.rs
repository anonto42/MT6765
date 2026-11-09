//! walpad-shell library: gesture recogniser, X11 presenter/window, application state and the
//! event loop. The binary is a thin wrapper (src/main.rs).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod apps_screen;
pub mod config;
pub mod core_client;
pub mod device_screen;
pub mod gesture;
pub mod home;
pub mod network;
pub mod overlay;
pub mod present;
pub mod settings_screen;
pub mod shell;
pub mod window;
