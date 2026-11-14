//! Components (docs/rust/UI.md section "Components"). Each draws one widget in one explicit state
//! and returns hit rectangles where the caller needs them. No animation state lives here: the shell
//! passes the current state (and spinner angle) every frame.
//!
//! Line-art theme: controls are outlines on the charcoal background. Pressed = 10 % white fill and
//! a full-white outline (there is no hover on a touch panel). Colours are opaque pre-blends
//! (tokens.rs), so nothing here blends a translucent layer over the frame.
use crate::canvas::{Canvas, Rect};
use crate::icons::Icon;
use crate::text::Fonts;
use crate::tokens::*;

mod controls;
mod feedback;
mod keyboard;
mod lists;
mod overlays;
mod primitives;
mod status;
mod sys_keyboard;
#[cfg(test)]
mod tests;

pub use keyboard::{keyboard_layout, Key, KeyBtn, KeyboardLayer};
pub use status::{BOLT_W, BT_MARK_W, CELL_BARS_W, CHARGING_MARK, GPS_MARK_W};
pub use sys_keyboard::{
    sys_key_char, system_keyboard_layout, SysKbdState, SysKey, SysKeyBtn, SYS_ROWS,
};

pub struct Painter<'a> {
    pub cv: &'a mut Canvas,
    pub fonts: &'a mut Fonts,
}

// ------------------------------------------------------------------ small enums
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Danger,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Default,
    Pressed,
    Disabled,
    /// Spinner angle in degrees.
    Loading(u16),
}
#[derive(Debug, Clone, PartialEq)]
pub enum CardValue {
    Value(String),
    /// Reading is a voltage estimate: shown with `~` and a warning caption.
    Estimated(String),
    Unknown,
    Error(String),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Accessory {
    None,
    Chevron,
    Toggle(bool),
    Value(String),
    Action(String),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetState {
    Idle,
    Connected,
    /// Spinner angle in degrees.
    Connecting(u16),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warning,
    Danger,
}
/// The Wi-Fi mark in the header pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiMark {
    Connected,
    Disconnected,
    Failed,
    /// No status from the core.
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Home,
    Apps,
    Network,
    Device,
    Settings,
    /// Mobile network / SIM (owner request 2026-11-10; reached from Quick Settings and Settings).
    Mobile,
    /// Bluetooth devices (owner request 2026-11-10; reached from Quick Settings and Settings).
    Bluetooth,
    /// VPN, DNS and proxy (owner request 2026-11-10; reached from Settings).
    Vpn,
    /// Media browser (owner request 2026-11-11; reached from the Apps panel and Home).
    Media,
    /// Phone: dialer, recent calls, contacts, messages (owner request 2026-11-11).
    Phone,
    /// GPS switch, position and satellites (owner request 2026-11-11; Quick Settings, Settings).
    Gps,
    /// Notification centre (owner request 2026-11-12, change `device-notifications`; reached from
    /// the bell icon in the header, not a sidebar destination).
    Notifications,
}
impl Tab {
    pub const ALL: [Tab; 12] = [
        Tab::Home,
        Tab::Apps,
        Tab::Network,
        Tab::Device,
        Tab::Settings,
        Tab::Mobile,
        Tab::Bluetooth,
        Tab::Vpn,
        Tab::Media,
        Tab::Phone,
        Tab::Gps,
        Tab::Notifications,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Apps => "Apps",
            Tab::Network => "Network",
            Tab::Device => "Device",
            Tab::Settings => "Settings",
            Tab::Mobile => "Mobile network",
            Tab::Bluetooth => "Bluetooth",
            Tab::Vpn => "VPN and DNS",
            Tab::Media => "Media",
            Tab::Phone => "Phone",
            Tab::Gps => "GPS",
            Tab::Notifications => "Notifications",
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Tab::Home => Icon::Home,
            Tab::Apps => Icon::Apps,
            Tab::Network => Icon::Wifi,
            Tab::Device => Icon::Device,
            Tab::Settings => Icon::Settings,
            Tab::Mobile => Icon::Sim,
            Tab::Bluetooth => Icon::Bluetooth,
            Tab::Vpn => Icon::Lock,
            Tab::Media => Icon::Media,
            Tab::Phone => Icon::Phone,
            Tab::Gps => Icon::Location,
            Tab::Notifications => Icon::Bell,
        }
    }
}

pub fn tone_color(t: Tone) -> Color {
    match t {
        Tone::Info => text(),
        Tone::Warning => warning(),
        Tone::Danger => danger(),
    }
}

/// Tone wash of a banner (14 %), blended over its `SURFACE_STRONG` fill.
pub fn banner_tint(t: Tone) -> Tint {
    Tint::new(
        match t {
            Tone::Info => text(),
            other => tone_color(other),
        },
        14,
    )
}

/// Danger wash of a danger button (rest 16 %, pressed 30 %), over its surface fill.
pub(super) fn danger_tint() -> Tint {
    Tint::new(danger(), 16)
}
pub(super) fn danger_tint_pressed() -> Tint {
    Tint::new(danger(), 30)
}

/// Bounding rects of the runs of vertically adjacent list rows (`(row, in a group)`; a row that
/// is not grouped, or a gap, ends the run): what `Painter::row_group` backs.
pub fn row_runs(rows: impl IntoIterator<Item = (Rect, bool)>) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    let mut open = false;
    for (r, grouped) in rows {
        if !grouped {
            open = false;
            continue;
        }
        match out.last_mut() {
            Some(g) if open && g.bottom() == r.y && g.x == r.x && g.w == r.w => g.h += r.h,
            _ => out.push(r),
        }
        open = true;
    }
    out
}

/// Icon size for a box of side `box_px` (24 px grid icons, scaled up in the bigger boxes).
pub fn icon_for_box(box_px: i32) -> i32 {
    match box_px {
        b if b >= 64 => 28,
        b if b >= 56 => 24,
        _ => 22,
    }
}

/// The page surface rect for a content area (frame.content): inset by `PAGE_MARGIN` at the sides
/// and the bottom, flush with the header at the top.
pub fn page_rect(content: Rect) -> Rect {
    Rect::new(
        content.x + PAGE_MARGIN,
        content.y,
        content.w - 2 * PAGE_MARGIN,
        content.h - PAGE_MARGIN,
    )
}
