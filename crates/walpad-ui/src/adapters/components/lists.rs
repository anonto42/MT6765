//! List row, signal bars, and network row rendering for [`Painter`].
use super::{Accessory, NetState, Painter};
use crate::canvas::Rect;
use crate::icons::Icon;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

impl Painter<'_> {
    /// Pressed highlight of a list row: flush edge-to-edge `PRESSED_TINT` across the row.
    pub fn row_pressed(&mut self, r: Rect) {
        self.cv
            .rect(Rect::new(r.x, r.y, r.w, (r.h - 1).max(1)), pressed_tint());
    }

    /// Returns the rect of the toggle when the accessory is one (for hit testing).
    #[allow(clippy::too_many_arguments)]
    pub fn list_row(
        &mut self,
        r: Rect,
        icon: Option<Icon>,
        title: &str,
        subtitle: Option<&str>,
        acc: &Accessory,
        pressed: bool,
        disabled: bool,
    ) -> Option<Rect> {
        if !self.cv.visible(r) {
            return matches!(acc, Accessory::Toggle(_))
                .then(|| Rect::new(r.right() - SP_4 - 56, r.y + (r.h - 32) / 2, 56, 32));
        }
        if pressed && !disabled {
            self.row_pressed(r);
        }
        self.divider(r.x, r.bottom() - 1, r.w);
        let fg = if disabled { text_disabled() } else { text() };
        let mut x = r.x + SP_4;
        if let Some(ic) = icon {
            self.cv.icon(
                ic,
                x,
                r.y + (r.h - ICON) / 2,
                ICON,
                if disabled { text_disabled() } else { outline() },
            );
            x += ICON + SP_4;
        }
        let right_w = match acc {
            Accessory::None => 0,
            Accessory::Chevron => ICON + SP_4,
            Accessory::Toggle(_) => 56 + SP_4,
            Accessory::Value(v) | Accessory::Action(v) => {
                self.fonts.measure(v, Face::Regular, SIZE_BODY) as i32 + SP_4 + SP_3
            }
        };
        let text_w = r.w - (x - r.x) - right_w - SP_2;
        match subtitle {
            Some(sub) => {
                self.body(title, Rect::new(x, r.y + 8, text_w, 28), Align::Left, fg);
                self.small(
                    sub,
                    Rect::new(x, r.y + 34, text_w, 22),
                    Align::Left,
                    text_secondary(),
                );
            }
            None => self.body(title, Rect::new(x, r.y, text_w, r.h), Align::Left, fg),
        }
        let ar = Rect::new(r.right() - right_w - SP_2, r.y, right_w, r.h);
        match acc {
            Accessory::None => None,
            Accessory::Chevron => {
                self.cv.icon(
                    Icon::ChevronRight,
                    r.right() - SP_4 - ICON,
                    r.y + (r.h - ICON) / 2,
                    ICON,
                    text_secondary(),
                );
                None
            }
            Accessory::Toggle(on) => Some(self.toggle(
                r.right() - SP_4 - 56,
                r.y + (r.h - 32) / 2,
                *on,
                pressed,
                disabled,
            )),
            Accessory::Value(v) => {
                self.body(
                    v,
                    Rect::new(ar.x, ar.y, right_w - SP_4, ar.h),
                    Align::Right,
                    text_secondary(),
                );
                None
            }
            Accessory::Action(v) => {
                self.text(
                    v,
                    Style::new(
                        Face::Bold,
                        SIZE_BODY,
                        if disabled { text_disabled() } else { text() },
                    ),
                    Rect::new(ar.x, ar.y, right_w - SP_4, ar.h),
                    Align::Right,
                );
                None
            }
        }
    }

    /// Four-step signal icon (0..=4 bars lit).
    pub fn signal_bars(&mut self, x: i32, y: i32, bars: u8, color: Color) {
        for i in 0..4 {
            let h = 6 + i * 4;
            let lit = (i as u8) < bars;
            self.cv.rect(
                Rect::new(x + i * 7, y + 18 - h, 4, h),
                if lit { color } else { divider() },
            );
        }
    }

    pub fn network_row(
        &mut self,
        r: Rect,
        ssid: &str,
        bars: u8,
        secured: bool,
        state: NetState,
        pressed: bool,
    ) {
        if !self.cv.visible(r) {
            return;
        }
        if pressed {
            self.row_pressed(r);
        }
        self.divider(r.x, r.bottom() - 1, r.w);
        let (sub, subcol) = match (state, secured) {
            (NetState::Connected, _) => ("Connected", success()),
            (NetState::Connecting(_), _) => ("Connecting…", warning()),
            (_, true) => ("Secured", text_secondary()),
            (_, false) => ("Open", text_secondary()),
        };
        let right = r.right() - SP_4;
        let sig_x = right - 26;
        self.signal_bars(
            sig_x,
            r.y + (r.h - 18) / 2,
            bars,
            if state == NetState::Connected {
                success()
            } else {
                text()
            },
        );
        let mut tx = sig_x - SP_3;
        if secured {
            tx -= ICON;
            self.cv.icon(
                Icon::Lock,
                tx,
                r.y + (r.h - ICON) / 2,
                ICON,
                text_secondary(),
            );
            tx -= SP_2;
        }
        // connection state icon (check / spinner) left of the lock
        let status_x = match state {
            NetState::Idle => None,
            _ => {
                tx -= ICON;
                let x = tx;
                tx -= SP_2;
                Some(x)
            }
        };
        let w = tx - (r.x + SP_4);
        self.text(
            ssid,
            Style::new(
                if state == NetState::Connected {
                    Face::Bold
                } else {
                    Face::Regular
                },
                SIZE_BODY,
                text(),
            ),
            Rect::new(r.x + SP_4, r.y + 10, w, 28),
            Align::Left,
        );
        self.text(
            sub,
            Style::new(Face::Regular, SIZE_SECONDARY, subcol),
            Rect::new(r.x + SP_4, r.y + 38, w, 22),
            Align::Left,
        );
        if let Some(x) = status_x {
            let y = r.y + (r.h - ICON) / 2;
            match state {
                NetState::Connecting(deg) => {
                    self.cv
                        .icon_rotated(Icon::Spinner, x, y, ICON, warning(), deg as f32)
                }
                _ => self.cv.icon(Icon::Check, x, y, ICON, success()),
            }
        }
    }
}
