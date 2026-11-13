//! The lock screen's content (owner request 2026-11-10, OpenSpec `lock-screen`): clock, prompt, a
//! PIN pad, a password field with the shell's on-screen keyboard, or a 3x3 pattern grid. Pure of
//! I/O: geometry, state machine and drawing; `lock_window.rs` owns the X window and the grabs, the
//! shell loop sends the secret to the core (which alone verifies it).
//!
//! The typed secret lives only in `entry` / `pattern` until it is handed over with
//! `Out::Submit` and is wiped at once; it is never printed (no `Debug` on this type).
use crate::app::WallTime;
use walpad_proto::LockKind;
use walpad_ui::components::{
    keyboard_layout, ButtonKind, ButtonState, Key, KeyboardLayer, Painter,
};
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

/// Shortest PIN the core accepts; the pad refuses to submit less.
pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 12;
pub const PATTERN_MIN: usize = 4;
const PW_MAX: usize = 64;
const KEY_W: i32 = 128;
const KEY_H: i32 = 96;
const KEY_GAP: i32 = 16;
const KB_H: i32 = 274;
/// Pointer distance (px) to a pattern dot that captures it.
const DOT_CAPTURE: i32 = 52;

/// What the finger is on (for the pressed look and for hit testing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Digit(char),
    Backspace,
    Submit,
    /// Index into `keyboard_layout` of the current layer.
    Key(usize),
    Eye,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Out {
    None,
    Redraw,
    /// Hand this secret to the core (`unlock`). Already cleared from the screen's state.
    Submit(String),
}

pub struct LockScreen {
    pub kind: LockKind,
    /// The visible area (800x1280 or 1280x800).
    pub area: (i32, i32),
    pub wall: WallTime,
    entry: String,
    pattern: Vec<u8>,
    finger: Option<(i32, i32)>,
    shown: bool,
    layer: KeyboardLayer,
    pressed: Option<Hit>,
    /// A dragged pattern is in progress.
    drawing: bool,
    pub error: Option<String>,
    /// Waiting for the core's answer.
    pub busy: bool,
    /// Attempts are refused until this shell-clock time (ms).
    retry_until: Option<u64>,
    pub now: u64,
    hits: Vec<(Rect, Hit)>,
}

/// Where everything sits for an area and a kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub clock: Rect,
    pub date: Rect,
    pub prompt: Rect,
    /// PIN dots row / password field.
    pub entry: Rect,
    pub pad: Vec<(Rect, Hit)>,
    pub keyboard: Rect,
    pub submit: Rect,
    /// The 3x3 dot centres, row-major.
    pub dots: [(i32, i32); 9],
}

