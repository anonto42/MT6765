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
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style, Tint};

pub type Ms = u64;

/// Round buttons: small (mute, speaker, keypad) and the big answer / decline / end buttons.
const SMALL_D: i32 = 88;
const BIG_D: i32 = 112;
const KEY_W: i32 = 92;
const KEY_H: i32 = 68;
const KEY_GAP: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallTarget {
    Answer,
    Decline,
    Hangup,
    Mute,
    Speaker,
    Keypad,
    Digit(char),
    HideKeypad,
}

/// What the owner asked for; the shell turns it into a core command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAct {
    Answer,
    /// Decline a ringing call or end a running one.
    Hangup,
    Mute(bool),
    Speaker(bool),
    Dtmf(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    None,
    Redraw,
    Act(CallAct),
}

/// The hit areas for a call state: (rectangle, target). Circles are given by their bounding box.
pub fn layout(area: (i32, i32), call: CallState, keypad: bool) -> Vec<(Rect, CallTarget)> {
    let (w, h) = area;
    let cx = w / 2;
    let landscape = w > h;
    let mut v = Vec::new();
    let circle = |cx: i32, cy: i32, d: i32| Rect::new(cx - d / 2, cy - d / 2, d, d);
    match call {
        CallState::Ringing => {
            let y = h - if landscape { 170 } else { 240 };
            v.push((circle(cx - 150, y, BIG_D), CallTarget::Decline));
            v.push((circle(cx + 150, y, BIG_D), CallTarget::Answer));
        }
        CallState::Dialing | CallState::Active | CallState::Held => {
            let end_y = h - if landscape { 100 } else { 200 };
            v.push((circle(cx, end_y, BIG_D), CallTarget::Hangup));
            if keypad {
                let top = if landscape { 250 } else { 340 };
                let gw = 3 * KEY_W + 2 * KEY_GAP;
                let chars = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];
                for (i, c) in chars.iter().enumerate() {
                    let (col, row) = ((i % 3) as i32, (i / 3) as i32);
                    v.push((
                        Rect::new(
                            cx - gw / 2 + col * (KEY_W + KEY_GAP),
                            top + row * (KEY_H + KEY_GAP),
                            KEY_W,
                            KEY_H,
                        ),
                        CallTarget::Digit(*c),
                    ));
                }
                v.push((
                    Rect::new(cx + 100, end_y - 28, 190.min(w - cx - 110), 56),
                    CallTarget::HideKeypad,
                ));
            } else {
                let y = h - if landscape { 290 } else { 400 };
                v.push((circle(cx - 150, y, SMALL_D), CallTarget::Mute));
                v.push((circle(cx, y, SMALL_D), CallTarget::Keypad));
                v.push((circle(cx + 150, y, SMALL_D), CallTarget::Speaker));
            }
        }
        CallState::Idle | CallState::Ended => {}
    }
    v
}

