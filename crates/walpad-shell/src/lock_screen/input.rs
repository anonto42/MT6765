//! Touch, pointer-drag pattern, and hardware keyboard input handling for `LockScreen`.

use super::{layout, Hit, LockScreen, Out, DOT_CAPTURE, PATTERN_MIN, PIN_MAX, PIN_MIN, PW_MAX};
use walpad_proto::LockKind;
use walpad_ui::components::{keyboard_layout, Key, KeyboardLayer};

impl LockScreen {
    fn hit_at(&self, x: i32, y: i32) -> Option<Hit> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, h)| *h)
    }

    fn nearest_dot(&self, x: i32, y: i32) -> Option<u8> {
        let l = layout(self.area, self.kind);
        l.dots.iter().enumerate().find_map(|(i, (cx, cy))| {
            let (dx, dy) = (x - cx, y - cy);
            (dx * dx + dy * dy <= DOT_CAPTURE * DOT_CAPTURE).then_some(i as u8)
        })
    }

    /// Add dot `d` (and the dot it jumps over, like Android does).
    fn add_dot(&mut self, d: u8) {
        if self.pattern.contains(&d) {
            return;
        }
        if let Some(&a) = self.pattern.last() {
            let (ar, ac, br, bc) = (a / 3, a % 3, d / 3, d % 3);
            if (ar + br) % 2 == 0 && (ac + bc) % 2 == 0 {
                let mid = (ar + br) / 2 * 3 + (ac + bc) / 2;
                if mid != a && mid != d && !self.pattern.contains(&mid) {
                    self.pattern.push(mid);
                }
            }
        }
        self.pattern.push(d);
    }

    pub fn down(&mut self, x: i32, y: i32) -> Out {
        if self.locked_out() {
            return Out::None;
        }
        if self.kind == LockKind::Pattern {
            self.pattern.clear();
            self.error = None;
            self.finger = Some((x, y));
            self.drawing = true;
            if let Some(d) = self.nearest_dot(x, y) {
                self.add_dot(d);
            }
            return Out::Redraw;
        }
        self.pressed = self.hit_at(x, y);
        Out::Redraw
    }

    pub fn moved(&mut self, x: i32, y: i32) -> Out {
        if self.kind == LockKind::Pattern && self.drawing {
            self.finger = Some((x, y));
            if let Some(d) = self.nearest_dot(x, y) {
                self.add_dot(d);
            }
            return Out::Redraw;
        }
        Out::None
    }

    pub fn up(&mut self, x: i32, y: i32) -> Out {
        if self.kind == LockKind::Pattern {
            if !self.drawing {
                return Out::None;
            }
            self.drawing = false;
            self.finger = None;
            if self.pattern.len() >= PATTERN_MIN {
                let s: String = self.pattern.iter().map(|d| (b'0' + d) as char).collect();
                self.wipe();
                self.busy = true;
                return Out::Submit(s);
            }
            self.pattern.clear();
            self.error = Some("Connect at least 4 dots".into());
            return Out::Redraw;
        }
        let down = self.pressed.take();
        let hit = self.hit_at(x, y);
        match (down, hit) {
            (Some(d), Some(h)) if d == h && !self.locked_out() => self.activate(h),
            _ => Out::Redraw,
        }
    }

    fn activate(&mut self, h: Hit) -> Out {
        match h {
            Hit::Digit(c) => {
                if self.entry.len() < PIN_MAX {
                    self.entry.push(c);
                    self.error = None;
                }
                Out::Redraw
            }
            Hit::Backspace => {
                self.entry.pop();
                self.error = None;
                Out::Redraw
            }
            Hit::Eye => {
                self.shown = !self.shown;
                Out::Redraw
            }
            Hit::Submit => self.submit(),
            Hit::Key(i) => {
                let Some(k) = keyboard_layout(layout(self.area, self.kind).keyboard, self.layer)
                    .get(i)
                    .map(|b| b.key)
                else {
                    return Out::None;
                };
                self.on_keyboard_key(k)
            }
        }
    }

    fn on_keyboard_key(&mut self, key: Key) -> Out {
        match key {
            Key::Char(c) => {
                if self.entry.len() + c.len_utf8() <= PW_MAX {
                    self.entry.push(c);
                    self.error = None;
                }
                if self.layer == KeyboardLayer::Upper {
                    self.layer = KeyboardLayer::Lower;
                }
                Out::Redraw
            }
            Key::Space => {
                if self.entry.len() < PW_MAX {
                    self.entry.push(' ');
                    self.error = None;
                }
                Out::Redraw
            }
            Key::Backspace => {
                self.entry.pop();
                self.error = None;
                Out::Redraw
            }
            Key::Shift => {
                self.layer = if self.layer == KeyboardLayer::Upper {
                    KeyboardLayer::Lower
                } else {
                    KeyboardLayer::Upper
                };
                Out::Redraw
            }
            Key::Layer => {
                self.layer = if self.layer == KeyboardLayer::Symbols {
                    KeyboardLayer::Lower
                } else {
                    KeyboardLayer::Symbols
                };
                Out::Redraw
            }
            Key::Enter => self.submit(),
        }
    }

    /// A key from a hardware keyboard (grabbed by the lock window): printable ASCII, BackSpace,
    /// Return.
    pub fn keysym(&mut self, sym: u32) -> Out {
        if self.locked_out() || self.kind == LockKind::Pattern {
            return Out::None;
        }
        match sym {
            0xff0d | 0xff8d => self.submit(),
            0xff08 => self.activate(Hit::Backspace),
            0x20..=0x7e => {
                let c = char::from(sym as u8);
                if self.kind == LockKind::Pin && !c.is_ascii_digit() {
                    return Out::None;
                }
                if self.entry.len()
                    < if self.kind == LockKind::Pin {
                        PIN_MAX
                    } else {
                        PW_MAX
                    }
                {
                    self.entry.push(c);
                    self.error = None;
                }
                Out::Redraw
            }
            _ => Out::None,
        }
    }

    fn submit(&mut self) -> Out {
        let min = if self.kind == LockKind::Pin {
            PIN_MIN
        } else {
            1
        };
        if self.entry.chars().count() < min {
            self.error = Some(format!("The PIN has at least {PIN_MIN} digits"));
            return Out::Redraw;
        }
        let s = std::mem::take(&mut self.entry);
        self.wipe();
        self.busy = true;
        Out::Submit(s)
    }
}
