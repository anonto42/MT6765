//! System keyboard layout and rendering for [`Painter`] (used by terminal/browser apps).
use super::Painter;
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

/// A key of the shell's system keyboard (the on-screen keyboard for apps: terminal, browser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysKey {
    Char(char),
    Esc,
    Tab,
    /// Sticky one-shot Control.
    Ctrl,
    /// Sticky one-shot Alt.
    Alt,
    /// The paste shortcut (Ctrl+V; Ctrl+Shift+V in a terminal).
    Paste,
    /// Sticky one-shot Shift (letters are drawn upper case while it is on).
    Shift,
    /// Letters <-> symbols.
    Layer,
    Backspace,
    Enter,
    Space,
    Up,
    Down,
    Left,
    Right,
    /// Minimise (hide) the keyboard.
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysKeyBtn {
    pub rect: Rect,
    pub key: SysKey,
}

/// What the system keyboard shows: the layer and the latched one-shot modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SysKbdState {
    pub symbols: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// Rows are measured in half key units; every row is 24 half units (12 keys) wide, so the arrow
/// keys line up as an inverted T at the right edge.
const SYS_HALF_UNITS: i32 = 24;
pub const SYS_ROWS: i32 = 5;

fn sys_rows(symbols: bool) -> [Vec<(SysKey, i32)>; 5] {
    use SysKey::*;
    let chars = |s: &str| s.chars().map(|c| (Char(c), 2)).collect::<Vec<_>>();
    let mut r0 = vec![(Esc, 2), (Tab, 2)];
    r0.extend(chars("1234567890"));
    let mut r1 = chars(if symbols { "!@#$%^&*()" } else { "qwertyuiop" });
    r1.push((Backspace, 4));
    let mut r2 = vec![(Ctrl, 3)];
    r2.extend(chars(if symbols { "-_=+[]{}|" } else { "asdfghjkl" }));
    r2.push((Enter, 3));
    let r3 = if symbols {
        let mut r = chars(r#"<>`~\;:'","#);
        r.extend([(Up, 2), (Char('?'), 2)]);
        r
    } else {
        let mut r = vec![(Shift, 4)];
        r.extend(chars("zxcvbnm."));
        r.extend([(Up, 2), (Char('/'), 2)]);
        r
    };
    let r4 = vec![
        (Layer, 3),
        (Hide, 3),
        (Alt, 3),
        (Space, 6),
        (Paste, 3),
        (Left, 2),
        (Down, 2),
        (Right, 2),
    ];
    [r0, r1, r2, r3, r4]
}

/// Padding and gap for a keyboard of height `h` (the landscape keyboard is compact).
fn sys_spacing(h: i32) -> (i32, i32) {
    if h >= 300 {
        (SP_2, 6)
    } else {
        (6, 4)
    }
}

/// Key rectangles for the system keyboard filling `area` (5 rows, full width).
pub fn system_keyboard_layout(area: Rect, symbols: bool) -> Vec<SysKeyBtn> {
    let (pad, gap) = sys_spacing(area.h);
    let key_h = (area.h - 2 * pad - (SYS_ROWS - 1) * gap) / SYS_ROWS;
    // the leftover pixels go above the first row, so the bottom row sits `pad` from the edge
    let top = area.bottom() - pad - SYS_ROWS * key_h - (SYS_ROWS - 1) * gap;
    let span = area.w - 2 * pad + gap;
    let x_at = |hu: i32| area.x + pad + hu * span / SYS_HALF_UNITS;
    let mut out = Vec::new();
    for (ri, row) in sys_rows(symbols).iter().enumerate() {
        let y = top + ri as i32 * (key_h + gap);
        let mut hu = 0;
        for (key, units) in row {
            let (x0, x1) = (x_at(hu), x_at(hu + units));
            out.push(SysKeyBtn {
                rect: Rect::new(x0, y, x1 - x0 - gap, key_h),
                key: *key,
            });
            hu += units;
        }
    }
    out
}

/// The character a `Char` key types in this state (letters follow Shift).
pub fn sys_key_char(c: char, st: SysKbdState) -> char {
    if st.shift && c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c
    }
}

