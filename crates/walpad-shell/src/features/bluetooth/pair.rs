use super::model::PairState;
use crate::app::Target;
use walpad_ui::components::{ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Rect, Style};

/// Geometry of the 560 x 260 dialog (same as `Painter::dialog`): (dialog, buttons).
fn dialog_rect(screen: Rect) -> Rect {
    let w = 560.min(screen.w - 2 * SP_5);
    let h = 260;
    Rect::new(
        screen.x + (screen.w - w) / 2,
        screen.y + (screen.h - h) / 2,
        w,
        h,
    )
}

pub fn render_pair(
    p: &mut Painter,
    frame: &Frame,
    name: &str,
    state: &PairState,
    spinner_deg: u16,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let r = dialog_rect(screen);
    p.popover(r);
    let title = match state {
        PairState::Waiting => "Pairing".to_string(),
        PairState::Confirm(_) => format!("Pair with {name}?"),
        PairState::Display(_) => format!("Type this on {name}"),
        PairState::Failed(_) => "Pairing failed".to_string(),
    };
    p.text(
        &title,
        Style::new(Face::Regular, SIZE_TITLE, text()),
        Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
        Align::Left,
    );
    let body_r = Rect::new(r.x + SP_5, r.y + 68, r.w - 2 * SP_5, 28);
    let small = |p: &mut Painter, s: &str, c: Color, rr: Rect| {
        p.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, c),
            rr,
            Align::Left,
        )
    };
    let big = |p: &mut Painter, s: &str| {
        p.text(
            s,
            Style::new(Face::Mono, 36.0, text()),
            Rect::new(r.x + SP_5, r.y + 104, r.w - 2 * SP_5, 48),
            Align::Center,
        )
    };
    match state {
        PairState::Waiting => {
            small(
                p,
                &format!("Connecting to {name}..."),
                text_secondary(),
                body_r,
            );
            p.spinner(r.x + r.w / 2, r.y + 130, 32, spinner_deg as f32);
        }
        PairState::Confirm(code) => {
            small(
                p,
                "Does the other device show this number?",
                text_secondary(),
                body_r,
            );
            big(p, code);
        }
        PairState::Display(code) => {
            small(
                p,
                "then press Enter. Pairing finishes by itself.",
                text_secondary(),
                body_r,
            );
            big(p, code);
        }
        PairState::Failed(why) => small(
            p,
            why,
            danger(),
            Rect::new(body_r.x, body_r.y, body_r.w, 56),
        ),
    }
    let by = r.bottom() - SP_5 - BUTTON_H;
    let mut hits = Vec::new();
    match state {
        PairState::Confirm(_) => {
            let bw = (r.w - 2 * SP_5 - SP_4) / 2;
            let no = Rect::new(r.x + SP_5, by, bw, BUTTON_H);
            let yes = Rect::new(no.right() + SP_4, by, bw, BUTTON_H);
            p.button(
                no,
                "Don't pair",
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            );
            p.button(yes, "Pair", ButtonKind::Primary, ButtonState::Default, None);
            hits.push((no, Target::BtPairReject));
            hits.push((yes, Target::BtPairAccept));
        }
        other => {
            let b = Rect::new(r.x + SP_5, by, r.w - 2 * SP_5, BUTTON_H);
            let label = match other {
                PairState::Failed(_) => "OK",
                PairState::Waiting => "Close",
                _ => "Cancel",
            };
            p.button(b, label, ButtonKind::Secondary, ButtonState::Default, None);
            hits.push((b, Target::SheetCancel));
        }
    }
    hits
}

pub fn render_forget(p: &mut Painter, frame: &Frame, name: &str) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (cancel, confirm) = p.dialog(
        screen,
        "Forget device?",
        &format!("{name} will be unpaired. You can pair it again later."),
        "Cancel",
        "Forget",
        "Forget",
        true,
        false,
    );
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}
