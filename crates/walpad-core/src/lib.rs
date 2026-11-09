//! walpad-core library: pure logic modules (no I/O, time passed in) plus the daemon.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod apps;
pub mod brain;
pub mod daemon;
pub mod fake;
pub mod inputs;
pub mod logic;
pub mod shadow;
pub mod shadow_live;
pub mod wpa;
pub mod xbridge;
