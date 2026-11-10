//! walpad-shell library: gesture recogniser, X11 presenter/window, application state and the
//! event loop. The binary is a thin wrapper (src/main.rs).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod bluetooth;
pub mod chrome;
pub mod clipboard;
pub mod config;
pub mod core_client;
pub mod cursor;
pub mod device_screen;
pub mod dock;
pub mod edge;
pub mod gesture;
pub mod home;
pub mod ime;
pub mod keyboard;
pub mod lock_screen;
pub mod lock_window;
pub mod mobile;
pub mod mousepad;
pub mod nav;
pub mod network;
pub mod overlay;
pub mod panels;
pub mod present;
pub mod seltools;
pub mod recents;
pub mod settings_screen;
pub mod shell;
pub mod volume_ui;
pub mod store;
pub mod vpn_screen;
pub mod window;
