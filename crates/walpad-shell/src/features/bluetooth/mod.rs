//! Bluetooth screen (owner request 2026-11-10): switch, scan, My devices (paired: connect /
//! disconnect / forget), Available devices (tap to pair) and the pairing dialog. Model helpers and
//! layout are pure; content coordinates as in network.rs (y starts at 0 at the top of the
//! scrollable content).
mod model;
mod pair;
mod render;
#[cfg(test)]
mod tests;

pub use model::{
    available_devices, bars_from_rssi, chip, device_icon, layout, my_devices, powered, qs_line,
    scanning, supported, switch_on, switchable, BtLayout, BtRow, PairState, View,
};
pub use pair::{render_forget, render_pair};
pub use render::render;
