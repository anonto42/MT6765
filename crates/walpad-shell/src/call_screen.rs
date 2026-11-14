//! The call screen (owner request 2026-11-11, change `phone-dialer`): the incoming-call sheet
//! (answer / decline), the outgoing "calling" state, the in-call screen (timer, mute, speaker,
//! dial pad for tones, end) and the short "call ended" state. Pure: `call_window.rs` puts it on
//! screen in its own window above every app and above the lock screen.
//!
//! Lock screen policy: an incoming or running call covers the lock screen with this window, which
//! shows the caller (number and contact name) and offers only answer, decline, mute, speaker,
//! tones and end. Nothing else of the tablet is reachable through it; when the call is over the
//! lock screen is back, untouched. Call audio is experimental until verified on the device: the
//! screen says so.
use crate::phone_ui::{end_reason_text, fmt_duration};
use walpad_proto::{CallState, PhoneStatus};
use walpad_ui::Rect;

mod layout;
mod render;
#[cfg(test)]
mod tests;

pub use layout::{hit_test, layout, CallAct, CallTarget, Ms, Out};

pub struct CallScreen {
    pub area: (i32, i32),
    pub st: PhoneStatus,
    /// Shell clock when `st` arrived: the timer counts on from `st.elapsed_s`.
    base: Ms,
    now: Ms,
    pub keypad: bool,
    /// Tones sent in this call (the last few are shown).
    pub typed: String,
    pressed: Option<CallTarget>,
    last_secs: u64,
}

impl CallScreen {
    pub fn new(area: (i32, i32)) -> CallScreen {
        CallScreen {
            area,
            st: PhoneStatus::default(),
            base: 0,
            now: 0,
            keypad: false,
            typed: String::new(),
            pressed: None,
            last_secs: 0,
        }
    }

    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
    }

    /// A new status from the core. A different call (or the end of one) resets the dial pad.
    pub fn set_status(&mut self, st: &PhoneStatus, now: Ms) {
        if matches!(
            st.call,
            CallState::Idle | CallState::Ended | CallState::Ringing
        ) || self.st.call == CallState::Idle
            || self.st.call == CallState::Ended
        {
            self.keypad = false;
            self.typed.clear();
        }
        self.st = st.clone();
        self.base = now;
        self.now = now;
        self.last_secs = self.elapsed(now);
    }

    pub fn elapsed(&self, now: Ms) -> u64 {
        match self.st.call {
            CallState::Active | CallState::Held => {
                self.st.elapsed_s + now.saturating_sub(self.base) / 1000
            }
            _ => 0,
        }
    }

    /// The screen shows something (a call exists).
    pub fn wanted(&self) -> bool {
        !matches!(self.st.call, CallState::Idle)
    }

    pub fn tick(&mut self, now: Ms) -> bool {
        self.now = now;
        let s = self.elapsed(now);
        let changed = s != self.last_secs;
        self.last_secs = s;
        changed && matches!(self.st.call, CallState::Active | CallState::Held)
    }

    /// Milliseconds until the timer ticks (None when no timer runs).
    pub fn next_deadline(&self, now: Ms) -> Option<Ms> {
        matches!(self.st.call, CallState::Active | CallState::Held)
            .then(|| 1000 - (now.saturating_sub(self.base) % 1000))
    }

    fn layout(&self) -> Vec<(Rect, CallTarget)> {
        layout(self.area, self.st.call, self.keypad)
    }

    pub fn down(&mut self, x: i32, y: i32) -> Out {
        self.pressed = hit_test(&self.layout(), x, y);
        if self.pressed.is_some() {
            Out::Redraw
        } else {
            Out::None
        }
    }

    pub fn moved(&mut self, x: i32, y: i32) -> Out {
        if self.pressed.is_some() && hit_test(&self.layout(), x, y) != self.pressed {
            self.pressed = None;
            return Out::Redraw;
        }
        Out::None
    }

    /// A tap is a press and a release on the same button.
    pub fn up(&mut self, x: i32, y: i32) -> Out {
        let Some(t) = self.pressed.take() else {
            return Out::None;
        };
        if hit_test(&self.layout(), x, y) != Some(t) {
            return Out::Redraw;
        }
        match t {
            CallTarget::Answer => Out::Act(CallAct::Answer),
            CallTarget::Decline | CallTarget::Hangup => Out::Act(CallAct::Hangup),
            CallTarget::Mute => Out::Act(CallAct::Mute(!self.st.muted)),
            CallTarget::Speaker => Out::Act(CallAct::Speaker(!self.st.speaker)),
            CallTarget::Keypad => {
                self.keypad = true;
                Out::Redraw
            }
            CallTarget::HideKeypad => {
                self.keypad = false;
                Out::Redraw
            }
            CallTarget::Digit(c) => {
                if self.typed.chars().count() < 32 {
                    self.typed.push(c);
                }
                Out::Act(CallAct::Dtmf(c))
            }
        }
    }

    /// Who is calling / being called: the contact's name, else the number.
    pub fn who(&self) -> (String, Option<String>) {
        match (&self.st.name, &self.st.number) {
            (Some(n), Some(num)) => (n.clone(), Some(num.clone())),
            (None, Some(num)) => (num.clone(), None),
            (Some(n), None) => (n.clone(), None),
            (None, None) => ("Unknown number".into(), None),
        }
    }

    pub fn headline(&self) -> String {
        match (self.st.call, self.st.incoming) {
            (CallState::Ringing, _) => "Incoming call".into(),
            (CallState::Dialing, _) => "Calling...".into(),
            (CallState::Active, _) => fmt_duration(self.elapsed(self.now)),
            (CallState::Held, _) => format!("On hold  {}", fmt_duration(self.elapsed(self.now))),
            (CallState::Ended, _) => end_reason_text(self.st.end_reason.as_deref()).into(),
            (CallState::Idle, _) => String::new(),
        }
    }
}
