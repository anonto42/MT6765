//! walpad-btbridge: joins the MediaTek CONNSYS Bluetooth char device (`/dev/stpbt`, H:4 byte
//! stream, opened = BT function powered on) to the kernel's virtual HCI driver (`/dev/vhci`, one
//! packet per read/write), so BlueZ sees an ordinary `hci0`.
//!
//! Logging never contains packet payloads: only types, lengths and counters.

pub mod h4;
pub mod init;

mod bridge;
mod io;
#[cfg(test)]
mod tests;

pub use bridge::{Bridge, Step};
pub use io::{
    controller_command, create_hci, read_some, wait_readable, write_all_timeout, Stats, READ_BUF,
};
