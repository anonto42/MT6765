use super::{CallScreen, CallTarget};
use walpad_proto::CallState;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

impl CallScreen {
    pub fn render(&mut self, p: &mut Painter) {
        p.cv.fill(bg());
        let (w, h) = self.area;
        let landscape = w > h;
        let cx = w / 2;
        let top = if landscape { 40 } else { 140 };
        let (who, sub) = self.who();
        let call = self.st.call;
        // the state line
        let head_col = match call {
            CallState::Ended => text_secondary(),
            CallState::Ringing => text(),
            _ => text_secondary(),
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
        p.cv.icon(Icon::Phone, cx - 28, top + 40, 56, text_disabled());
        p.text(
            &who,
            Style::new(Face::Bold, 36.0, text()),
            Rect::new(SP_5, top + 108, w - 2 * SP_5, 48),
            Align::Center,
        );
        if let Some(n) = sub {
            p.text(
                &n,
                Style::new(Face::Regular, SIZE_BODY, text_secondary()),
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
            warning()
        } else {
            text()
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
                Style::new(Face::Regular, 26.0, text_secondary()),
                Rect::new(0, big_y + 40, w, 36),
                Align::Center,
            );
        }
        if self.st.audio == "experimental"
            && matches!(
                call,
                CallState::Ringing | CallState::Dialing | CallState::Active | CallState::Held
            )
        {
            p.text(
                "Call audio is experimental: it may be silent",
                Style::new(Face::Regular, SIZE_CAPTION, warning()),
                Rect::new(SP_5, h - 36, w - 2 * SP_5, 22),
                Align::Center,
            );
        }
        let l = self.layout();
        for (r, t) in &l {
            let pressed = self.pressed == Some(*t);
            match t {
                CallTarget::Answer => {
                    self.round(p, *r, Icon::Phone, 0.0, success(), true, pressed, "Answer")
                }
                CallTarget::Decline => self.round(
                    p,
                    *r,
                    Icon::Phone,
                    135.0,
                    danger(),
                    true,
                    pressed,
                    "Decline",
                ),
                CallTarget::Hangup => {
                    self.round(p, *r, Icon::Phone, 135.0, danger(), true, pressed, "End")
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
                    text(),
                    self.st.muted,
                    pressed,
                    "Mute",
                ),
                CallTarget::Speaker => self.round(
                    p,
                    *r,
                    Icon::Volume,
                    0.0,
                    text(),
                    self.st.speaker,
                    pressed,
                    "Speaker",
                ),
                CallTarget::Keypad => {
                    self.round(p, *r, Icon::Keypad, 0.0, text(), false, pressed, "Keypad")
                }
                CallTarget::Digit(c) => {
                    p.outline_box(*r, 0, pressed, false, false);
                    p.text(
                        &c.to_string(),
                        Style::new(Face::Regular, 32.0, text()),
                        *r,
                        Align::Center,
                    );
                }
                CallTarget::HideKeypad => {
                    p.outline_box(*r, 0, pressed, false, false);
                    p.text(
                        "Hide keypad",
                        Style::new(Face::Regular, SIZE_BODY, text()),
                        *r,
                        Align::Center,
                    );
                }
            }
        }
    }

    /// Action button: sharp box fill, bottom indicator bar, icon, label.
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
        p.cv.rect(r, surface_strong());
        if on {
            p.cv.rect(r, Tint::new(color, 22));
        }
        if pressed {
            p.cv.rect(r, pressed_tint());
        }
        p.cv.rect(
            Rect::new(r.x, r.bottom() - 3, r.w, 3),
            if on || pressed { color } else { divider() },
        );
        let s = r.w * 2 / 5;
        p.cv.icon_rotated(icon, cx - s / 2, cy - s / 2, s, color, angle);
        p.text(
            label,
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(r.x - 20, r.bottom() + 6, r.w + 40, 20),
            Align::Center,
        );
    }
}