impl Painter<'_> {
    /// Draws the system keyboard over the whole canvas and returns the key hit rects.
    /// `pressed` = index into the returned vec.
    pub fn system_keyboard(&mut self, st: SysKbdState, pressed: Option<usize>) -> Vec<SysKeyBtn> {
        let area = self.cv.bounds();
        // its own window over other apps (no compositor, nothing to blend with): the strip is
        // SURFACE_STRONG over BG flattened into one opaque fill; the keys blend over it
        self.cv.rect(area, surface_strong().over(bg()));
        self.divider(area.x, area.y, area.w);
        let keys = system_keyboard_layout(area, st.symbols);
        for (i, k) in keys.iter().enumerate() {
            self.draw_system_key(k, st, pressed == Some(i), area);
        }
        keys
    }

    /// Redraws a single key in place on the system keyboard canvas (fast path for key press/release).
    pub fn system_keyboard_key(&mut self, k: &SysKeyBtn, st: SysKbdState, down: bool) {
        let area = self.cv.bounds();
        self.cv.rect(k.rect, surface_strong().over(bg()));
        self.draw_system_key(k, st, down, area);
    }

    fn draw_system_key(&mut self, k: &SysKeyBtn, st: SysKbdState, down: bool, area: Rect) {
        let compact = area.h < 300;
        let char_px = if compact { 20.0 } else { 24.0 };
        let label_px = if compact { SIZE_SECONDARY } else { SIZE_BODY };
        let latched = match k.key {
            SysKey::Shift => st.shift,
            SysKey::Ctrl => st.ctrl,
            SysKey::Alt => st.alt,
            _ => false,
        };
        // surface fill; pressed: + white 12 %; latched modifier: stronger fill + 2 px indicator bar
        self.outline_box(k.rect, 0, down || latched, latched, false);
        let c = k.rect;
        let label = |s: &'static str| (s, Face::Bold, label_px);
        let text: Option<(&str, Face, f32)> = match k.key {
            SysKey::Char(_) | SysKey::Shift | SysKey::Backspace | SysKey::Hide => None,
            SysKey::Esc => Some(label("Esc")),
            SysKey::Tab => Some(label("Tab")),
            SysKey::Ctrl => Some(label("Ctrl")),
            SysKey::Alt => Some(label("Alt")),
            SysKey::Paste => Some(label("Paste")),
            SysKey::Layer => Some(label(if st.symbols { "ABC" } else { "?123" })),
            SysKey::Enter => Some(label("Enter")),
            SysKey::Space => Some(("space", Face::Regular, SIZE_SECONDARY)),
            SysKey::Up => Some(("\u{2191}", Face::Regular, char_px)),
            SysKey::Down => Some(("\u{2193}", Face::Regular, char_px)),
            SysKey::Left => Some(("\u{2190}", Face::Regular, char_px)),
            SysKey::Right => Some(("\u{2192}", Face::Regular, char_px)),
        };
        let icon_at = |w: i32| (c.x + (c.w - w) / 2, c.y + (c.h - ICON) / 2);
        match (k.key, text) {
            (SysKey::Char(ch), _) => self.text(
                &sys_key_char(ch, st).to_string(),
                Style::new(Face::Regular, char_px, crate::tokens::text()),
                c,
                Align::Center,
            ),
            (SysKey::Shift, _) => {
                let (x, y) = icon_at(ICON);
                self.cv.icon(Icon::Shift, x, y, ICON, crate::tokens::text());
            }
            (SysKey::Backspace, _) => {
                let (x, y) = icon_at(ICON);
                self.cv
                    .icon(Icon::Backspace, x, y, ICON, crate::tokens::text());
            }
            (SysKey::Hide, _) => {
                // keyboard with a chevron down: "put the keyboard away"
                let (x, y) = icon_at(ICON + 2 + 16);
                self.cv
                    .icon(Icon::Keyboard, x, y, ICON, crate::tokens::text());
                self.cv.icon(
                    Icon::ChevronDown,
                    x + ICON + 2,
                    y + 4,
                    16,
                    crate::tokens::text(),
                );
            }
            (_, Some((s, face, px))) => {
                let colour = if k.key == SysKey::Space {
                    text_secondary()
                } else {
                    crate::tokens::text()
                };
                self.text(s, Style::new(face, px, colour), c, Align::Center)
            }
            (_, None) => {}
        }
    }
}
