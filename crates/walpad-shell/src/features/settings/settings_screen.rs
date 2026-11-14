//! Settings tab: category list and subpages. Pages are built as plain rows from real data
//! (`SysInfo` + status + config) so they can be tested without drawing.

mod pages;
mod pages_power_net;
mod render;
mod sysinfo;

#[cfg(test)]
mod tests;

pub use pages::page_rows;
pub use render::{layout, render, row_h, SettingsLayout};
pub use sysinfo::{fmt_bytes, fmt_uptime, gather, telnet_scope, SysInfo};

use crate::app::Target;
use crate::config::DevdSettings;
use walpad_proto::Status;
use walpad_ui::components::Tone;
use walpad_ui::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Display,
    /// Volume, mute and the test tone (owner request 2026-11-10).
    Sound,
    Input,
    Power,
    Network,
    /// Opens the Mobile network screen (owner request 2026-11-10).
    Mobile,
    /// Opens the VPN and DNS screen (owner request 2026-11-10).
    Vpn,
    /// Opens the Bluetooth screen (owner request 2026-11-10).
    Bluetooth,
    /// Opens the GPS screen (owner request 2026-11-11).
    Gps,
    /// Opens the app store's installed apps (change app-catalogue-manager).
    Apps,
    Security,
    System,
    Clock,
    /// Which live figures the header shows (owner request 2026-11-11).
    TopBar,
    Maintenance,
    About,
    Logs,
}

pub const CATEGORIES: [(Page, &str, Icon); 16] = [
    (Page::Display, "Display", Icon::Sun),
    (Page::Sound, "Sound", Icon::Volume),
    (Page::Input, "Input", Icon::Keyboard),
    (Page::Power, "Power", Icon::Power),
    (Page::Network, "Network", Icon::Wifi),
    (Page::Mobile, "Mobile network", Icon::Sim),
    (Page::Vpn, "VPN and DNS", Icon::Lock),
    (Page::Bluetooth, "Bluetooth", Icon::Bluetooth),
    (Page::Gps, "GPS", Icon::Location),
    (Page::Apps, "Apps", Icon::Apps),
    (Page::Security, "Security", Icon::Lock),
    (Page::System, "System", Icon::Device),
    (Page::Clock, "Clock", Icon::Settings),
    (Page::TopBar, "Top bar items", Icon::Device),
    (Page::Maintenance, "Maintenance", Icon::Refresh),
    (Page::About, "About", Icon::Globe),
];

pub fn title(p: Page) -> &'static str {
    match p {
        Page::Logs => "Logs",
        other => CATEGORIES
            .iter()
            .find(|(c, _, _)| *c == other)
            .map_or("", |(_, n, _)| n),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Info(String, String, Tone),
    /// Tappable setting row with a label on the left and its current value on the right.
    Choice(String, String, Target),
    Toggle(&'static str, bool, Target),
    Action {
        label: &'static str,
        target: Target,
        danger: bool,
    },
    Link(&'static str, Page),
    Mono(String),
    /// Label line (icon, name, value text) above a 56 px slider; the slider is the hit area.
    Slider {
        icon: Icon,
        label: &'static str,
        pct: String,
        value: f32,
        active: bool,
        target: Target,
        disabled: bool,
    },
    /// An action button whose label is computed (the screen lock's auto-lock delay).
    ActionText {
        label: String,
        target: Target,
        danger: bool,
    },
}

pub struct Ctx<'a> {
    pub sys: &'a SysInfo,
    pub status: &'a Option<Status>,
    pub devd: &'a DevdSettings,
    pub clock_24h: bool,
}
