//! Geometry calculations for the Phone dialpad, SMS thread bubbles, and scroll height.
use super::parse::wrap;
use super::{
    PhoneModel, PhoneView, SmsEntry, CALL_H, DISPLAY_H, GRID_MAX_W, KEY_GAP, KEY_H, LINE_H,
    NOTICE_H, SEG_H,
};
use walpad_proto::Status;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

#[derive(Debug, Clone, PartialEq)]
pub struct KeypadGeom {
    pub display: Rect,
    pub backspace: Rect,
    pub keys: Vec<(Rect, char)>,
    pub call: Rect,
    pub bottom: i32,
}

/// The dial pad: 3x4 keys, then `+` and the Call button.
pub fn keypad_geom(frame: &Frame, y0: i32) -> KeypadGeom {
    let c = frame.padded();
    let gw = c.w.min(GRID_MAX_W);
    let gx = c.x + (c.w - gw) / 2;
    let kw = (gw - 2 * KEY_GAP) / 3;
    let display = Rect::new(gx, y0, gw, DISPLAY_H);
    let backspace = Rect::new(display.right() - 72, display.y, 72, DISPLAY_H);
    let mut keys = Vec::new();
    let chars = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];
    let top = display.bottom() + KEY_GAP;
    for (i, ch) in chars.iter().enumerate() {
        let (col, row) = ((i % 3) as i32, (i / 3) as i32);
        keys.push((
            Rect::new(
                gx + col * (kw + KEY_GAP),
                top + row * (KEY_H + KEY_GAP),
                kw,
                KEY_H,
            ),
            *ch,
        ));
    }
    let ry = top + 4 * (KEY_H + KEY_GAP);
    keys.push((Rect::new(gx, ry, kw, CALL_H), '+'));
    let call = Rect::new(gx + kw + KEY_GAP, ry, 2 * kw + KEY_GAP, CALL_H);
    KeypadGeom {
        display,
        backspace,
        keys,
        call,
        bottom: call.bottom(),
    }
}

pub(super) fn body_top() -> i32 {
    SP_4 + SEG_H + NOTICE_H
}

/// Bubble geometry of the open conversation: (rect, wrapped lines) per message, newest first.
pub fn thread_geom(frame: &Frame, y0: i32, msgs: &[&SmsEntry]) -> (Vec<(Rect, Vec<String>)>, i32) {
    let c = frame.padded();
    let bw = ((c.w as f32 * 0.78) as i32).min(520);
    let chars = ((bw - 2 * SP_3) as f32 / (SIZE_BODY * 0.56)) as usize;
    let mut y = y0;
    let mut out = Vec::new();
    for m in msgs {
        let lines = wrap(&m.text, chars);
        let h = lines.len() as i32 * LINE_H + 2 * SP_3;
        let x = if m.incoming { c.x } else { c.right() - bw };
        out.push((Rect::new(x, y, bw, h), lines));
        y += h + 20 + SP_3; // the time line under the bubble
    }
    (out, y)
}

const ROW: i32 = ROW_H;

/// Total content height of the screen for the current view.
pub fn layout_total(frame: &Frame, m: &PhoneModel, st: &Option<Status>) -> i32 {
    let y0 = body_top();
    let rows = |n: usize, extra: i32| y0 + n as i32 * ROW + extra + SP_6;
    match m.view {
        PhoneView::Keypad => keypad_geom(frame, y0).bottom + SP_6,
        PhoneView::Recents => {
            let n = m.recents().len();
            rows(n.max(5), if n > 0 { SP_3 + BUTTON_H } else { 0 })
        }
        PhoneView::Contacts => {
            let n = m.filtered_contacts().len();
            rows(n.max(4), SEG_H + SP_3)
        }
        PhoneView::Messages => match &m.thread {
            None => {
                let n = m.conversations().len();
                rows(n.max(4), BUTTON_H + SP_3)
            }
            Some(_) => {
                let msgs = m.thread_messages();
                let top = y0 + ROW + SP_2 + BUTTON_H + SP_4;
                let (_, bottom) = thread_geom(frame, top, &msgs);
                let _ = st;
                bottom.max(top + 200) + SP_6
            }
        },
    }
}
