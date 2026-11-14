use super::layout::{layout, Item};
use super::model::{StoreModel, StoreTarget};
use crate::app::Target;
use walpad_ui::components::{row_runs, ButtonState, Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

fn wrap(p: &mut Painter, s: &str, w: i32, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let t = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || p.fonts.measure(&t, Face::Regular, SIZE_SECONDARY) as i32 <= w {
            cur = t;
        } else {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines.truncate(max);
    lines
}

pub fn render(
    p: &mut Painter,
    frame: &Frame,
    m: &StoreModel,
    online: bool,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, m);
    let mut hits = Vec::new();
    p.outline_box(l.back, 0, pressed == Some(Target::Back), false, false);
    p.cv.icon(
        Icon::Back,
        l.back.x + (l.back.w - ICON) / 2,
        l.back.y + (l.back.h - ICON) / 2,
        ICON,
        text(),
    );
    p.screen_title(m.title(), l.title);
    hits.push((l.back, Target::Back));
    let busy = m.busy.is_some();
    for g in row_runs(
        l.items
            .iter()
            .map(|(r, it)| (*r, matches!(it, Item::Row { .. }))),
    ) {
        p.row_group(g);
    }
    for (r, it) in &l.items {
        match it {
            Item::Note(s, col) => p.text(
                s,
                Style::new(Face::Regular, SIZE_SECONDARY, *col),
                Rect::new(r.x + SP_1, r.y, r.w, r.h),
                Align::Left,
            ),
            Item::Section(s) => p.section_label(s, Rect::new(r.x + SP_1, r.y, r.w, r.h)),
            Item::Error(msg) => {
                let t = StoreTarget::DismissError;
                let pr = pressed == Some(Target::Store(t));
                p.card(*r, pr);
                p.cv.icon(Icon::Warning, r.x + SP_3, r.y + SP_3, ICON, danger());
                p.text(
                    "Could not finish (tap to dismiss)",
                    Style::new(Face::Bold, SIZE_SECONDARY, danger()),
                    Rect::new(
                        r.x + SP_3 + ICON + SP_2,
                        r.y + SP_2,
                        r.w - 2 * SP_4 - ICON,
                        28,
                    ),
                    Align::Left,
                );
                let w = r.w - 2 * SP_3;
                for (i, line) in wrap(p, msg, w, 2).iter().enumerate() {
                    p.text(
                        line,
                        Style::new(Face::Regular, SIZE_SECONDARY, text()),
                        Rect::new(r.x + SP_3, r.y + 34 + i as i32 * 22, w, 22),
                        Align::Left,
                    );
                }
                hits.push((*r, Target::Store(t)));
            }
            Item::Progress {
                title,
                percent,
                text,
            } => {
                p.card(*r, false);
                p.text(
                    title,
                    Style::new(Face::Regular, SIZE_BODY, walpad_ui::tokens::text()),
                    Rect::new(r.x + SP_4, r.y + SP_2, r.w - 2 * SP_4 - 56, 28),
                    Align::Left,
                );
                p.text(
                    &format!("{percent}%"),
                    Style::new(Face::Bold, SIZE_BODY, walpad_ui::tokens::text()),
                    Rect::new(r.right() - SP_4 - 56, r.y + SP_2, 56, 28),
                    Align::Right,
                );
                let bar = Rect::new(r.x + SP_4, r.y + 42, r.w - 2 * SP_4, 8);
                p.cv.rect(bar, divider());
                let fw = (bar.w * *percent as i32 / 100).max(if *percent > 0 { 8 } else { 0 });
                if fw > 0 {
                    p.cv.rect(Rect::new(bar.x, bar.y, fw, bar.h), accent());
                }
                p.text(
                    text,
                    Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
                    Rect::new(r.x + SP_4, r.y + 58, r.w - 2 * SP_4, 24),
                    Align::Left,
                );
            }
            Item::Button(label, t, kind) => {
                let st = if !online {
                    ButtonState::Disabled
                } else if pressed == Some(Target::Store(*t)) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                };
                p.button(*r, label, *kind, st, None);
                hits.push((*r, Target::Store(*t)));
            }
            Item::Row {
                icon,
                title,
                sub,
                acc,
                target,
            } => {
                let t = target.map(Target::Store);
                let disabled = !online
                    || (busy
                        && target.is_some_and(|t| {
                            matches!(t, StoreTarget::Install(_) | StoreTarget::Remove(_))
                        }));
                p.list_row(
                    *r,
                    *icon,
                    title,
                    sub.as_deref(),
                    acc,
                    t.is_some() && pressed == t,
                    disabled,
                );
                if let Some(t) = t {
                    hits.push((*r, t));
                }
            }
        }
    }
    let _ = Tone::Info;
    hits
}
