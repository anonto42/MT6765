//! On-screen keyboard layout and rendering for [`Painter`] (text fields / dialogs).
use super::Painter;
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardLayer {
    Lower,
    Upper,
    Symbols,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Shift,
    Backspace,
    /// Switch between letters and symbols.
    Layer,
    Space,
    Enter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBtn {
    pub rect: Rect,
    pub key: Key,
}

const KEY_W: i32 = 72;
const KEY_H: i32 = 64;
const KEY_GAP: i32 = 6;

/// Key layout: rows of up to 10 keys of 72x64 with 6 px gaps, plus shift, 123, space, backspace.
pub fn keyboard_layout(area: Rect, layer: KeyboardLayer) -> Vec<KeyBtn> {
    let rows: [&str; 3] = match layer {
        KeyboardLayer::Lower => ["qwertyuiop", "asdfghjkl", "zxcvbnm"],
        KeyboardLayer::Upper => ["QWERTYUIOP", "ASDFGHJKL", "ZXCVBNM"],
        KeyboardLayer::Symbols => ["1234567890", "-/:;()$&@\"", ".,?!'#%"],
    };
    let mut out = Vec::new();
    let unit = KEY_W + KEY_GAP;
    let row_w = |n: i32| n * unit - KEY_GAP;
    let top = area.y;
    let row_y = |i: i32| top + i * (KEY_H + KEY_GAP);
    for (ri, row) in rows.iter().enumerate() {
        let n = row.chars().count() as i32;
        let special = ri == 2;
        let total = row_w(n) + if special { 2 * (unit + KEY_W / 2) } else { 0 };
        let mut x = area.x + (area.w - total) / 2;
        if special {
            let w = KEY_W + KEY_W / 2;
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), w, KEY_H),
                key: if layer == KeyboardLayer::Symbols {
                    Key::Char('=')
                } else {
                    Key::Shift
                },
            });
            x += w + KEY_GAP;
        }
        for ch in row.chars() {
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), KEY_W, KEY_H),
                key: Key::Char(ch),
            });
            x += unit;
        }
        if special {
            out.push(KeyBtn {
                rect: Rect::new(x, row_y(ri as i32), KEY_W + KEY_W / 2, KEY_H),
                key: Key::Backspace,
            });
        }
    }
    // bottom row: 123 / ABC, space, enter
    let y = row_y(3);
    let total = row_w(10);
    let x0 = area.x + (area.w - total) / 2;
    let side = 2 * unit - KEY_GAP;
    out.push(KeyBtn {
        rect: Rect::new(x0, y, side, KEY_H),
        key: Key::Layer,
    });
    out.push(KeyBtn {
        rect: Rect::new(
            x0 + side + KEY_GAP,
            y,
            total - 2 * side - 2 * KEY_GAP,
            KEY_H,
        ),
        key: Key::Space,
    });
    out.push(KeyBtn {
        rect: Rect::new(x0 + total - side, y, side, KEY_H),
        key: Key::Enter,
    });
    out
}

impl Painter<'_> {
    /// Draws the keyboard and returns the key hit rects. `pressed` = index into the returned vec.
    pub fn keyboard(
        &mut self,
        area: Rect,
        layer: KeyboardLayer,
        pressed: Option<usize>,
    ) -> Vec<KeyBtn> {
        let keys = keyboard_layout(area, layer);
        for (i, k) in keys.iter().enumerate() {
            let special = !matches!(k.key, Key::Char(_) | Key::Space);
            let down = pressed == Some(i);
            // a pressed key is filled white: the strongest feedback while typing blind-ish
            if down {
                self.cv.rect(k.rect, text());
            } else if k.key == Key::Shift && layer == KeyboardLayer::Upper {
                // shift engaged: drawn as held
                self.outline_box(k.rect, 0, true, true, false);
            } else {
                self.cv
                    .rect(k.rect, if special { surface() } else { surface_icon() });
            }
            let fg = if down { bg() } else { text() };
            let c = k.rect;
            match k.key {
                Key::Char(ch) => self.text(
                    &ch.to_string(),
                    Style::new(Face::Regular, 24.0, fg),
                    c,
                    Align::Center,
                ),
                Key::Shift => self.cv.icon(
                    Icon::Shift,
                    c.x + (c.w - ICON) / 2,
                    c.y + (c.h - ICON) / 2,
                    ICON,
                    fg,
                ),
                Key::Backspace => self.cv.icon(
                    Icon::Backspace,
                    c.x + (c.w - ICON) / 2,
                    c.y + (c.h - ICON) / 2,
                    ICON,
                    fg,
                ),
                Key::Layer => self.text(
                    if layer == KeyboardLayer::Symbols {
                        "ABC"
                    } else {
                        "123"
                    },
                    Style::new(Face::Bold, SIZE_BODY, fg),
                    c,
                    Align::Center,
                ),
                Key::Space => self.small(
                    "space",
                    c,
                    Align::Center,
                    if down { bg() } else { text_secondary() },
                ),
                Key::Enter => self.text(
                    "Done",
                    Style::new(Face::Bold, SIZE_BODY, fg),
                    c,
                    Align::Center,
                ),
            }
        }
        keys
    }
}