pub fn layout(area: (i32, i32), kind: LockKind) -> Layout {
    let (w, h) = area;
    let landscape = w > h;
    let mut l = Layout {
        clock: Rect::new(0, 0, 0, 0),
        date: Rect::new(0, 0, 0, 0),
        prompt: Rect::new(0, 0, 0, 0),
        entry: Rect::new(0, 0, 0, 0),
        pad: Vec::new(),
        keyboard: Rect::new(0, 0, 0, 0),
        submit: Rect::new(0, 0, 0, 0),
        dots: [(0, 0); 9],
    };
    match kind {
        LockKind::Password => {
            // clock on top, the field above the keyboard, Unlock at the bottom
            let (ct, ch) = if landscape { (16, 76) } else { (96, 100) };
            l.clock = Rect::new(0, ct, w, ch);
            l.date = Rect::new(0, ct + ch, w, 28);
            let kw = 800.min(w);
            l.submit = Rect::new(
                (w - kw) / 2 + SP_5,
                h - 24 - BUTTON_H,
                kw - 2 * SP_5,
                BUTTON_H,
            );
            l.keyboard = Rect::new((w - kw) / 2, l.submit.y - SP_3 - KB_H, kw, KB_H);
            l.entry = Rect::new(
                (w - kw) / 2 + SP_5,
                l.keyboard.y - 34 - 56,
                kw - 2 * SP_5,
                56,
            );
            l.prompt = Rect::new(l.entry.x, l.entry.y - 36, l.entry.w, 28);
        }
        LockKind::Pin | LockKind::Pattern => {
            // portrait: stacked and centred; landscape: clock left, input right
            let (cx_clock, cx_in) = if landscape {
                (w / 4, w * 3 / 4)
            } else {
                (w / 2, w / 2)
            };
            let (ct, ch) = if landscape {
                (h / 2 - 120, 100)
            } else {
                (110, 100)
            };
            let half = if landscape { w / 2 } else { w };
            l.clock = Rect::new(cx_clock - half / 2, ct, half, ch);
            l.date = Rect::new(l.clock.x, ct + ch, half, 28);
            l.prompt = Rect::new(l.clock.x, l.date.bottom() + 36, half, 28);
            if kind == LockKind::Pin {
                let gw = 3 * KEY_W + 2 * KEY_GAP;
                let gh = 4 * KEY_H + 3 * KEY_GAP;
                let x0 = cx_in - gw / 2;
                let y0 = if landscape { (h - gh) / 2 + 32 } else { 470 };
                l.entry = Rect::new(x0, y0 - 64, gw, 32);
                let keys = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
                for (i, k) in keys.iter().enumerate() {
                    let (r, c) = (i as i32 / 3, i as i32 % 3);
                    l.pad.push((
                        Rect::new(
                            x0 + c * (KEY_W + KEY_GAP),
                            y0 + r * (KEY_H + KEY_GAP),
                            KEY_W,
                            KEY_H,
                        ),
                        Hit::Digit(*k),
                    ));
                }
                let y3 = y0 + 3 * (KEY_H + KEY_GAP);
                let at = |c: i32| Rect::new(x0 + c * (KEY_W + KEY_GAP), y3, KEY_W, KEY_H);
                l.pad.push((at(0), Hit::Backspace));
                l.pad.push((at(1), Hit::Digit('0')));
                l.pad.push((at(2), Hit::Submit));
            } else {
                let step = if landscape { 130 } else { 150 };
                let (cy0, cx0) = (if landscape { h / 2 - step } else { 600 }, cx_in - step);
                for i in 0..9 {
                    l.dots[i] = (cx0 + (i as i32 % 3) * step, cy0 + (i as i32 / 3) * step);
                }
                l.entry = Rect::new(cx0 - 60, cy0 - 100, 2 * step + 120, 28);
            }
        }
    }
    l
}

/// Seconds shown for a remaining time (rounded up).
fn secs(ms: u64) -> u64 {
    ms.div_ceil(1000)
}

impl LockScreen {
    pub fn new(kind: LockKind, area: (i32, i32), wall: WallTime) -> LockScreen {
        LockScreen {
            kind,
            area,
            wall,
            entry: String::new(),
            pattern: Vec::new(),
            finger: None,
            shown: false,
            layer: KeyboardLayer::Lower,
            pressed: None,
            drawing: false,
            error: None,
            busy: false,
            retry_until: None,
            now: 0,
            hits: Vec::new(),
        }
    }

