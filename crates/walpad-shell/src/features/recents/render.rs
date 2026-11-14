use super::{Geometry, Recents, CLOSE_BTN, LABEL_H, PAD};
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

impl Recents {
    pub fn render(&self, p: &mut Painter) {
        let (w, h) = self.area;
        p.cv.rect(Rect::new(0, 0, w, h), surface_strong().over(bg()));
        if self.cards.is_empty() {
            p.empty_state(
                Rect::new(0, h / 2 - 120, w, 240),
                Icon::Apps,
                "No recent apps",
                "Open an app from Home",
            );
            return;
        }
        let g = self.geo();
        for i in 0..self.cards.len() {
            if self.closing[i].is_some() {
                continue;
            }
            let r = self.visible_rect(i);
            if r.right() <= 0 || r.x >= w {
                continue;
            }
            self.render_card(p, i, r, g);
        }
        p.text(
            "Swipe a card up to close it",
            Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
            Rect::new(0, h - 96, w, 24),
            Align::Center,
        );
    }

    fn render_card(&self, p: &mut Painter, i: usize, r: Rect, g: Geometry) {
        p.card(r, self.pressed == Some(i));
        let thumb = Rect::new(r.x + PAD, r.y + PAD, g.thumb_w, g.thumb_h);
        match &self.scaled[i] {
            Some(t) => p.cv.blit(t, thumb.x, thumb.y),
            None => {
                p.cv.rect(thumb, surface_icon().over(bg()));
                let big = (g.thumb_w / 3).min(96);
                let c = thumb.center();
                p.cv.icon(
                    self.cards[i].icon,
                    c.0 - big / 2,
                    c.1 - big / 2,
                    big,
                    text_secondary(),
                );
            }
        }
        let ly = r.y + PAD + g.thumb_h;
        p.divider(r.x, ly, r.w);
        p.cv.icon(
            self.cards[i].icon,
            r.x + PAD + SP_1,
            ly + (LABEL_H - ICON) / 2,
            ICON,
            text(),
        );
        p.text(
            &self.cards[i].name,
            Style::new(Face::Regular, SIZE_SECONDARY, text()),
            Rect::new(
                r.x + PAD + SP_1 + ICON + SP_2,
                ly,
                g.card_w - 2 * PAD - ICON - CLOSE_BTN - 2 * SP_2,
                LABEL_H,
            ),
            Align::Left,
        );
        let cb = self.close_rect(r);
        p.cv.icon(
            Icon::Close,
            cb.x + (cb.w - ICON) / 2,
            cb.y + (cb.h - ICON) / 2,
            ICON,
            text_secondary(),
        );
    }
}
