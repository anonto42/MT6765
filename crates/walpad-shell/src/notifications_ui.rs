//! Notification centre screen (owner request 2026-11-12, change `device-notifications`). Kept
//! intentionally minimal per the owner's "fast as possible" instruction: a plain list, newest
//! first (the core already answers `notifications` that way), tap a row to mark it read, one
//! Clear all button. No day-grouping, no swipe-to-clear. Follows the list pattern in
//! `phone_ui.rs` (recents/messages) and the translucent surface tokens the rest of the shell uses.
use crate::app::Target;
use walpad_proto::Notification;
use walpad_ui::components::{Accessory, ButtonKind, ButtonState, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Rect, Style};

const TITLE_H: i32 = 40;
const CLEAR_H: i32 = BUTTON_H;
const ROW_H: i32 = 76;
const EMPTY_H: i32 = 96;

#[derive(Debug, Clone, PartialEq)]
pub struct NotifLayout {
    pub title: Rect,
    pub clear_all: Option<Rect>,
    pub rows: Vec<Rect>,
    pub empty: Option<Rect>,
    pub total: i32,
}

pub fn layout(frame: &Frame, count: usize) -> NotifLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let title = Rect::new(c.x, y, c.w, TITLE_H);
    y += TITLE_H + SP_3;
    let clear_all = (count > 0).then(|| {
        let r = Rect::new(c.x, y, c.w, CLEAR_H);
        y += CLEAR_H + SP_4;
        r
    });
    let mut rows = Vec::new();
    let mut empty = None;
    if count == 0 {
        empty = Some(Rect::new(c.x, y, c.w, EMPTY_H));
        y += EMPTY_H;
    } else {
        for _ in 0..count {
            rows.push(Rect::new(c.x, y, c.w, ROW_H));
            y += ROW_H;
        }
    }
    NotifLayout {
        title,
        clear_all,
        rows,
        empty,
        total: y + SP_6,
    }
}

pub struct View<'a> {
    pub items: &'a [Notification],
    pub pressed: Option<Target>,
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let l = layout(frame, v.items.len());
    let mut hits = Vec::new();
    p.screen_title("Notifications", l.title);
    if let Some(r) = l.clear_all {
        let state = if v.pressed == Some(Target::NotificationsClearAll) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(r, "Clear all", ButtonKind::Secondary, state, None);
        hits.push((r, Target::NotificationsClearAll));
    }
    if let Some(r) = l.empty {
        p.text(
            "No notifications yet",
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
            r,
            Align::Center,
        );
    }
    for (i, (row, n)) in l.rows.iter().zip(v.items.iter()).enumerate() {
        let pressed = v.pressed == Some(Target::NotificationRow(i));
        p.list_row(
            *row,
            None,
            &n.title,
            Some(&n.body),
            &Accessory::None,
            pressed,
            n.read,
        );
        hits.push((*row, Target::NotificationRow(i)));
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use walpad_proto::NotificationCategory;
    use walpad_ui::Fonts;

    fn fonts() -> Fonts {
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap()
    }

    fn n(id: u64, title: &str, read: bool) -> Notification {
        Notification {
            id,
            ts_ms: 1000,
            category: NotificationCategory::Charging,
            title: title.into(),
            body: "body".into(),
            read,
        }
    }

    #[test]
    fn empty_list_has_no_rows_or_clear_button() {
        let f = Frame::new(800, 1280);
        let l = layout(&f, 0);
        assert!(l.rows.is_empty() && l.clear_all.is_none() && l.empty.is_some());
    }

    #[test]
    fn rows_stack_newest_first_and_clear_all_appears() {
        let f = Frame::new(800, 1280);
        let l = layout(&f, 3);
        assert_eq!(l.rows.len(), 3);
        assert!(l.clear_all.is_some() && l.empty.is_none());
        // rows do not overlap, top to bottom
        for w in l.rows.windows(2) {
            assert!(w[0].bottom() <= w[1].y);
        }
        assert!(l.total > l.rows.last().unwrap().bottom());
    }

    #[test]
    fn render_returns_one_hit_per_row_plus_clear_all() {
        let f = Frame::new(800, 1280);
        let mut fonts = fonts();
        let mut cv = walpad_ui::Canvas::new(f.w as u32, f.h as u32, BG).unwrap();
        let mut p = Painter {
            cv: &mut cv,
            fonts: &mut fonts,
        };
        let items = vec![
            n(2, "Charger connected", false),
            n(1, "USB device connected", true),
        ];
        let hits = render(
            &mut p,
            &f,
            &View {
                items: &items,
                pressed: None,
            },
        );
        assert_eq!(hits.len(), 3); // clear all + 2 rows
        assert!(hits.contains(&(hits[0].0, Target::NotificationsClearAll)));
        assert!(hits.iter().any(|(_, t)| *t == Target::NotificationRow(0)));
        assert!(hits.iter().any(|(_, t)| *t == Target::NotificationRow(1)));
    }
}
