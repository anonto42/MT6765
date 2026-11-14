//! SMS conversation list and message thread bubble rendering for the Phone screen.
use super::geom::thread_geom;
use super::parse::{can_call, fmt_when, phone};
use super::render::{pt, View};
use super::{PhoneTarget, LINE_H, ROW};
use crate::app::Target;
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

pub(super) fn render_conversations(
    p: &mut Painter,
    frame: &Frame,
    v: &View,
    y0: i32,
    hits: &mut Vec<(Rect, Target)>,
) {
    let c = frame.padded();
    let m = v.m;
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let ok = v.online && phone(v.st).is_some_and(|ph| ph.available);
    let nb = Rect::new(c.x, y0, c.w, BUTTON_H);
    let state = if !ok {
        ButtonState::Disabled
    } else if pr(PhoneTarget::NewMessage) {
        ButtonState::Pressed
    } else {
        ButtonState::Default
    };
    p.button(
        nb,
        "New message",
        ButtonKind::Primary,
        state,
        Some(Icon::Message),
    );
    hits.push((nb, pt(PhoneTarget::NewMessage)));
    let convs = m.conversations();
    let ly = y0 + BUTTON_H + SP_3;
    if convs.is_empty() {
        p.empty_state(
            Rect::new(c.x, ly, c.w, 4 * ROW),
            Icon::Message,
            "No messages",
            "Text messages you send and receive show up here",
        );
        return;
    }
    p.row_group(Rect::new(c.x, ly, c.w, convs.len() as i32 * ROW));
    let now = m.clock_ms();
    for (i, cv) in convs.iter().enumerate() {
        let r = Rect::new(c.x, ly + i as i32 * ROW, c.w, ROW);
        let title = m.label(&cv.number);
        let first = cv.last_text.lines().next().unwrap_or("");
        let sub = if cv.last_incoming {
            first.to_string()
        } else {
            format!("You: {first}")
        };
        let t = PhoneTarget::Thread(i);
        p.list_row(
            r,
            Some(Icon::Message),
            &title,
            Some(&sub),
            &Accessory::Value(fmt_when(cv.last_ts, now, &m.tz)),
            pr(t),
            false,
        );
        if cv.unread > 0 {
            p.cv.rect(Rect::new(r.x + SP_4 + ICON - 5, r.y + 9, 10, 10), text());
        }
        hits.push((r, pt(t)));
    }
}

pub(super) fn render_thread(
    p: &mut Painter,
    frame: &Frame,
    v: &View,
    y0: i32,
    num: &str,
    hits: &mut Vec<(Rect, Target)>,
) {
    let c = frame.padded();
    let m = v.m;
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let back = Rect::new(c.x, y0, 64, ROW);
    p.cv.icon(
        Icon::Back,
        back.x + (back.w - ICON) / 2,
        back.y + (back.h - ICON) / 2,
        ICON,
        if pr(PhoneTarget::ThreadBack) {
            text()
        } else {
            text_secondary()
        },
    );
    hits.push((back, pt(PhoneTarget::ThreadBack)));
    let call = Rect::new(c.right() - 112, y0 + 4, 112, ROW - 8);
    let ok = can_call(v.st, v.online, num);
    p.button(
        call,
        "Call",
        ButtonKind::Secondary,
        if !ok {
            ButtonState::Disabled
        } else if pr(PhoneTarget::ThreadCall) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        },
        Some(Icon::Phone),
    );
    hits.push((call, pt(PhoneTarget::ThreadCall)));
    let title = m.label(num);
    p.text(
        &title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(back.right(), y0, call.x - back.right() - SP_2, ROW),
        Align::Left,
    );
    let reply = Rect::new(c.x, y0 + ROW + SP_2, c.w, BUTTON_H);
    let can_send = v.online && phone(v.st).is_some_and(|ph| ph.available);
    p.button(
        reply,
        "Write a message",
        ButtonKind::Primary,
        if !can_send {
            ButtonState::Disabled
        } else if pr(PhoneTarget::Reply) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        },
        Some(Icon::Message),
    );
    hits.push((reply, pt(PhoneTarget::Reply)));
    let msgs = m.thread_messages();
    let top = reply.bottom() + SP_4;
    let (geom, _) = thread_geom(frame, top, &msgs);
    if msgs.is_empty() {
        p.empty_state(
            Rect::new(c.x, top, c.w, 200),
            Icon::Message,
            "No messages",
            "Write the first one",
        );
    }
    let now = m.clock_ms();
    for (i, ((r, lines), msg)) in geom.iter().zip(&msgs).enumerate() {
        let t = PhoneTarget::Bubble(i);
        p.cv.rect(
            *r,
            if msg.incoming {
                surface()
            } else {
                surface_strong()
            },
        );
        if pr(t) {
            p.cv.rect(*r, pressed_tint());
        }
        if msg.incoming {
            p.cv.rect(Rect::new(r.x, r.y, 3, r.h), outline());
        } else {
            p.cv.rect(Rect::new(r.right() - 3, r.y, 3, r.h), text());
        }
        for (k, l) in lines.iter().enumerate() {
            p.text(
                l,
                Style::new(Face::Regular, SIZE_BODY, text()),
                Rect::new(
                    r.x + SP_3,
                    r.y + SP_3 + k as i32 * LINE_H,
                    r.w - 2 * SP_3,
                    LINE_H,
                ),
                Align::Left,
            );
        }
        let state = match (msg.incoming, msg.state.as_str()) {
            (false, "sending") => " · Sending...",
            (false, "failed") => " · Not sent",
            (false, "sent") => " · Sent",
            _ => "",
        };
        let col = if state.contains("Not sent") {
            danger()
        } else {
            text_secondary()
        };
        p.text(
            &format!("{}{state}", fmt_when(msg.ts, now, &m.tz)),
            Style::new(Face::Regular, SIZE_CAPTION, col),
            Rect::new(r.x + SP_1, r.bottom() + 2, r.w, 18),
            if msg.incoming {
                Align::Left
            } else {
                Align::Right
            },
        );
        hits.push((*r, pt(t)));
    }
}
