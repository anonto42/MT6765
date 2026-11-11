//! walpad-core library: pure logic modules (no I/O, time passed in) plus the daemon.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod apps;
pub mod bluetooth;
pub mod brain;
pub mod daemon;
pub mod fake;
pub mod gnss;
pub mod hotspot;
pub mod inputs;
pub mod lock;
pub mod logic;
pub mod modem;
pub mod phone;
pub mod pkg;
pub mod shadow;
pub mod shadow_live;
pub mod volume;
pub mod vpn;
pub mod wpa;
pub mod xbridge;
