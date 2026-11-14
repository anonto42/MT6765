use crate::keyboard::keyboard_rect;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Rect};

pub const WM_CLASS_INSTANCE: &str = "walpad-mousepad";
pub const WM_CLASS_CLASS: &str = "WalPad-MousePad";
pub const STRUT_CLASS: &str = "walpad-mousepad-strut";
/// WM_CLASS of the cursor marker window.
pub const MARKER_CLASS: &str = "walpad-cursor";
/// Pointer gain: 10 px of finger travel move the cursor 15 px.
pub const ACCEL: f32 = 1.5;
/// Finger travel (px, either axis) before the cursor starts to move.
pub const TAP_SLOP_PX: i32 = SLOP_PX;
/// A touch that stayed within this travel (px, either axis) can still be a tap or a long press
/// (the panel's touch is noisy: the finger "moves" a few px on its own).
pub const TAP_TRAVEL_PX: i32 = 20;
/// A tap: released within this time (after the bridge's lift delay).
pub const TAP_MAX_MS: u32 = 300;
/// A touch held this long within the tap travel is a right click.
pub const LONG_PRESS_MS: u32 = 500;
/// One finger sample that jumps further than this (px, either axis) is noise.
pub const JUMP_PX: i32 = 80;
/// Low-pass: the share of the pending travel applied per sample.
pub const SMOOTH: f32 = 0.6;
/// The core's touch bridge releases the button 150 ms after the last touch report: subtracted
/// from the press duration.
pub const BRIDGE_LIFT_MS: u32 = 150;

pub(super) const PAD: i32 = SP_2;
/// Height cap of the mini-map (px).
const MAP_MAX_H: i32 = 200;

/// The pad window over other apps (no compositor, nothing to blend with): the strip is
/// `SURFACE_STRONG` over BG flattened into one opaque colour; the touch surface adds `SURFACE`.
pub fn strip_color() -> Color {
    surface_strong().over(bg())
}

/// The touch surface's (opaque) colour: what the mini-map is restored to.
pub fn surface_bg() -> Color {
    surface().over(strip_color())
}

/// X pointer buttons.
pub const BUTTON_LEFT: u8 = 1;
pub const BUTTON_RIGHT: u8 = 3;
pub const BUTTON_WHEEL_UP: u8 = 4;
pub const BUTTON_WHEEL_DOWN: u8 = 5;

/// The pad window: same place and size as the system keyboard.
pub fn pad_rect(area: (i32, i32)) -> Rect {
    keyboard_rect(area)
}

/// Where the virtual cursor may go: the visible area above the pad (a click below would land on
/// the pad itself).
pub fn cursor_bounds(area: (i32, i32)) -> Rect {
    Rect::new(0, 0, area.0, pad_rect(area).y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadButton {
    Left,
    Right,
    ScrollUp,
    ScrollDown,
    /// Minimise (hide) the pad.
    Close,
}

pub const PAD_BUTTONS: [PadButton; 5] = [
    PadButton::Left,
    PadButton::Right,
    PadButton::ScrollUp,
    PadButton::ScrollDown,
    PadButton::Close,
];

/// Pad-local geometry: the touch surface, the mini-map in its top-right corner and the button row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadLayout {
    pub surface: Rect,
    pub map: Rect,
    pub buttons: Vec<(Rect, PadButton)>,
}

pub fn pad_layout(area: (i32, i32)) -> PadLayout {
    let r = pad_rect(area);
    let by = r.h - PAD - BUTTON_H;
    let n = PAD_BUTTONS.len() as i32;
    let bw = (r.w - 2 * PAD - (n - 1) * PAD) / n;
    let buttons = PAD_BUTTONS
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let x = PAD + i as i32 * (bw + PAD);
            // the last button takes the rounding leftover
            let w = if i as i32 == n - 1 { r.w - PAD - x } else { bw };
            (Rect::new(x, by, w, BUTTON_H), *b)
        })
        .collect();
    let surface = Rect::new(PAD, PAD, r.w - 2 * PAD, by - 2 * PAD);
    // the mini-map is the visible cursor of last resort (the X cursor may not show on the panel)
    let mh = (surface.h - 2 * SP_3).min(MAP_MAX_H);
    let mw = mh * area.0 / area.1.max(1);
    let map = Rect::new(surface.right() - SP_3 - mw, surface.y + SP_3, mw, mh);
    PadLayout {
        surface,
        map,
        buttons,
    }
}

/// What a finished touch asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadOut {
    /// Press and release this X button at the virtual cursor.
    Click(u8),
    Hide,
}
