//! Text input field, toast, banner, spinner, empty state, and error state for [`Painter`].
use super::{banner_tint, tone_color, ButtonKind, ButtonState, Painter, Tone};
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Fonts, Style};
use crate::tokens::*;

impl Painter<'_> {
    /// Returns the rect of the show/hide button (secret fields) for hit testing.
    #[allow(clippy::too_many_arguments)]
    pub fn text_field(
        &mut self,
        r: Rect,
        value: &str,
        placeholder: &str,
        secret: bool,
        shown: bool,
        focused: bool,
        error: Option<&str>,
    ) -> Option<Rect> {
        let (line, line_h) = if error.is_some() {
            (danger(), 2)
        } else if focused {
            (text(), 2)
        } else {
            (divider(), 1)
        };
        self.cv.rect(r, surface());
        self.cv
            .rect(Rect::new(r.x, r.bottom() - line_h, r.w, line_h), line);
        let eye = secret.then(|| Rect::new(r.right() - TOUCH_MIN, r.y, TOUCH_MIN, r.h));
        let text_w = r.w - 2 * SP_4 - if secret { TOUCH_MIN - SP_4 } else { 0 };
        let area = Rect::new(r.x + SP_4, r.y, text_w, r.h);
        // the text as displayed: bullets for hidden secrets, otherwise the tail that fits
        let display = if value.is_empty() {
            String::new()
        } else if secret && !shown {
            "•".repeat(value.chars().count())
        } else {
            tail_fit(self.fonts, value, text_w as f32)
        };
        if value.is_empty() {
            self.body(placeholder, area, Align::Left, text_disabled());
        } else {
            self.body(&display, area, Align::Left, text());
        }
        if focused {
            let w = if display.is_empty() {
                0.0
            } else {
                self.fonts.measure(&display, Face::Regular, SIZE_BODY)
            };
            let cx = (area.x as f32 + w).round() as i32 + 1;
            self.cv.rect(Rect::new(cx, r.y + 14, 2, r.h - 28), text());
        }
        if let Some(e) = eye {
            self.cv.icon(
                if shown { Icon::EyeOff } else { Icon::Eye },
                e.x + (e.w - ICON) / 2,
                e.y + (e.h - ICON) / 2,
                ICON,
                text_secondary(),
            );
        }
        if let Some(msg) = error {
            self.small(
                msg,
                Rect::new(r.x + 4, r.bottom() + 6, r.w, 22),
                Align::Left,
                danger(),
            );
        }
        eye
    }

    /// One toast, centred near the top of `area` (the main area below the header).
    pub fn toast(&mut self, area: Rect, text: &str, tone: Tone) -> Rect {
        let tw = self.fonts.measure(text, Face::Regular, SIZE_SECONDARY) as i32;
        let w = (tw + 2 * SP_4 + ICON + SP_2 + 8)
            .min(area.w - 2 * SP_5)
            .max(160);
        let r = Rect::new(area.x + (area.w - w) / 2, area.y + SP_2, w, 48);
        self.cv.rect(r, surface_strong().over(bg()));
        self.cv.rect(r, banner_tint(tone));
        self.cv.rect(Rect::new(r.x, r.y, 3, r.h), tone_color(tone));
        let ic = match tone {
            Tone::Info => Icon::Check,
            _ => Icon::Warning,
        };
        self.cv
            .icon(ic, r.x + SP_4, r.y + 12, ICON, tone_color(tone));
        self.small(
            text,
            Rect::new(
                r.x + SP_4 + ICON + SP_2,
                r.y,
                r.w - 2 * SP_4 - ICON - SP_2,
                r.h,
            ),
            Align::Left,
            crate::tokens::text(),
        );
        r
    }

    pub fn banner(&mut self, r: Rect, text: &str, tone: Tone) {
        self.cv.rect(r, surface_strong());
        self.cv.rect(r, banner_tint(tone));
        self.cv.rect(Rect::new(r.x, r.y, 3, r.h), tone_color(tone));
        self.cv.icon(
            Icon::Warning,
            r.x + SP_4,
            r.y + (r.h - ICON) / 2,
            ICON,
            tone_color(tone),
        );
        self.small(
            text,
            Rect::new(
                r.x + SP_4 + ICON + SP_3,
                r.y,
                r.w - 2 * SP_4 - ICON - SP_3,
                r.h,
            ),
            Align::Left,
            crate::tokens::text(),
        );
    }

    pub fn spinner(&mut self, cx: i32, cy: i32, size: i32, deg: f32) {
        self.cv.icon_rotated(
            Icon::Spinner,
            cx - size / 2,
            cy - size / 2,
            size,
            text(),
            deg,
        );
    }

    pub fn empty_state(&mut self, r: Rect, icon: Icon, title: &str, hint: &str) {
        let (cx, cy) = r.center();
        self.cv.icon(icon, cx - 24, cy - 56, 48, text_disabled());
        self.text(
            title,
            Style::new(Face::Regular, SIZE_BODY, text_secondary()),
            Rect::new(r.x, cy, r.w, 28),
            Align::Center,
        );
        self.small(
            hint,
            Rect::new(r.x + SP_5, cy + 30, r.w - 2 * SP_5, 24),
            Align::Center,
            text_disabled(),
        );
    }

    /// Returns the retry button rect.
    pub fn error_state(&mut self, r: Rect, title: &str, msg: &str, retry: &str) -> Rect {
        let (cx, cy) = r.center();
        self.cv.icon(Icon::Warning, cx - 24, cy - 88, 48, danger());
        self.text(
            title,
            Style::new(Face::Regular, SIZE_BODY, text()),
            Rect::new(r.x, cy - 28, r.w, 28),
            Align::Center,
        );
        self.small(
            msg,
            Rect::new(r.x + SP_5, cy + 2, r.w - 2 * SP_5, 24),
            Align::Center,
            text_secondary(),
        );
        let b = Rect::new(cx - 90, cy + 40, 180, BUTTON_H);
        self.button(
            b,
            retry,
            ButtonKind::Secondary,
            ButtonState::Default,
            Some(Icon::Refresh),
        );
        b
    }
}

/// The end of `s` that fits `max_w` (long field contents stay visible at the cursor).
fn tail_fit(fonts: &mut Fonts, s: &str, max_w: f32) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let t: String = chars[start..].iter().collect();
        if fonts.measure(&t, Face::Regular, SIZE_BODY) <= max_w {
            return t;
        }
        start += 1;
    }
    String::new()
}
