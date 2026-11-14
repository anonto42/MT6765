//! The lock screen's content (owner request 2026-11-10, OpenSpec `lock-screen`): clock, prompt, a
//! PIN pad, a password field with the shell's on-screen keyboard, or a 3x3 pattern grid. Pure of
//! I/O: geometry, state machine and drawing; `lock_window.rs` owns the X window and the grabs, the
//! shell loop sends the secret to the core (which alone verifies it).
//!
//! The typed secret lives only in `entry` / `pattern` until it is handed over with
//! `Out::Submit` and is wiped at once; it is never printed (no `Debug` on this type).

mod input;
mod layout;
mod render;

#[cfg(test)]
mod tests;

pub use layout::{layout, Layout};

use crate::app::WallTime;
use walpad_proto::LockKind;
use walpad_ui::components::KeyboardLayer;
use walpad_ui::Rect;

/// Shortest PIN the core accepts; the pad refuses to submit less.
pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 12;
pub const PATTERN_MIN: usize = 4;
pub(crate) const PW_MAX: usize = 64;
pub(crate) const KEY_W: i32 = 128;
pub(crate) const KEY_H: i32 = 96;
pub(crate) const KEY_GAP: i32 = 16;
pub(crate) const KB_H: i32 = 274;
/// Pointer distance (px) to a pattern dot that captures it.
pub(crate) const DOT_CAPTURE: i32 = 52;

/// What the finger is on (for the pressed look and for hit testing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Digit(char),
    Backspace,
    Submit,
    /// Index into `keyboard_layout` of the current layer.
    Key(usize),
    Eye,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    None,
    Redraw,
    /// Hand this secret to the core (`unlock`). Already cleared from the screen's state.
    Submit(String),
}

pub struct LockScreen {
    pub kind: LockKind,
    /// The visible area (800x1280 or 1280x800).
    pub area: (i32, i32),
    pub wall: WallTime,
    pub(crate) entry: String,
    pub(crate) pattern: Vec<u8>,
    pub(crate) finger: Option<(i32, i32)>,
    pub(crate) shown: bool,
    pub(crate) layer: KeyboardLayer,
    pub(crate) pressed: Option<Hit>,
    /// A dragged pattern is in progress.
    pub(crate) drawing: bool,
    pub error: Option<String>,
    /// Waiting for the core's answer.
    pub busy: bool,
    /// Attempts are refused until this shell-clock time (ms).
    retry_until: Option<u64>,
    pub now: u64,
    pub(crate) hits: Vec<(Rect, Hit)>,
}

/// Seconds shown for a remaining time (rounded up).
fn secs(ms: u64) -> u64 {
    ms.div_ceil(1000)
}

impl LockScreen {
    pub fn new(kind: LockKind, area: (i32, i32), wall: WallTime) -> LockScreen {
        LockScreen {
            kind,
            area,
            wall,
            entry: String::new(),
            pattern: Vec::new(),
            finger: None,
            shown: false,
            layer: KeyboardLayer::Lower,
            pressed: None,
            drawing: false,
            error: None,
            busy: false,
            retry_until: None,
            now: 0,
            hits: Vec::new(),
        }
    }

    pub fn set_kind(&mut self, kind: LockKind) {
        if self.kind != kind {
            self.kind = kind;
            self.wipe();
            self.error = None;
        }
    }

    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.pressed = None;
    }

    /// Forget whatever was typed.
    pub fn wipe(&mut self) {
        // overwrite before dropping so the secret does not linger in the freed buffer
        let n = self.entry.len();
        self.entry.clear();
        self.entry.extend(std::iter::repeat_n('\0', n));
        self.entry.clear();
        self.pattern.clear();
        self.finger = None;
        self.drawing = false;
        self.pressed = None;
    }

    /// Attempts are refused for `secs` seconds (from `now`).
    pub fn set_retry(&mut self, secs: Option<u32>) {
        self.retry_until = secs.map(|s| self.now + u64::from(s) * 1000);
    }

    pub fn waiting(&self) -> bool {
        self.retry_until.is_some_and(|t| self.now < t)
    }

    /// Advance the clock; returns true when the picture changes (the countdown).
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.countdown();
        self.now = now;
        if self.retry_until.is_some_and(|t| now >= t) {
            self.retry_until = None;
            if self.error.is_some() && before.is_some() {
                self.error = None;
            }
        }
        before != self.countdown()
    }

    pub(crate) fn countdown(&self) -> Option<u64> {
        self.retry_until
            .and_then(|t| t.checked_sub(self.now))
            .map(secs)
    }

    /// When the loop must wake again for the countdown.
    pub fn next_deadline(&self) -> Option<u64> {
        let t = self.retry_until?;
        let left = t.saturating_sub(self.now);
        // the shown number is ceil(left / 1 s): it drops at the next whole second of `left`
        Some(self.now + if left % 1000 == 0 { 1000 } else { left % 1000 })
    }

    pub fn secret_len(&self) -> usize {
        match self.kind {
            LockKind::Pattern => self.pattern.len(),
            _ => self.entry.chars().count(),
        }
    }

    pub(crate) fn locked_out(&self) -> bool {
        self.busy || self.waiting()
    }

    /// `unlock` came back with `unlocked: false`.
    pub fn rejected(&mut self, attempts_left: Option<u32>, retry_in_s: Option<u32>) {
        self.busy = false;
        self.wipe();
        let what = match self.kind {
            LockKind::Pin => "Wrong PIN",
            LockKind::Password => "Wrong password",
            LockKind::Pattern => "Wrong pattern",
        };
        self.error = Some(match (retry_in_s, attempts_left) {
            (Some(_), _) => "Too many wrong attempts".into(),
            (None, Some(n)) if n <= 3 => {
                format!("{what}. {n} {} left", if n == 1 { "try" } else { "tries" })
            }
            _ => what.into(),
        });
        self.set_retry(retry_in_s);
    }

    /// The request failed (core away, protocol error): let the user try again.
    pub fn failed(&mut self, msg: &str) {
        self.busy = false;
        self.wipe();
        self.error = Some(msg.into());
    }
}
