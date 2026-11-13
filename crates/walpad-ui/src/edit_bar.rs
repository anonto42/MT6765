//! The selection toolbar (Cut, Copy, Paste, Select all, Undo) and the clipboard list, the two
//! small override-redirect windows the shell shows over apps (owner request 2026-11-10: "I'm
//! unable to copy and paste anything"). Like the keyboard and the mouse pad they sit over other
//! apps with no compositor, so the strip colour is `SURFACE_STRONG` flattened over BG (opaque).
use crate::canvas::Rect;
use crate::components::Painter;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

/// A button of the selection toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditBtn {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
}

impl EditBtn {
    pub fn label(self) -> &'static str {
        match self {
            EditBtn::Cut => "Cut",
            EditBtn::Copy => "Copy",
            EditBtn::Paste => "Paste",
            EditBtn::SelectAll => "Select all",
            EditBtn::Undo => "Undo",
        }
    }
    /// Owner report 2026-11-13, "copy paste functions are not profashonalize like normal
    /// android devices": the toolbar draws this instead of `label()` now (Android's own
    /// selection toolbar is icon-only); `label()` stays for the accessibility/test name.
    pub fn icon(self) -> Icon {
        match self {
            EditBtn::Cut => Icon::Cut,
            EditBtn::Copy => Icon::Copy,
            EditBtn::Paste => Icon::Clipboard,
            EditBtn::SelectAll => Icon::SelectAll,
            EditBtn::Undo => Icon::Undo,
        }
    }
}

/// Apps with a text field: all five actions (Ctrl+X/C/V/A/Z).
pub const TEXT_BUTTONS: [EditBtn; 5] = [
    EditBtn::Cut,
    EditBtn::Copy,
    EditBtn::Paste,
    EditBtn::SelectAll,
    EditBtn::Undo,
];
/// Terminals: Ctrl+C is an interrupt and Ctrl+X / Ctrl+A / Ctrl+Z are shell keys, so only the
/// two safe actions are offered.
pub const TERMINAL_BUTTONS: [EditBtn; 2] = [EditBtn::Copy, EditBtn::Paste];

pub const BAR_H: i32 = 52;
pub const BAR_BTN_W: i32 = 96;
/// Gap between the toolbar and the keyboard / pad / screen edge below it.
pub const BAR_GAP: i32 = 8;
/// Left margin of the toolbar and of the clipboard list.
pub const MARGIN: i32 = 8;

pub fn bar_size(n: usize) -> (i32, i32) {
    (n as i32 * BAR_BTN_W, BAR_H)
}

/// Button rectangles, window-local.
pub fn bar_layout(buttons: &[EditBtn]) -> Vec<(Rect, EditBtn)> {
    buttons
        .iter()
        .enumerate()
        .map(|(i, b)| (Rect::new(i as i32 * BAR_BTN_W, 0, BAR_BTN_W, BAR_H), *b))
        .collect()
}

/// What a tap in the clipboard list hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipHit {
    /// History entry (0 = newest).
    Entry(usize),
    Clear,
    Close,
}

pub const CLIP_W: i32 = 520;
pub const CLIP_HEAD_H: i32 = 52;
pub const CLIP_ROW_H: i32 = 56;
/// Rows of the empty state's message.
const CLIP_EMPTY_H: i32 = 72;

/// Height of the list window for `entries` entries.
pub fn clip_height(entries: usize) -> i32 {
    CLIP_HEAD_H
        + if entries == 0 {
            CLIP_EMPTY_H
        } else {
            entries as i32 * CLIP_ROW_H
        }
}

/// Hit rectangles of the clipboard list, window-local.
pub fn clip_layout(w: i32, entries: usize) -> Vec<(Rect, ClipHit)> {
    let mut out = vec![
        (Rect::new(w - 2 * 56, 0, 56, CLIP_HEAD_H), ClipHit::Clear),
        (Rect::new(w - 56, 0, 56, CLIP_HEAD_H), ClipHit::Close),
    ];
    for i in 0..entries {
        out.push((
            Rect::new(0, CLIP_HEAD_H + i as i32 * CLIP_ROW_H, w, CLIP_ROW_H),
            ClipHit::Entry(i),
        ));
    }
    out
}

/// One line of text for a history entry: whitespace runs collapsed, newlines shown as a space.
pub fn preview(s: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for ch in s.chars().take(200) {
        if ch.is_whitespace() || ch.is_control() {
            gap = !out.is_empty();
        } else {
            if gap {
                out.push(' ');
                gap = false;
            }
            out.push(ch);
        }
    }
    out
}

/// The strip colour of both windows.
pub fn strip_color() -> crate::tokens::Color {
    surface_strong().over(bg())
}

