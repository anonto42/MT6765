//! walpad-core library: pure logic modules (no I/O, time passed in) plus the daemon.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod daemon;
pub mod fake;
pub mod logic;
