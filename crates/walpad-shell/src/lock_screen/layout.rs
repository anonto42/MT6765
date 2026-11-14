//! Geometry layout for PIN, password, and pattern lock screens.

use super::{Hit, KB_H, KEY_GAP, KEY_H, KEY_W};
use walpad_proto::LockKind;
use walpad_ui::tokens::{BUTTON_H, SP_3, SP_5};
use walpad_ui::Rect;

/// Where everything sits for an area and a kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub clock: Rect,
    pub date: Rect,
    pub prompt: Rect,
    /// PIN dots row / password field.
    pub entry: Rect,
    pub pad: Vec<(Rect, Hit)>,
    pub keyboard: Rect,
    pub submit: Rect,
    /// The 3x3 dot centres, row-major.
    pub dots: [(i32, i32); 9],
}

pub fn layout(area: (i32, i32), kind: LockKind) -> Layout {
    let (w, h) = area;
    let landscape = w > h;
    let mut l = Layout {
        clock: Rect::new(0, 0, 0, 0),
        date: Rect::new(0, 0, 0, 0),
        prompt: Rect::new(0, 0, 0, 0),
        entry: Rect::new(0, 0, 0, 0),
        pad: Vec::new(),
        keyboard: Rect::new(0, 0, 0, 0),
        submit: Rect::new(0, 0, 0, 0),
        dots: [(0, 0); 9],
    };
    match kind {
        LockKind::Password => {
            // clock on top, the field above the keyboard, Unlock at the bottom
            let (ct, ch) = if landscape { (16, 76) } else { (96, 100) };
            l.clock = Rect::new(0, ct, w, ch);
            l.date = Rect::new(0, ct + ch, w, 28);
            let kw = 800.min(w);
            l.submit = Rect::new(
                (w - kw) / 2 + SP_5,
                h - 24 - BUTTON_H,
                kw - 2 * SP_5,
                BUTTON_H,
            );
            l.keyboard = Rect::new((w - kw) / 2, l.submit.y - SP_3 - KB_H, kw, KB_H);
            l.entry = Rect::new(
                (w - kw) / 2 + SP_5,
                l.keyboard.y - 34 - 56,
                kw - 2 * SP_5,
                56,
            );
            l.prompt = Rect::new(l.entry.x, l.entry.y - 36, l.entry.w, 28);
        }
        LockKind::Pin | LockKind::Pattern => {
            // portrait: stacked and centred; landscape: clock left, input right
            let (cx_clock, cx_in) = if landscape {
                (w / 4, w * 3 / 4)
            } else {
                (w / 2, w / 2)
            };
            let (ct, ch) = if landscape {
                (h / 2 - 120, 100)
            } else {
                (110, 100)
            };
            let half = if landscape { w / 2 } else { w };
            l.clock = Rect::new(cx_clock - half / 2, ct, half, ch);
            l.date = Rect::new(l.clock.x, ct + ch, half, 28);
            l.prompt = Rect::new(l.clock.x, l.date.bottom() + 36, half, 28);
            if kind == LockKind::Pin {
                let gw = 3 * KEY_W + 2 * KEY_GAP;
                let gh = 4 * KEY_H + 3 * KEY_GAP;
                let x0 = cx_in - gw / 2;
                let y0 = if landscape { (h - gh) / 2 + 32 } else { 470 };
                l.entry = Rect::new(x0, y0 - 64, gw, 32);
                let keys = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
                for (i, k) in keys.iter().enumerate() {
                    let (r, c) = (i as i32 / 3, i as i32 % 3);
                    l.pad.push((
                        Rect::new(
                            x0 + c * (KEY_W + KEY_GAP),
                            y0 + r * (KEY_H + KEY_GAP),
                            KEY_W,
                            KEY_H,
                        ),
                        Hit::Digit(*k),
                    ));
                }
                let y3 = y0 + 3 * (KEY_H + KEY_GAP);
                let at = |c: i32| Rect::new(x0 + c * (KEY_W + KEY_GAP), y3, KEY_W, KEY_H);
                l.pad.push((at(0), Hit::Backspace));
                l.pad.push((at(1), Hit::Digit('0')));
                l.pad.push((at(2), Hit::Submit));
            } else {
                let step = if landscape { 130 } else { 150 };
                let (cy0, cx0) = (if landscape { h / 2 - step } else { 600 }, cx_in - step);
                for i in 0..9 {
                    l.dots[i] = (cx0 + (i as i32 % 3) * step, cy0 + (i as i32 / 3) * step);
                }
                l.entry = Rect::new(cx0 - 60, cy0 - 100, 2 * step + 120, 28);
            }
        }
    }
    l
}
