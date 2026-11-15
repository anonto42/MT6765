//! walpad-btbridge: Hexagonal architecture joining `/dev/stpbt` (H:4) to `/dev/vhci`.
//! - `domain`: pure H:4 packet framing and MTK controller init sequence
//! - `adapters`: VHCI and char-device fd I/O
//! - `app`: bridge event loop
//!
//! Logging never contains packet payloads: only types, lengths and counters.

pub mod adapters;
pub mod app;
pub mod domain;

pub use adapters::io;
pub use app::bridge;
pub use domain::{h4, init};

#[cfg(test)]
mod tests;

pub use bridge::{Bridge, Step};
pub use io::{
    controller_command, create_hci, read_some, wait_readable, write_all_timeout, Stats, READ_BUF,
};
