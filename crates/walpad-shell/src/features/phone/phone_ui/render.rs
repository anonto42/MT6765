//! Main Phone screen rendering (tabs, notice line, keypad, recents, contacts).
use super::geom::{body_top, keypad_geom};
use super::messages::{render_conversations, render_thread};
use super::parse::{call_kind_text, can_call, fmt_duration, fmt_when, notice_line, unread};
use super::{CallKind, PhoneModel, PhoneTarget, PhoneView, MAX_DIAL, NOTICE_H, ROW, SEG_H};
use crate::app::Target;
use walpad_proto::Status;
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

pub struct View<'a> {
    pub st: &'a Option<Status>,
    pub m: &'a PhoneModel,
    pub online: bool,
    pub pressed: Option<Target>,
}

pub(super) fn pt(t: PhoneTarget) -> Target {
    Target::Phone(t)
}

/// A key of the dial pad: sharp box, big digit.
fn key_box(p: &mut Painter, r: Rect, label: &str, pressed: bool, disabled: bool) {
    p.outline_box(r, 0, pressed, false, disabled);
    p.text(
        label,
        Style::new(
            Face::Regular,
            34.0,
            if disabled { text_disabled() } else { text() },
        ),
        r,
        Align::Center,
    );
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let c = frame.padded();
    let mut hits: Vec<(Rect, Target)> = Vec::new();
    let pr = |t: PhoneTarget| v.pressed == Some(pt(t));
    let m = v.m;
    // tabs
    let seg = Rect::new(c.x, SP_4, c.w, SEG_H);
    let unread = unread(v.st);
    let msg_label = if unread > 0 {
        format!("Messages ({unread})")
    } else {
        "Messages".to_string()
    };
    let labels = ["Keypad", "Recents", "Contacts", msg_label.as_str()];
    let selected = PhoneView::ALL
        .iter()
        .position(|x| *x == m.view)
        .unwrap_or(0);
    let pressed_seg = PhoneView::ALL
        .iter()
        .position(|x| v.pressed == Some(pt(PhoneTarget::View(*x))));
    let rects = p.segmented(seg, &labels, selected, pressed_seg, false);
    for (r, view) in rects.iter().zip(PhoneView::ALL) {
        hits.push((*r, pt(PhoneTarget::View(view))));
    }
    let (note, note_col) = notice_line(v.st, v.online);
    p.text(
        &note,
        Style::new(Face::Regular, SIZE_CAPTION, note_col),
        Rect::new(c.x + SP_1, seg.bottom() + SP_1, c.w - SP_2, NOTICE_H - SP_2),
        Align::Left,
    );
    let y0 = body_top();
    match m.view {
        PhoneView::Keypad => {
            let g = keypad_geom(frame, y0);
            p.outline_box(g.display, 0, false, false, false);
            let shown = if m.dial.is_empty() {
                "Enter a number".to_string()
            } else {
                m.dial.clone()
            };
            let col = if m.dial.is_empty() {
                text_disabled()
            } else {
                text()
            };
            let size = if m.dial.chars().count() > 16 {
                26.0
            } else {
                34.0
            };
            p.text(
                &shown,
                Style::new(Face::Regular, size, col),
                Rect::new(
                    g.display.x + SP_4,
                    g.display.y,
                    g.display.w - SP_4 - 72,
                    g.display.h,
                ),
                Align::Left,
            );
            if !m.dial.is_empty() {
                let b = g.backspace;
                p.cv.icon(
                    Icon::Backspace,
                    b.x + (b.w - ICON) / 2,
                    b.y + (b.h - ICON) / 2,
                    ICON,
                    if pr(PhoneTarget::Backspace) {
                        text()
                    } else {
                        text_secondary()
                    },
                );
                hits.push((b, pt(PhoneTarget::Backspace)));
            }
            let full = m.dial.len() >= MAX_DIAL;
            for (r, ch) in &g.keys {
                let t = PhoneTarget::Digit(*ch);
                key_box(p, *r, &ch.to_string(), pr(t), full);
                hits.push((*r, pt(t)));
            }
            let ok = can_call(v.st, v.online, &m.dial);
            let state = if !ok {
                ButtonState::Disabled
            } else if pr(PhoneTarget::Call) {
                ButtonState::Pressed
            } else {
                ButtonState::Default
            };
            p.button(
                g.call,
                "Call",
                ButtonKind::Primary,
                state,
                Some(Icon::Phone),
            );
            hits.push((g.call, pt(PhoneTarget::Call)));
            if let Some(u) = v.st.as_ref().and_then(|s| s.phone.ussd.as_ref()) {
                let r = Rect::new(c.x, g.call.bottom() + SP_3, c.w, ROW);
                p.card(r, pr(PhoneTarget::UssdDismiss));
                let msg = u.text.as_deref().unwrap_or("Running USSD...");
                p.text(
                    msg,
                    Style::new(Face::Regular, SIZE_BODY, text()),
                    Rect::new(r.x + SP_4, r.y, r.w - 2 * SP_4, r.h),
                    Align::Left,
                );
                hits.push((r, pt(PhoneTarget::UssdDismiss)));
            }
        }
        PhoneView::Recents => {
            let recents = m.recents();
            if recents.is_empty() {
                p.empty_state(
                    Rect::new(c.x, y0, c.w, 5 * ROW),
                    Icon::Phone,
                    "No calls yet",
                    "Calls you make and receive show up here",
                );
            } else {
                p.row_group(Rect::new(c.x, y0, c.w, recents.len() as i32 * ROW));
                let now = m.clock_ms();
                for (i, e) in recents.iter().enumerate() {
                    let r = Rect::new(c.x, y0 + i as i32 * ROW, c.w, ROW);
                    let title = e
                        .name
                        .clone()
                        .or_else(|| m.contact_name(&e.number).map(String::from))
                        .unwrap_or_else(|| {
                            if e.number.is_empty() {
                                "Unknown".into()
                            } else {
                                e.number.clone()
                            }
                        });
                    let dur = if e.dur_s > 0 {
                        format!(" · {}", fmt_duration(e.dur_s))
                    } else {
                        String::new()
                    };
                    let sub = format!("{}{dur}", call_kind_text(e.kind));
                    let when = fmt_when(e.ts, now, &m.tz);
                    let t = PhoneTarget::Recent(i);
                    p.list_row(
                        r,
                        Some(Icon::Phone),
                        &title,
                        Some(&sub),
                        &Accessory::Value(when),
                        pr(t),
                        false,
                    );
                    if e.kind == CallKind::Missed {
                        p.cv.rect(Rect::new(r.x + SP_4 + ICON - 4, r.y + 10, 8, 8), danger());
                    }
                    hits.push((r, pt(t)));
                }
                let by = y0 + recents.len() as i32 * ROW + SP_3;
                let b = Rect::new(c.x, by, c.w.min(320), BUTTON_H);
                p.button(
                    b,
                    "Clear call log",
                    ButtonKind::Secondary,
                    if pr(PhoneTarget::ClearLog) {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Default
                    },
                    None,
                );
                hits.push((b, pt(PhoneTarget::ClearLog)));
            }
        }
        PhoneView::Contacts => {
            let search = Rect::new(c.x, y0, c.w - 210 - SP_3, SEG_H);
            let sim_btn = Rect::new(c.right() - 210, y0, 82, SEG_H);
            let add = Rect::new(c.right() - 120, y0, 120, SEG_H);
            p.outline_box(search, 0, pr(PhoneTarget::Search), false, false);
            let (txt, col) = if m.search.is_empty() {
                ("Search contacts".to_string(), text_disabled())
            } else {
                (m.search.clone(), text())
            };
            p.text(
                &txt,
                Style::new(Face::Regular, SIZE_BODY, col),
                Rect::new(search.x + SP_4, search.y, search.w - 2 * SP_4, search.h),
                Align::Left,
            );
            hits.push((search, pt(PhoneTarget::Search)));
            p.button(
                sim_btn,
                "SIM",
                ButtonKind::Secondary,
                if !v.online {
                    ButtonState::Disabled
                } else if pr(PhoneTarget::SimContacts) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                },
                None,
            );
            hits.push((sim_btn, pt(PhoneTarget::SimContacts)));
            p.button(
                add,
                "Add",
                ButtonKind::Primary,
                if pr(PhoneTarget::AddContact) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                },
                Some(Icon::Person),
            );
            hits.push((add, pt(PhoneTarget::AddContact)));
            let list = m.filtered_contacts();
            let ly = y0 + SEG_H + SP_3;
            if list.is_empty() {
                let (title, hint) = if m.contacts.is_empty() {
                    ("No contacts", "Tap Add to save one")
                } else {
                    ("No match", "Try another name or number")
                };
                p.empty_state(Rect::new(c.x, ly, c.w, 4 * ROW), Icon::Person, title, hint);
            } else {
                p.row_group(Rect::new(c.x, ly, c.w, list.len() as i32 * ROW));
                for (i, e) in list.iter().enumerate() {
                    let r = Rect::new(c.x, ly + i as i32 * ROW, c.w, ROW);
                    let sub = match e.numbers.len() {
                        0 => String::new(),
                        1 => e.numbers[0].clone(),
                        n => format!("{} (+{} more)", e.numbers[0], n - 1),
                    };
                    let t = PhoneTarget::Contact(i);
                    p.list_row(
                        r,
                        Some(Icon::Person),
                        &e.name,
                        Some(&sub),
                        &Accessory::Chevron,
                        pr(t),
                        false,
                    );
                    hits.push((r, pt(t)));
                }
            }
        }
        PhoneView::Messages => match &m.thread {
            None => render_conversations(p, frame, v, y0, &mut hits),
            Some(num) => render_thread(p, frame, v, y0, num, &mut hits),
        },
    }
    hits
}
