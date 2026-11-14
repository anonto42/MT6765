//! Touch gestures (owner request 2026-11-10: "think like a mobile device"). Sits behind
//! `touch_map` (finger -> pointer actions) and turns a finger into what a phone app expects:
//!
//! - one-finger tap = left click at the touch point (no added delay: the click goes out when the
//!   touch ends; the bridge's 150 ms lift delay is the only wait);
//! - one-finger drag = scroll: wheel buttons 4/5 (vertical) and 6/7 (horizontal), one click per
//!   `px_per_click` of finger travel, then a fling that decays over `fling_ms`;
//! - hold 500 ms without moving, then lift = a double click (word select), like Android's own
//!   long-press-to-select (owner report 2026-11-13: "on android if i want to copy any think i
//!   have to pres a text ... after that we abale get all thows copy, paste, cut"; traded away the
//!   right-click context menu this used to open, by the owner's own choice);
//! - hold 500 ms, then drag = a mouse drag (button 1 pressed at the touch point): text selection;
//! - taps close together in time and place = double (triple) click: the second click is snapped
//!   to the first one's position, the X toolkits do the counting;
//! - two fingers (only when the panel reports two contacts) = pinch, as Ctrl + wheel.
//!
//! A touch is classified once, when it starts, by `Mode`: `Mouse` passes the pointer actions
//! through unchanged (the shell's own windows, apps whose drag must stay a drag: apps.json
//! `"touch": "mouse"`), `Scroll` uses the recogniser. The caller answers the question "what is
//! under the finger" through the `probe` closure (an X query), once per touch.

mod recogniser;
mod scroll_fling;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_extra;

pub use recogniser::Gestures;

use super::Ms;
use serde::Deserialize;

pub const BTN_LEFT: u8 = 1;
pub const BTN_RIGHT: u8 = 3;
pub const WHEEL_UP: u8 = 4;
pub const WHEEL_DOWN: u8 = 5;
pub const WHEEL_LEFT: u8 = 6;
pub const WHEEL_RIGHT: u8 = 7;

/// What a touch does, decided when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Finger = mouse (press at once, move, release at the lift).
    Mouse,
    /// Finger = touchscreen: tap, scroll, long press.
    #[default]
    Scroll,
}

impl Mode {
    /// apps.json `touch` value (case-insensitive); anything else is None (= the default).
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mouse" => Some(Mode::Mouse),
            "scroll" => Some(Mode::Scroll),
            _ => None,
        }
    }
}

/// What the recogniser asks the X bridge to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    Move(i32, i32),
    /// Button n pressed (true) or released (false).
    Button(u8, bool),
    /// One wheel click (button 4-7 press + release), with Control held when `ctrl`.
    Wheel {
        button: u8,
        ctrl: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    /// false = every touch is a mouse touch (the behaviour before gestures).
    pub enabled: bool,
    /// Finger travel before a touch stops being a tap/long press and becomes a scroll.
    pub slop_px: i32,
    pub long_press_ms: Ms,
    /// A long press needs less travel than this.
    pub long_press_px: i32,
    /// A tap this soon after a click (ms) and this near it (px) lands on the same spot.
    pub double_tap_ms: Ms,
    pub double_tap_px: i32,
    /// Finger travel per wheel click.
    pub px_per_click: i32,
    /// Fling: minimum / maximum release speed (px per ms), duration and time constant (ms).
    pub fling_min: f32,
    pub fling_max: f32,
    pub fling_ms: Ms,
    pub fling_tau_ms: f32,
    /// Wheel clicks per call at most (the rest waits for the next one).
    pub max_clicks: u32,
    /// Change of the distance between two fingers per zoom click.
    pub pinch_px_per_click: i32,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            enabled: true,
            slop_px: 12,
            long_press_ms: 500,
            long_press_px: 10,
            double_tap_ms: 450,
            double_tap_px: 40,
            px_per_click: 48,
            fling_min: 0.25,
            fling_max: 3.0,
            fling_ms: 400,
            fling_tau_ms: 150.0,
            max_clicks: 8,
            pinch_px_per_click: 40,
        }
    }
}
