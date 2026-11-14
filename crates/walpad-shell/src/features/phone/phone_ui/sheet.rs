//! Modal action sheets, confirmation dialogs, and unread badge for the Phone screen.
use super::render::pt;
use super::{PhoneModel, PhoneSheet, PhoneTarget};
use crate::app::Target;
use walpad_ui::components::{ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

const SHEET_W: i32 = 800;

/// The stacked buttons of a bottom sheet: (target, label, kind, icon).
pub type SheetButtons = Vec<(PhoneTarget, String, ButtonKind, Option<Icon>)>;

pub fn sheet_buttons(sheet: &PhoneSheet, m: &PhoneModel) -> (String, SheetButtons) {
    let mut b: Vec<(PhoneTarget, String, ButtonKind, Option<Icon>)> = Vec::new();
    let title = match sheet {
        PhoneSheet::Contact(id) => {
            let Some(c) = m.contact_by_id(id) else {
                return (String::new(), Vec::new());
            };
            for (i, n) in c.numbers.iter().enumerate() {
                b.push((
                    PhoneTarget::SheetCall(i),
                    format!("Call {n}"),
                    ButtonKind::Primary,
                    Some(Icon::Phone),
                ));
            }
            b.push((
                PhoneTarget::SheetMessage,
                "Message".into(),
                ButtonKind::Secondary,
                Some(Icon::Message),
            ));
            b.push((
                PhoneTarget::SheetEdit,
                "Edit".into(),
                ButtonKind::Secondary,
                None,
            ));
            b.push((
                PhoneTarget::SheetDelete,
                "Delete contact".into(),
                ButtonKind::Danger,
                None,
            ));
            c.name.clone()
        }
        PhoneSheet::Message(id) => {
            let failed = m
                .sms
                .iter()
                .any(|x| x.id == *id && !x.incoming && x.state == "failed");
            if failed {
                b.push((
                    PhoneTarget::SheetResend,
                    "Send again".into(),
                    ButtonKind::Primary,
                    None,
                ));
            }
            b.push((
                PhoneTarget::SheetDelete,
                "Delete message".into(),
                ButtonKind::Danger,
                None,
            ));
            "Message".into()
        }
        PhoneSheet::DeleteContact(_) | PhoneSheet::ClearLog => String::new(),
    };
    b.push((
        PhoneTarget::SheetClose,
        "Cancel".into(),
        ButtonKind::Secondary,
        None,
    ));
    (title, b)
}

/// Geometry of the stacked-button sheet: the sheet rect and the button rects.
pub fn sheet_geom(frame: &Frame, n: usize) -> (Rect, Vec<Rect>) {
    let h = 68 + n as i32 * (BUTTON_H + SP_3) + SP_4;
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new((frame.w - w) / 2, frame.h - h, w, h);
    let mut y = sheet.y + 68;
    let rects = (0..n)
        .map(|_| {
            let r = Rect::new(sheet.x + SP_5, y, sheet.w - 2 * SP_5, BUTTON_H);
            y += BUTTON_H + SP_3;
            r
        })
        .collect();
    (sheet, rects)
}

pub fn render_sheet(
    p: &mut Painter,
    frame: &Frame,
    sheet: &PhoneSheet,
    m: &PhoneModel,
) -> Vec<(Rect, Target)> {
    match sheet {
        PhoneSheet::DeleteContact(id) => {
            let name = m
                .contact_by_id(id)
                .map_or("this contact", |c| c.name.as_str());
            let (cancel, confirm) = p.dialog(
                frame.screen(),
                "Delete contact?",
                &format!("{name} will be removed from your contacts."),
                "Cancel",
                "Delete",
                "Delete",
                true,
                false,
            );
            vec![
                (cancel, pt(PhoneTarget::SheetClose)),
                (confirm, pt(PhoneTarget::SheetConfirm)),
            ]
        }
        PhoneSheet::ClearLog => {
            let (cancel, confirm) = p.dialog(
                frame.screen(),
                "Clear call log?",
                "All recent calls will be removed.",
                "Cancel",
                "Clear",
                "Clear",
                true,
                false,
            );
            vec![
                (cancel, pt(PhoneTarget::SheetClose)),
                (confirm, pt(PhoneTarget::SheetConfirm)),
            ]
        }
        _ => {
            let (title, buttons) = sheet_buttons(sheet, m);
            let (rect, rects) = sheet_geom(frame, buttons.len());
            p.sheet(rect, &title);
            let mut hits = vec![(rect, Target::PanelBody)];
            for ((t, label, kind, icon), r) in buttons.iter().zip(rects) {
                p.button(r, label, *kind, ButtonState::Default, *icon);
                hits.push((r, pt(*t)));
            }
            hits
        }
    }
}

/// A small unread count on the top-right corner of an icon box.
pub fn paint_badge(p: &mut Painter, icon_box: Rect, count: u32) {
    if count == 0 {
        return;
    }
    let text = if count > 9 {
        "9+".to_string()
    } else {
        count.to_string()
    };
    let r = Rect::new(icon_box.right() - 14, icon_box.y - 6, 22, 22);
    p.cv.circle(
        (r.x + 11) as f32,
        (r.y + 11) as f32,
        11.0,
        walpad_ui::tokens::danger(),
    );
    p.text(
        &text,
        Style::new(Face::Bold, SIZE_LABEL, walpad_ui::tokens::bg()),
        r,
        Align::Center,
    );
}