    pub fn set_kind(&mut self, kind: LockKind) {
        if self.kind != kind {
            self.kind = kind;
            self.wipe();
            self.error = None;
        }
    }

    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.pressed = None;
    }

    /// Forget whatever was typed.
    pub fn wipe(&mut self) {
        // overwrite before dropping so the secret does not linger in the freed buffer
        let n = self.entry.len();
        self.entry.clear();
        self.entry.extend(std::iter::repeat_n('\0', n));
        self.entry.clear();
        self.pattern.clear();
        self.finger = None;
        self.drawing = false;
        self.pressed = None;
    }

    /// Attempts are refused for `secs` seconds (from `now`).
    pub fn set_retry(&mut self, secs: Option<u32>) {
        self.retry_until = secs.map(|s| self.now + u64::from(s) * 1000);
    }

    pub fn waiting(&self) -> bool {
        self.retry_until.is_some_and(|t| self.now < t)
    }

    /// Advance the clock; returns true when the picture changes (the countdown).
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.countdown();
        self.now = now;
        if self.retry_until.is_some_and(|t| now >= t) {
            self.retry_until = None;
            if self.error.is_some() && before.is_some() {
                self.error = None;
            }
        }
        before != self.countdown()
    }

    fn countdown(&self) -> Option<u64> {
        self.retry_until
            .and_then(|t| t.checked_sub(self.now))
            .map(secs)
    }

    /// When the loop must wake again for the countdown.
    pub fn next_deadline(&self) -> Option<u64> {
        let t = self.retry_until?;
        let left = t.saturating_sub(self.now);
        // the shown number is ceil(left / 1 s): it drops at the next whole second of `left`
        Some(self.now + if left % 1000 == 0 { 1000 } else { left % 1000 })
    }

    pub fn secret_len(&self) -> usize {
        match self.kind {
            LockKind::Pattern => self.pattern.len(),
            _ => self.entry.chars().count(),
        }
    }

    fn locked_out(&self) -> bool {
        self.busy || self.waiting()
    }

    // ------------------------------------------------------------------ the core's answer
    /// `unlock` came back with `unlocked: false`.
    pub fn rejected(&mut self, attempts_left: Option<u32>, retry_in_s: Option<u32>) {
        self.busy = false;
        self.wipe();
        let what = match self.kind {
            LockKind::Pin => "Wrong PIN",
            LockKind::Password => "Wrong password",
            LockKind::Pattern => "Wrong pattern",
        };
        self.error = Some(match (retry_in_s, attempts_left) {
            (Some(_), _) => "Too many wrong attempts".into(),
            (None, Some(n)) if n <= 3 => {
                format!("{what}. {n} {} left", if n == 1 { "try" } else { "tries" })
            }
            _ => what.into(),
        });
        self.set_retry(retry_in_s);
    }

    /// The request failed (core away, protocol error): let the user try again.
    pub fn failed(&mut self, msg: &str) {
        self.busy = false;
        self.wipe();
        self.error = Some(msg.into());
    }

    // ------------------------------------------------------------------ input
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

    // ------------------------------------------------------------------ drawing
    pub fn render(&mut self, p: &mut Painter) {
        let l = layout(self.area, self.kind);
        p.cv.fill(bg());
        self.hits.clear();
        // clock
        let (time, suffix) = (
            format!("{}:{:02}", self.wall.hour, self.wall.minute),
            self.wall.suffix,
        );
        let big = if self.area.0 > self.area.1 && self.kind == LockKind::Password {
            64.0
        } else {
            88.0
        };
        let tw = p.fonts.measure(&time, Face::Regular, big).ceil() as i32;
        let sw = suffix.map_or(0, |s| {
            p.fonts.measure(s, Face::Regular, 22.0).ceil() as i32 + 8
        });
        let x0 = l.clock.x + (l.clock.w - tw - sw) / 2;
        p.text(
            &time,
            Style::new(Face::Regular, big, text()),
            Rect::new(x0, l.clock.y, tw + 4, l.clock.h),
            Align::Left,
        );
        if let Some(s) = suffix {
            p.text(
                s,
                Style::new(Face::Regular, 22.0, text_secondary()),
                Rect::new(x0 + tw + 8, l.clock.y + 4, sw, l.clock.h / 2),
                Align::Left,
            );
        }
        p.text(
            &self.wall.date,
            Style::new(Face::Regular, 20.0, text_secondary()),
            l.date,
            Align::Center,
        );
        // prompt: the countdown, an error, or what to enter
        let dis = self.locked_out();
        let (msg, color) = match (self.countdown(), &self.error) {
            (Some(s), _) => (format!("Try again in {s} s"), danger()),
            (None, Some(e)) => (e.clone(), danger()),
            (None, None) => (
                match self.kind {
                    LockKind::Pin => "Enter PIN",
                    LockKind::Password => "Enter password",
                    LockKind::Pattern => "Draw your pattern",
                }
                .to_string(),
                text_secondary(),
            ),
        };
        p.text(
            &msg,
            Style::new(Face::Regular, SIZE_BODY, color),
            l.prompt,
            Align::Center,
        );
        match self.kind {
            LockKind::Pin => self.render_pin(p, &l, dis),
            LockKind::Password => self.render_password(p, &l, dis),
            LockKind::Pattern => self.render_pattern(p, &l),
        }
    }

    fn render_pin(&mut self, p: &mut Painter, l: &Layout, dis: bool) {
        let n = self.entry.chars().count();
        let slots = n.max(PIN_MIN);
        let step = 30;
        let x0 = l.entry.x + (l.entry.w - (slots as i32 - 1) * step) / 2;
        let cy = l.entry.y + l.entry.h / 2;
        for i in 0..slots {
            let cx = x0 + i as i32 * step;
            if i < n {
                p.cv.rect(Rect::new(cx - 8, cy - 8, 16, 16), text());
            } else {
                p.cv.rect(Rect::new(cx - 8, cy - 8, 16, 16), surface_strong());
                p.cv.rect(Rect::new(cx - 8, cy + 6, 16, 2), divider());
            }
        }
        for (r, h) in &l.pad {
            let pressed = self.pressed == Some(*h);
            p.outline_box(*r, 0, pressed, false, dis);
            let fg = if dis { text_disabled() } else { text() };
            match h {
                Hit::Digit(c) => p.text(
                    &c.to_string(),
                    Style::new(Face::Regular, 38.0, fg),
                    *r,
                    Align::Center,
                ),
                Hit::Backspace => p.cv.icon(
                    Icon::Backspace,
                    r.x + (r.w - 28) / 2,
                    r.y + (r.h - 28) / 2,
                    28,
                    fg,
                ),
                _ => p.cv.icon(
                    Icon::Check,
                    r.x + (r.w - 28) / 2,
                    r.y + (r.h - 28) / 2,
                    28,
                    fg,
                ),
            }
            self.hits.push((*r, *h));
        }
    }

    fn render_password(&mut self, p: &mut Painter, l: &Layout, dis: bool) {
        if let Some(eye) = p.text_field(
            l.entry,
            &self.entry,
            "Password",
            true,
            self.shown,
            !dis,
            None,
        ) {
            self.hits.push((eye, Hit::Eye));
        }
        let keys = p.keyboard(
            l.keyboard,
            self.layer,
            match self.pressed {
                Some(Hit::Key(i)) => Some(i),
                _ => None,
            },
        );
        for (i, k) in keys.iter().enumerate() {
            self.hits.push((k.rect, Hit::Key(i)));
        }
        let state = if self.busy {
            ButtonState::Loading(0)
        } else if dis || self.entry.is_empty() {
            ButtonState::Disabled
        } else if self.pressed == Some(Hit::Submit) {
            ButtonState::Pressed
        } else {
            ButtonState::Default
        };
        p.button(l.submit, "Unlock", ButtonKind::Primary, state, None);
        self.hits.push((l.submit, Hit::Submit));
    }

    fn render_pattern(&mut self, p: &mut Painter, l: &Layout) {
        let sel = &self.pattern;
        let c = |i: u8| {
            let (x, y) = l.dots[i as usize];
            (x as f32, y as f32)
        };
        for w in sel.windows(2) {
            p.cv.line(c(w[0]), c(w[1]), 5.0, outline());
        }
        if let (Some(&last), Some((fx, fy)), true) = (sel.last(), self.finger, self.drawing) {
            p.cv.line(c(last), (fx as f32, fy as f32), 5.0, outline());
        }
        for i in 0..9u8 {
            let (x, y) = l.dots[i as usize];
            if sel.contains(&i) {
                p.cv.rect(Rect::new(x - 20, y - 20, 40, 40), text());
                p.cv.rect(Rect::new(x - 6, y - 6, 12, 12), bg());
            } else {
                p.cv.rect(Rect::new(x - 12, y - 12, 24, 24), surface_strong());
                p.cv.rect(Rect::new(x - 12, y + 10, 24, 2), outline());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREAS: [(i32, i32); 2] = [(800, 1280), (1280, 800)];

    fn wall() -> WallTime {
        WallTime::new(9, 41, "Tuesday, 10 November", true)
    }

    fn screen(kind: LockKind, area: (i32, i32)) -> LockScreen {
        LockScreen::new(kind, area, wall())
    }

    /// Draw once so the hit areas exist, then tap the centre of the hit area `h`.
    fn tap(s: &mut LockScreen, h: Hit) -> Out {
        let (mut cv, mut f) = canvas(s.area);
        s.render(&mut Painter {
            cv: &mut cv,
            fonts: &mut f,
        });
        let r = s.hits.iter().find(|(_, x)| *x == h).expect("hit").0;
        let (x, y) = r.center();
        s.down(x, y);
        s.up(x, y)
    }

    fn canvas(area: (i32, i32)) -> (walpad_ui::Canvas, walpad_ui::Fonts) {
        let f = walpad_ui::Fonts::load(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"),
        )
        .unwrap();
        (
            walpad_ui::Canvas::new(area.0 as u32, area.1 as u32, BG).unwrap(),
            f,
        )
    }

    #[test]
    fn layouts_fit_the_screen_and_the_targets_are_big_enough() {
        for area in AREAS {
            let r = Rect::new(0, 0, area.0, area.1);
            for kind in [LockKind::Pin, LockKind::Password, LockKind::Pattern] {
                let l = layout(area, kind);
                for rect in [l.clock, l.prompt, l.date]
                    .into_iter()
                    .chain(l.pad.iter().map(|p| p.0))
                    .chain((kind == LockKind::Password).then_some(l.keyboard))
                    .chain((kind == LockKind::Password).then_some(l.submit))
                    .chain((kind == LockKind::Password).then_some(l.entry))
                {
                    assert!(
                        rect.x >= 0 && rect.y >= 0 && rect.right() <= r.w && rect.bottom() <= r.h,
                        "{kind:?} {area:?}: {rect:?}"
                    );
                }
                for (rect, _) in &l.pad {
                    assert!(rect.w >= 56 && rect.h >= 56);
                }
                if kind == LockKind::Pin {
                    assert_eq!(l.pad.len(), 12);
                    // the pad stays clear of the prompt
                    assert!(l.pad.iter().all(|(p, _)| p.y > l.entry.y));
                }
                if kind == LockKind::Pattern {
                    for (x, y) in l.dots {
                        assert!(x > 40 && x < area.0 - 40 && y > 40 && y < area.1 - 40);
                    }
                }
                if kind == LockKind::Password {
                    assert!(l.date.bottom() < l.prompt.y, "{area:?}: clock above prompt");
                    assert!(l.entry.bottom() <= l.keyboard.y);
                }
            }
        }
    }

    #[test]
    fn pin_digits_backspace_and_submit() {
        let mut s = screen(LockKind::Pin, (800, 1280));
        for d in ['4', '3', '2'] {
            assert_eq!(tap(&mut s, Hit::Digit(d)), Out::Redraw);
        }
        // too short: refused with a message
        assert_eq!(tap(&mut s, Hit::Submit), Out::Redraw);
        assert!(s.error.as_deref().unwrap().contains("at least 4"));
        tap(&mut s, Hit::Digit('1'));
        assert!(s.error.is_none(), "typing clears the message");
        tap(&mut s, Hit::Digit('9'));
        tap(&mut s, Hit::Backspace);
        assert_eq!(s.secret_len(), 4);
        assert_eq!(tap(&mut s, Hit::Submit), Out::Submit("4321".into()));
        assert_eq!(
            s.secret_len(),
            0,
            "the secret leaves the screen state at once"
        );
        assert!(s.busy);
        // nothing is accepted while waiting for the core
        assert_eq!(s.down(10, 10), Out::None);
        // a wrong PIN: message, tries left, field empty again
        s.rejected(Some(2), None);
        assert!(!s.busy);
        assert_eq!(s.error.as_deref(), Some("Wrong PIN. 2 tries left"));
        s.rejected(Some(4), None);
        assert_eq!(s.error.as_deref(), Some("Wrong PIN"));
    }

    #[test]
    fn five_wrong_pins_show_a_countdown_and_the_pad_is_dead_during_it() {
        let mut s = screen(LockKind::Pin, (800, 1280));
        s.now = 1000;
        s.rejected(None, Some(30));
        assert!(s.waiting());
        assert_eq!(s.error.as_deref(), Some("Too many wrong attempts"));
        assert_eq!(s.countdown(), Some(30));
        // taps do nothing during the delay
        let (mut cv, mut f) = canvas(s.area);
        s.render(&mut Painter {
            cv: &mut cv,
            fonts: &mut f,
        });
        let r = s
            .hits
            .iter()
            .find(|(_, h)| *h == Hit::Digit('1'))
            .unwrap()
            .0;
        assert_eq!(s.down(r.x + 5, r.y + 5), Out::None);
        assert!(s.tick(2000), "the number changes every second");
        assert_eq!(s.countdown(), Some(29));
        assert!(!s.tick(2100));
        assert!(s.tick(31_000));
        assert!(!s.waiting());
        assert!(s.error.is_none(), "the message goes with the countdown");
        assert_eq!(s.down(r.x + 5, r.y + 5), Out::Redraw);
    }

    #[test]
    fn next_deadline_is_the_next_second_of_the_countdown() {
        let mut s = screen(LockKind::Pin, (800, 1280));
        s.now = 0;
        s.set_retry(Some(30));
        s.tick(400);
        assert_eq!(s.next_deadline(), Some(1000), "29.6 s left: wake in 0.6 s");
        s.tick(1000);
        assert_eq!(s.next_deadline(), Some(2000));
        assert_eq!(screen(LockKind::Pin, (800, 1280)).next_deadline(), None);
    }

    #[test]
    fn password_typed_on_the_keyboard_submits_with_unlock_or_done() {
        let mut s = screen(LockKind::Password, (1280, 800));
        let (mut cv, mut f) = canvas(s.area);
        s.render(&mut Painter {
            cv: &mut cv,
            fonts: &mut f,
        });
        let key = |s: &LockScreen, c: char| {
            let l = layout(s.area, s.kind);
            keyboard_layout(l.keyboard, s.layer)
                .iter()
                .position(|k| k.key == Key::Char(c))
                .unwrap()
        };
        for c in "abcd".chars() {
            let i = key(&s, c);
            tap(&mut s, Hit::Key(i));
        }
        // shift: next letter upper case, then back to lower
        let l = layout(s.area, s.kind);
        let shift = keyboard_layout(l.keyboard, s.layer)
            .iter()
            .position(|k| k.key == Key::Shift)
            .unwrap();
        tap(&mut s, Hit::Key(shift));
        let i = key(&s, 'E');
        tap(&mut s, Hit::Key(i));
        assert_eq!(s.secret_len(), 5);
        assert_eq!(tap(&mut s, Hit::Submit), Out::Submit("abcdE".into()));
        // a hardware keyboard works too
        s.failed("try again");
        for sym in ['h' as u32, 'i' as u32] {
            assert_eq!(s.keysym(sym), Out::Redraw);
        }
        assert_eq!(s.keysym(0xff0d), Out::Submit("hi".into()));
        // a PIN screen takes only digits from a hardware keyboard
        let mut p = screen(LockKind::Pin, (800, 1280));
        assert_eq!(p.keysym('a' as u32), Out::None);
        assert_eq!(p.keysym('7' as u32), Out::Redraw);
    }

    fn drag(s: &mut LockScreen, dots: &[usize]) -> Out {
        let l = layout(s.area, LockKind::Pattern);
        let (x, y) = l.dots[dots[0]];
        s.down(x, y);
        for d in &dots[1..] {
            let (x, y) = l.dots[*d];
            s.moved(x, y);
        }
        let (x, y) = l.dots[*dots.last().unwrap()];
        s.up(x, y)
    }

    #[test]
    fn pattern_collects_dots_including_the_one_jumped_over() {
        let mut s = screen(LockKind::Pattern, (800, 1280));
        // 0 -> 2 passes over 1; 2 -> 8 passes over 5
        assert_eq!(drag(&mut s, &[0, 2, 8, 7]), Out::Submit("012587".into()));
        s.failed("x");
        // a knight's move jumps nothing; revisiting a dot is ignored
        assert_eq!(
            drag(&mut s, &[0, 5, 0, 5, 4, 3]),
            Out::Submit("0543".into())
        );
    }

    #[test]
    fn a_short_pattern_is_refused_on_the_spot() {
        let mut s = screen(LockKind::Pattern, (1280, 800));
        assert_eq!(drag(&mut s, &[0, 1, 4]), Out::Redraw);
        assert_eq!(s.error.as_deref(), Some("Connect at least 4 dots"));
        assert_eq!(s.secret_len(), 0);
        assert!(!s.busy);
    }
}
