//! Rendering for the lock screen (clock, prompt, PIN pad, password keyboard, pattern grid).

use super::{layout, Hit, Layout, LockScreen, PIN_MIN};
use walpad_proto::LockKind;
use walpad_ui::components::{ButtonKind, ButtonState, Painter};
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

impl LockScreen {
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
