//! Interactive controls for [`Painter`]: buttons, status cards, toggles, sliders, segmented controls.
use super::{danger_tint, danger_tint_pressed, ButtonKind, ButtonState, CardValue, Painter};
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

impl Painter<'_> {
    pub fn button(
        &mut self,
        r: Rect,
        label: &str,
        kind: ButtonKind,
        state: ButtonState,
        icon: Option<Icon>,
    ) {
        let disabled = state == ButtonState::Disabled;
        let pressed = state == ButtonState::Pressed;
        let btn_primary = Tint::new(Color::hex(0x303642), 250);
        let btn_secondary = Tint::new(Color::hex(0x272c36), 245);
        let btn_disabled = Tint::new(Color::hex(0x1e2128), 200);
        let (fill, wash, bar, bar_w, line, line_h, fg) = match (kind, disabled) {
            (_, true) => (
                btn_disabled,
                None,
                divider(),
                0,
                divider(),
                1,
                text_disabled(),
            ),
            (ButtonKind::Primary, _) => (
                btn_primary,
                pressed.then_some(pressed_tint()),
                text(),
                3,
                text(),
                2,
                text(),
            ),
            (ButtonKind::Danger, _) => (
                btn_secondary,
                Some(if pressed {
                    danger_tint_pressed()
                } else {
                    danger_tint()
                }),
                danger(),
                3,
                danger(),
                2,
                danger(),
            ),
            (ButtonKind::Secondary, _) => (
                btn_secondary,
                pressed.then_some(pressed_tint()),
                if pressed { text() } else { text_secondary() },
                2,
                if pressed { text() } else { divider() },
                if pressed { 2 } else { 1 },
                text(),
            ),
        };
        self.cv.rect(r, fill);
        if let Some(f) = wash {
            self.cv.rect(r, f);
        }
        if bar_w > 0 {
            self.cv.rect(Rect::new(r.x, r.y, bar_w, r.h), bar);
        }
        self.cv
            .rect(Rect::new(r.x, r.bottom() - line_h, r.w, line_h), line);
        if let ButtonState::Loading(deg) = state {
            let (cx, cy) = r.center();
            self.cv
                .icon_rotated(Icon::Spinner, cx - 12, cy - 12, ICON, text(), deg as f32);
            return;
        }
        let style = Style::new(Face::Regular, SIZE_BODY, fg);
        match icon {
            Some(ic) => {
                let w = self.fonts.measure(label, Face::Regular, SIZE_BODY) as i32 + ICON + SP_2;
                let x0 = r.x + (r.w - w).max(SP_3 * 2) / 2;
                self.cv.icon(ic, x0, r.y + (r.h - ICON) / 2, ICON, fg);
                let tx = x0 + ICON + SP_2;
                self.text(
                    label,
                    style,
                    Rect::new(tx, r.y, r.right() - SP_3 - tx, r.h),
                    Align::Left,
                );
            }
            None => self.text(label, style, r.inset(SP_2), Align::Center),
        }
    }

    pub fn status_card(
        &mut self,
        r: Rect,
        title: &str,
        value: &CardValue,
        detail: Option<&str>,
        pressed: bool,
    ) {
        self.card(r, pressed);
        let inner = r.inset(SP_4);
        self.text(
            &title.to_uppercase(),
            Style::new(Face::Bold, SIZE_TINY, text_secondary()),
            Rect::new(inner.x, inner.y, inner.w, 18),
            Align::Left,
        );
        let (text, color, caption): (String, Color, Option<(&str, Color)>) = match value {
            CardValue::Value(v) => (v.clone(), text(), None),
            CardValue::Estimated(v) => (
                format!("~{v}"),
                text(),
                Some(("Estimated from voltage", warning())),
            ),
            CardValue::Unknown => ("Unknown".into(), text_secondary(), None),
            CardValue::Error(e) => (e.clone(), danger(), None),
        };
        self.text(
            &text,
            Style::new(Face::Regular, SIZE_TITLE, color),
            Rect::new(inner.x, inner.y + 22, inner.w, 36),
            Align::Left,
        );
        let line3 = Rect::new(inner.x, inner.y + 60, inner.w, 20);
        match (caption, detail) {
            (Some((c, col)), _) => self.caption(c, line3, Align::Left, col),
            (None, Some(d)) => self.text(
                d,
                Style::new(Face::Mono, SIZE_CAPTION, text_secondary()),
                line3,
                Align::Left,
            ),
            _ => {}
        }
    }

    pub fn toggle(&mut self, x: i32, y: i32, on: bool, pressed: bool, disabled: bool) -> Rect {
        let r = Rect::new(x, y, 56, 32);
        let kx = if on { r.right() - 28 } else { r.x + 4 };
        let knob = Rect::new(kx, r.y + 4, 24, 24);
        if on && !disabled {
            self.cv.rect(r, text());
            if pressed {
                self.cv.rect(r, pressed_tint());
            }
            self.cv.rect(knob, bg());
        } else {
            self.cv.rect(r, surface_strong());
            if pressed && !disabled {
                self.cv.rect(r, pressed_tint());
            }
            self.cv
                .rect(Rect::new(r.x, r.bottom() - 1, r.w, 1), divider());
            self.cv.rect(
                knob,
                if disabled {
                    text_disabled()
                } else {
                    text_secondary()
                },
            );
        }
        r
    }

    /// Value 0.0..=1.0. Returns the track rect (the caller maps x to a value).
    pub fn slider(&mut self, r: Rect, value: f32, dragging: bool, disabled: bool) -> Rect {
        let v = value.clamp(0.0, 1.0);
        let track = Rect::new(r.x + 16, r.y + r.h / 2 - 2, r.w - 32, 4);
        let fill = if disabled { text_disabled() } else { text() };
        self.cv.rect(track, divider());
        let fw = (track.w as f32 * v) as i32;
        if fw > 0 {
            self.cv
                .rect(Rect::new(track.x, track.y, fw.max(4), 4), fill);
        }
        let kx = track.x + fw;
        let ky = track.y + 2;
        let (tw, th) = if dragging { (20, 28) } else { (16, 24) };
        self.cv
            .rect(Rect::new(kx - tw / 2, ky - th / 2, tw, th), fill);
        self.cv.rect(Rect::new(kx - 2, ky - 4, 4, 8), bg());
        track
    }

    /// Returns one hit rect per segment.
    pub fn segmented(
        &mut self,
        r: Rect,
        labels: &[&str],
        selected: usize,
        pressed: Option<usize>,
        disabled: bool,
    ) -> Vec<Rect> {
        self.cv.rect(r, surface());
        let n = labels.len().max(1) as i32;
        let sw = r.w / n;
        let mut out = Vec::new();
        for (i, l) in labels.iter().enumerate() {
            let sx = r.x + i as i32 * sw;
            let w = if i as i32 == n - 1 {
                r.right() - sx
            } else {
                sw
            };
            let seg = Rect::new(sx, r.y, w, r.h);
            let sel = i == selected;
            if sel {
                self.cv.rect(seg, surface_strong());
                self.cv.rect(seg, pressed_tint());
                self.cv.rect(
                    Rect::new(seg.x, seg.bottom() - 2, seg.w, 2),
                    if disabled { text_disabled() } else { text() },
                );
            } else if pressed == Some(i) && !disabled {
                self.cv.rect(seg, pressed_tint());
            }
            if i > 0 {
                self.cv.rect(Rect::new(sx, r.y, 1, r.h), divider());
            }
            let fg = match (sel, disabled) {
                (_, true) => text_disabled(),
                (true, _) => text(),
                _ => text_secondary(),
            };
            self.text(
                l,
                Style::new(
                    if sel { Face::Bold } else { Face::Regular },
                    SIZE_SECONDARY,
                    fg,
                ),
                seg,
                Align::Center,
            );
            out.push(seg);
        }
        out
    }
}
