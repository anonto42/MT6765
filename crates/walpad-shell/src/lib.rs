//! walpad-shell library: gesture recogniser, X11 presenter/window, application state and the
//! event loop. The binary is a thin wrapper (src/main.rs).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod chrome;
pub mod config;
pub mod core_client;
pub mod device_screen;
pub mod dock;
pub mod gesture;
pub mod home;
pub mod ime;
pub mod keyboard;
pub mod mousepad;
pub mod network;
pub mod overlay;
pub mod panels;
pub mod present;
pub mod settings_screen;
pub mod shell;
pub mod window;
