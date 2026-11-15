//! walpad-gnss: Hexagonal architecture for the GNSS HAL stand-in.
//! - `domain`: pure NMEA parser, fix/receiver state, and wire DTOs
//! - `adapters`: datagram socket, pty, and `/dev/stpgps` I/O
//! - `app`: daemon poll loop

pub mod adapters;
pub mod app;
pub mod domain;

pub use adapters::io;
pub use app::daemon;
pub use domain::{nmea, state, wire};
