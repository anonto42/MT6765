//! walpad-core library: Hexagonal / Feature-Sliced architecture
//! - `domain`: pure deterministic state machine and business logic (no I/O)
//! - `features`: vertical capability slices (connectivity, telephony, location, device)
//! - `adapters`: outbound platform/IPC bridges (shadow, xbridge)
//! - `app`: daemon composition root & event loop
//! - `shared`: shared test/dev helpers
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod adapters;
pub mod app;
pub mod domain;
pub mod features;
pub mod shared;

// Crate-root module re-exports preserving public & test paths.
pub use adapters::{shadow, shadow_live, xbridge};
pub use app::daemon;
pub use domain::{brain, logic};
pub use features::connectivity::{bluetooth, hotspot, vpn, wpa};
pub use features::device::{apps, inputs, jack, layout, lock, notifications, pkg, stats, volume};
pub use features::location::gnss;
pub use features::telephony::{modem, phone};
pub use shared::fake;
