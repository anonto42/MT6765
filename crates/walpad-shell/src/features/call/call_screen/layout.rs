use walpad_proto::CallState;
use walpad_ui::Rect;

pub type Ms = u64;

/// Round buttons: small (mute, speaker, keypad) and the big answer / decline / end buttons.
pub(super) const SMALL_D: i32 = 88;
pub(super) const BIG_D: i32 = 112;
pub(super) const KEY_W: i32 = 92;
pub(super) const KEY_H: i32 = 68;
pub(super) const KEY_GAP: i32 = 10;

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