pub fn hit_test(l: &[(Rect, CallTarget)], x: i32, y: i32) -> Option<CallTarget> {
    l.iter().find_map(|(r, t)| {
        let inside = match t {
            // circles: the inscribed circle, not the corners of the box
            CallTarget::Answer
            | CallTarget::Decline
            | CallTarget::Hangup
            | CallTarget::Mute
            | CallTarget::Speaker
            | CallTarget::Keypad => {
                let (cx, cy) = r.center();
                let rad = r.w / 2;
                (x - cx).pow(2) + (y - cy).pow(2) <= rad * rad
            }
            _ => r.contains(x, y),
        };
        inside.then_some(*t)
    })
}

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

    pub fn render(&mut self, p: &mut Painter) {
        p.cv.fill(BG);
        let (w, h) = self.area;
        let landscape = w > h;
        let cx = w / 2;
        let top = if landscape { 40 } else { 140 };
        let (who, sub) = self.who();
        let call = self.st.call;
        // the state line
        let head_col = match call {
            CallState::Ended => TEXT_SECONDARY,
            CallState::Ringing => TEXT,
            _ => TEXT_SECONDARY,
        };
        let label = match call {
            CallState::Active | CallState::Held => "In call".to_string(),
            CallState::Ringing => "Incoming call".to_string(),
            CallState::Dialing => "Calling...".to_string(),
            CallState::Ended => "Call ended".to_string(),
            CallState::Idle => String::new(),
        };
        p.text(
            &label,
            Style::new(Face::Regular, SIZE_BODY, head_col),
            Rect::new(0, top, w, 28),
            Align::Center,
        );
        p.cv.icon(Icon::Phone, cx - 28, top + 40, 56, TEXT_DISABLED);
        p.text(
            &who,
            Style::new(Face::Bold, 36.0, TEXT),
            Rect::new(SP_5, top + 108, w - 2 * SP_5, 48),
            Align::Center,
        );
        if let Some(n) = sub {
            p.text(
                &n,
                Style::new(Face::Regular, SIZE_BODY, TEXT_SECONDARY),
                Rect::new(SP_5, top + 158, w - 2 * SP_5, 28),
                Align::Center,
            );
        }
        // timer / outcome
        let big = match call {
            CallState::Active | CallState::Held | CallState::Ended => self.headline(),
            _ => String::new(),
        };
        let big_col = if call == CallState::Ended {
            WARNING
        } else {
            TEXT
        };
        let big_y = if landscape { top + 186 } else { top + 196 };
        if !big.is_empty() {
            p.text(
                &big,
                Style::new(Face::Regular, 30.0, big_col),
                Rect::new(0, big_y, w, 40),
                Align::Center,
            );
        }
        if self.keypad && !self.typed.is_empty() {
            let shown: String = self
                .typed
                .chars()
                .rev()
                .take(16)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            p.text(
                &shown,
                Style::new(Face::Regular, 26.0, TEXT_SECONDARY),
                Rect::new(0, big_y + 40, w, 36),
                Align::Center,
            );
        }
        // the honest line
        if matches!(
            call,
            CallState::Ringing | CallState::Dialing | CallState::Active | CallState::Held
        ) {
            p.text(
                "Call audio is experimental: it may be silent",
                Style::new(Face::Regular, SIZE_CAPTION, WARNING),
                Rect::new(SP_5, h - 36, w - 2 * SP_5, 22),
                Align::Center,
            );
        }
        let l = self.layout();
        for (r, t) in &l {
            let pressed = self.pressed == Some(*t);
            match t {
                CallTarget::Answer => {
                    self.round(p, *r, Icon::Phone, 0.0, SUCCESS, true, pressed, "Answer")
                }
                CallTarget::Decline => {
                    self.round(p, *r, Icon::Phone, 135.0, DANGER, true, pressed, "Decline")
                }
                CallTarget::Hangup => {
                    self.round(p, *r, Icon::Phone, 135.0, DANGER, true, pressed, "End")
                }
                CallTarget::Mute => self.round(
                    p,
                    *r,
                    if self.st.muted {
                        Icon::MicOff
                    } else {
                        Icon::Mic
                    },
                    0.0,
                    TEXT,
                    self.st.muted,
                    pressed,
                    "Mute",
                ),
                CallTarget::Speaker => self.round(
                    p,
                    *r,
                    Icon::Volume,
                    0.0,
                    TEXT,
                    self.st.speaker,
                    pressed,
                    "Speaker",
                ),
                CallTarget::Keypad => {
                    self.round(p, *r, Icon::Keypad, 0.0, TEXT, false, pressed, "Keypad")
                }
                CallTarget::Digit(c) => {
                    p.outline_box(*r, RADIUS, pressed, false, false);
                    p.text(
                        &c.to_string(),
                        Style::new(Face::Regular, 32.0, TEXT),
                        *r,
                        Align::Center,
                    );
                }
                CallTarget::HideKeypad => {
                    p.outline_box(*r, RADIUS, pressed, false, false);
                    p.text(
                        "Hide keypad",
                        Style::new(Face::Regular, SIZE_BODY, TEXT),
                        *r,
                        Align::Center,
                    );
                }
            }
        }
    }

    /// A round button: translucent fill, outline (filled tint when `on` or coloured), icon, label.
    #[allow(clippy::too_many_arguments)]
    fn round(
        &self,
        p: &mut Painter,
        r: Rect,
        icon: Icon,
        angle: f32,
        color: Color,
        on: bool,
        pressed: bool,
        label: &str,
    ) {
        let (cx, cy) = r.center();
        let rad = r.w as f32 / 2.0;
        p.cv.circle(cx as f32, cy as f32, rad, SURFACE_STRONG);
        if on {
            p.cv.circle(cx as f32, cy as f32, rad, Tint::new(color, 22));
        }
        if pressed {
            p.cv.circle(cx as f32, cy as f32, rad, PRESSED_TINT);
        }
        p.cv.rrect_stroke(r, r.w / 2, 2.0, if on || pressed { color } else { OUTLINE });
        let s = r.w * 2 / 5;
        p.cv.icon_rotated(icon, cx - s / 2, cy - s / 2, s, color, angle);
        p.text(
            label,
            Style::new(Face::Regular, SIZE_CAPTION, TEXT_SECONDARY),
            Rect::new(r.x - 20, r.bottom() + 6, r.w + 40, 20),
            Align::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(call: CallState) -> PhoneStatus {
        PhoneStatus {
            available: true,
            call,
            incoming: call == CallState::Ringing,
            number: Some("01711234567".into()),
            audio: "experimental".into(),
            ..Default::default()
        }
    }

    #[test]
    fn buttons_are_big_and_inside_the_screen() {
        for area in [(800, 1280), (1280, 800)] {
            for (call, kp) in [
                (CallState::Ringing, false),
                (CallState::Dialing, false),
                (CallState::Active, false),
                (CallState::Active, true),
                (CallState::Held, false),
            ] {
                let l = layout(area, call, kp);
                assert!(!l.is_empty());
                for (i, (r, t)) in l.iter().enumerate() {
                    assert!(r.w >= 56 && r.h >= 56, "{t:?} {r:?}");
                    assert!(
                        r.x >= 0 && r.y >= 0 && r.right() <= area.0 && r.bottom() <= area.1,
                        "{call:?} {t:?} {r:?} in {area:?}"
                    );
                    for (r2, t2) in &l[i + 1..] {
                        let o = r.x < r2.right()
                            && r2.x < r.right()
                            && r.y < r2.bottom()
                            && r2.y < r.bottom();
                        assert!(!o, "{call:?}: {t:?} overlaps {t2:?}");
                    }
                }
            }
        }
        assert!(layout((800, 1280), CallState::Ended, false).is_empty());
        assert!(layout((800, 1280), CallState::Idle, false).is_empty());
    }

    #[test]
    fn ringing_offers_answer_and_decline_only() {
        let l = layout((800, 1280), CallState::Ringing, false);
        let t: Vec<CallTarget> = l.iter().map(|x| x.1).collect();
        assert_eq!(t, [CallTarget::Decline, CallTarget::Answer]);
    }

    #[test]
    fn taps_become_acts_and_a_slid_off_press_is_cancelled() {
        let mut s = CallScreen::new((800, 1280));
        s.set_status(&st(CallState::Ringing), 0);
        let l = layout(s.area, CallState::Ringing, false);
        let (a, _) = l.iter().find(|x| x.1 == CallTarget::Answer).unwrap();
        let (cx, cy) = a.center();
        assert_eq!(s.down(cx, cy), Out::Redraw);
        assert_eq!(s.up(cx, cy), Out::Act(CallAct::Answer));
        // press on answer, release elsewhere: nothing
        s.down(cx, cy);
        assert_eq!(s.up(5, 5), Out::Redraw);
        s.down(cx, cy);
        assert_eq!(s.moved(5, 5), Out::Redraw);
        assert_eq!(s.up(cx, cy), Out::None);
        // a tap in the empty area does nothing
        assert_eq!(s.down(5, 5), Out::None);
        assert_eq!(s.up(5, 5), Out::None);
        // the corner of a circle's box is not part of the button
        let corner = (a.x + 3, a.y + 3);
        assert_eq!(s.down(corner.0, corner.1), Out::None);
        // decline = hangup
        let (d, _) = l.iter().find(|x| x.1 == CallTarget::Decline).unwrap();
        let (dx, dy) = d.center();
        s.down(dx, dy);
        assert_eq!(s.up(dx, dy), Out::Act(CallAct::Hangup));
    }

    #[test]
    fn in_call_controls_toggle_and_send_tones() {
        let mut s = CallScreen::new((1280, 800));
        let mut a = st(CallState::Active);
        a.elapsed_s = 59;
        s.set_status(&a, 1000);
        let tap = |s: &mut CallScreen, t: CallTarget| {
            let (r, _) = layout(s.area, s.st.call, s.keypad)
                .into_iter()
                .find(|x| x.1 == t)
                .unwrap();
            let (x, y) = r.center();
            s.down(x, y);
            s.up(x, y)
        };
        assert_eq!(tap(&mut s, CallTarget::Mute), Out::Act(CallAct::Mute(true)));
        a.muted = true;
        a.speaker = true;
        s.set_status(&a, 1000);
        assert_eq!(
            tap(&mut s, CallTarget::Mute),
            Out::Act(CallAct::Mute(false))
        );
        assert_eq!(
            tap(&mut s, CallTarget::Speaker),
            Out::Act(CallAct::Speaker(false))
        );
        assert_eq!(tap(&mut s, CallTarget::Keypad), Out::Redraw);
        assert!(s.keypad);
        assert_eq!(
            tap(&mut s, CallTarget::Digit('5')),
            Out::Act(CallAct::Dtmf('5'))
        );
        assert_eq!(
            tap(&mut s, CallTarget::Digit('#')),
            Out::Act(CallAct::Dtmf('#'))
        );
        assert_eq!(s.typed, "5#");
        assert_eq!(tap(&mut s, CallTarget::HideKeypad), Out::Redraw);
        assert!(!s.keypad);
        assert_eq!(tap(&mut s, CallTarget::Hangup), Out::Act(CallAct::Hangup));
        // a new call starts with a clean dial pad
        s.keypad = true;
        s.set_status(&st(CallState::Idle), 5000);
        s.set_status(&st(CallState::Dialing), 6000);
        assert!(!s.keypad && s.typed.is_empty());
    }

    #[test]
    fn timer_counts_on_from_the_core_and_ticks_each_second() {
        let mut s = CallScreen::new((800, 1280));
        let mut a = st(CallState::Active);
        a.elapsed_s = 59;
        s.set_status(&a, 10_000);
        assert_eq!(s.headline(), "0:59");
        assert!(!s.tick(10_500));
        assert!(s.tick(11_000));
        assert_eq!(s.headline(), "1:00");
        assert_eq!(s.next_deadline(11_250), Some(750));
        s.set_status(&st(CallState::Ringing), 20_000);
        assert_eq!(s.next_deadline(20_000), None);
        assert_eq!(s.headline(), "Incoming call");
        let mut e = st(CallState::Ended);
        e.end_reason = Some("busy".into());
        s.set_status(&e, 30_000);
        assert_eq!(s.headline(), "Busy");
        assert!(s.wanted());
        s.set_status(&st(CallState::Idle), 31_000);
        assert!(!s.wanted());
    }

    #[test]
    fn the_caller_shows_by_name_or_number() {
        let mut s = CallScreen::new((800, 1280));
        s.set_status(&st(CallState::Ringing), 0);
        assert_eq!(s.who(), ("01711234567".to_string(), None));
        let mut n = st(CallState::Ringing);
        n.name = Some("Rahim".into());
        s.set_status(&n, 0);
        assert_eq!(
            s.who(),
            ("Rahim".to_string(), Some("01711234567".to_string()))
        );
        n.number = None;
        n.name = None;
        s.set_status(&n, 0);
        assert_eq!(s.who().0, "Unknown number");
    }
}
