//! Bottom sheet, confirmation dialog, and power menu overlays for [`Painter`].
use super::{ButtonKind, ButtonState, Painter};
use crate::canvas::Rect;
use crate::text::{Align, Face, Style};
use crate::tokens::*;

impl Painter<'_> {
    /// Bottom sheet container; returns the content rect inside it.
    pub fn sheet(&mut self, r: Rect, title: &str) -> Rect {
        self.popover(r);
        self.divider(r.x, r.y, r.w);
        self.cv
            .rect(Rect::new(r.x + r.w / 2 - 24, r.y + 8, 48, 4), divider());
        self.text(
            title,
            Style::new(Face::Regular, SIZE_TITLE, text()),
            Rect::new(r.x + SP_5, r.y + 20, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        Rect::new(r.x + SP_5, r.y + 68, r.w - 2 * SP_5, r.h - 68 - SP_5)
    }

    /// Centred confirmation dialog. Returns (cancel, confirm) button rects. `armed` = the
    /// destructive first tap happened: the confirm button reads `confirm_armed`.
    #[allow(clippy::too_many_arguments)]
    pub fn dialog(
        &mut self,
        screen: Rect,
        title: &str,
        body: &str,
        cancel: &str,
        confirm: &str,
        confirm_armed: &str,
        danger: bool,
        armed: bool,
    ) -> (Rect, Rect) {
        let w = 560.min(screen.w - 2 * SP_5);
        let h = 260;
        let r = Rect::new(
            screen.x + (screen.w - w) / 2,
            screen.y + (screen.h - h) / 2,
            w,
            h,
        );
        self.popover(r);
        self.divider(r.x, r.y, r.w);
        self.text(
            title,
            Style::new(Face::Regular, SIZE_TITLE, text()),
            Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        self.small(
            body,
            Rect::new(r.x + SP_5, r.y + 72, r.w - 2 * SP_5, 48),
            Align::Left,
            text_secondary(),
        );
        let bw = (r.w - 2 * SP_5 - SP_4) / 2;
        let cancel_r = Rect::new(r.x + SP_5, r.bottom() - SP_5 - BUTTON_H, bw, BUTTON_H);
        let confirm_r = Rect::new(cancel_r.right() + SP_4, cancel_r.y, bw, BUTTON_H);
        self.button(
            cancel_r,
            cancel,
            ButtonKind::Secondary,
            ButtonState::Default,
            None,
        );
        let kind = if danger {
            ButtonKind::Danger
        } else {
            ButtonKind::Primary
        };
        self.button(
            confirm_r,
            if armed { confirm_armed } else { confirm },
            kind,
            ButtonState::Default,
            None,
        );
        (cancel_r, confirm_r)
    }

    /// Power menu (long power press). `focus` is the keyboard/hardware-key focus indicator, `armed`
    /// the item whose first tap happened (danger + "Tap again"). Returns one rect per item.
    pub fn power_menu(
        &mut self,
        screen: Rect,
        items: &[(&str, bool)],
        focus: usize,
        armed: Option<usize>,
    ) -> Vec<Rect> {
        let w = 560.min(screen.w - 2 * SP_5);
        let item_h = BUTTON_H + SP_2;
        let h = SP_5 * 2 + 40 + SP_3 + items.len() as i32 * item_h;
        let r = Rect::new(
            screen.x + (screen.w - w) / 2,
            screen.y + (screen.h - h) / 2,
            w,
            h,
        );
        self.popover(r);
        self.divider(r.x, r.y, r.w);
        self.text(
            "Power",
            Style::new(Face::Regular, SIZE_TITLE, text()),
            Rect::new(r.x + SP_5, r.y + SP_5, r.w - 2 * SP_5, 40),
            Align::Left,
        );
        let mut out = Vec::new();
        for (i, (label, dangerous)) in items.iter().enumerate() {
            let ir = Rect::new(
                r.x + SP_5,
                r.y + SP_5 + 40 + SP_3 + i as i32 * item_h,
                r.w - 2 * SP_5,
                BUTTON_H,
            );
            let is_armed = armed == Some(i);
            let text = if is_armed {
                format!("Tap again to {}", label.to_lowercase())
            } else {
                label.to_string()
            };
            let kind = if *dangerous && is_armed {
                ButtonKind::Danger
            } else {
                ButtonKind::Secondary
            };
            self.button(ir, &text, kind, ButtonState::Default, None);
            if focus == i {
                // focus bar: 4 px white left indicator + pressed tint
                self.cv.rect(ir, pressed_tint());
                self.cv
                    .rect(Rect::new(ir.x, ir.y, 4, ir.h), crate::tokens::text());
            }
            out.push(ir);
        }
        out
    }
}