impl Painter<'_> {
    /// Draws the toolbar over the whole canvas; `pressed` = index into `buttons`.
    pub fn edit_bar(&mut self, buttons: &[EditBtn], pressed: Option<usize>) {
        let area = self.cv.bounds();
        self.cv.rect(area, strip_color());
        for (i, (r, b)) in bar_layout(buttons).into_iter().enumerate() {
            if pressed == Some(i) {
                self.cv.rect(r, pressed_tint());
            }
            if i > 0 {
                self.cv.rect(Rect::new(r.x, 0, 1, r.h), divider());
            }
            // owner report 2026-11-13, "copy paste functions are not profashonalize like
            // normal android devices": an icon (like Android's own selection toolbar) with a
            // small caption under it, not a bare word filling the whole button.
            const ICON: i32 = 22;
            self.cv
                .icon(b.icon(), r.x + (r.w - ICON) / 2, r.y + 5, ICON, text());
            self.text(
                b.label(),
                Style::new(Face::Regular, SIZE_TINY, text()),
                Rect::new(r.x, r.y + 5 + ICON, r.w, r.h - 5 - ICON),
                Align::Center,
            );
        }
        self.divider(0, 0, area.w);
    }

    /// Draws the clipboard list over the whole canvas. Returns the hit rectangles.
    pub fn clipboard_list(
        &mut self,
        entries: &[String],
        pressed: Option<ClipHit>,
    ) -> Vec<(Rect, ClipHit)> {
        let area = self.cv.bounds();
        self.cv.rect(area, strip_color());
        let hits = clip_layout(area.w, entries.len());
        self.text(
            "Clipboard",
            Style::new(Face::Bold, SIZE_BODY, text()),
            Rect::new(16, 0, area.w - 140, CLIP_HEAD_H),
            Align::Left,
        );
        for (r, hit) in &hits {
            if pressed == Some(*hit) {
                self.cv.rect(*r, pressed_tint());
            }
            match hit {
                ClipHit::Clear => self.text(
                    "Clear",
                    Style::new(Face::Bold, SIZE_SECONDARY, text_secondary()),
                    *r,
                    Align::Center,
                ),
                ClipHit::Close => {
                    let s = 20;
                    self.cv.icon(
                        Icon::Close,
                        r.x + (r.w - s) / 2,
                        r.y + (r.h - s) / 2,
                        s,
                        text(),
                    )
                }
                ClipHit::Entry(i) => {
                    self.cv.rect(Rect::new(0, r.y, area.w, 1), divider());
                    let line = self.fonts.fit(
                        &preview(&entries[*i]),
                        Face::Regular,
                        SIZE_BODY,
                        (area.w - 32) as f32,
                    );
                    self.text(
                        &line,
                        Style::new(Face::Regular, SIZE_BODY, text()),
                        Rect::new(16, r.y, area.w - 32, r.h),
                        Align::Left,
                    );
                }
            }
        }
        if entries.is_empty() {
            self.cv
                .rect(Rect::new(0, CLIP_HEAD_H, area.w, 1), divider());
            self.text(
                "Nothing copied yet",
                Style::new(Face::Regular, SIZE_BODY, text_secondary()),
                Rect::new(16, CLIP_HEAD_H, area.w - 32, CLIP_EMPTY_H),
                Align::Center,
            );
        }
        self.divider(0, 0, area.w);
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_layout_is_a_row_of_equal_buttons() {
        let l = bar_layout(&TEXT_BUTTONS);
        assert_eq!(l.len(), 5);
        assert_eq!(bar_size(5), (480, 52));
        assert!(l.iter().all(|(r, _)| r.w == BAR_BTN_W && r.h == BAR_H));
        assert_eq!(l[4].0.right(), 480);
        assert_eq!(bar_layout(&TERMINAL_BUTTONS).len(), 2);
        const { assert!(BAR_H >= 48) };
    }

    #[test]
    fn preview_is_one_short_line() {
        assert_eq!(preview("  hello \n\n  world\t!  "), "hello world !");
        assert_eq!(preview(""), "");
        assert_eq!(preview("a\u{0}b"), "a b");
        assert!(preview(&"x".repeat(1000)).chars().count() <= 200);
    }

    #[test]
    fn clip_layout_rows_and_header_buttons() {
        let l = clip_layout(CLIP_W, 3);
        assert_eq!(l.len(), 5);
        assert_eq!(clip_height(3), CLIP_HEAD_H + 3 * CLIP_ROW_H);
        assert_eq!(clip_height(0), CLIP_HEAD_H + 72);
        let entry = |i| l.iter().find(|(_, h)| *h == ClipHit::Entry(i)).unwrap().0;
        assert_eq!(entry(0).y, CLIP_HEAD_H);
        assert_eq!(entry(2).bottom(), clip_height(3));
        let close = l.iter().find(|(_, h)| *h == ClipHit::Close).unwrap().0;
        assert_eq!(close.right(), CLIP_W);
        assert!(close.w >= 48 && close.h >= 48);
    }
}
