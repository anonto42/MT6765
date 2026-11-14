//! Recents overview (`app-switcher-navigation`): one card per running app (thumbnail + name),
//! most recent first, over the whole visible area. Tap a card = focus, swipe a card up = close,
//! drag sideways = scroll, tap the empty background = dismiss. Pure state + drawing; the X side
//! (window, thumbnails, focusing) is `nav.rs`.
use crate::gesture::{Gesture, Ms};
use walpad_ui::{Canvas, Rect};

/// Gap between cards and to the screen edge.
mod layout;
mod render;
#[cfg(test)]
mod tests;

use layout::Drag;
pub use layout::{
    geometry, Action, Card, Geometry, CLOSE_BTN, CLOSE_DRAG_PX, CLOSE_TIMEOUT_MS, FIT, GAP,
    LABEL_H, PAD,
};

pub struct Recents {
    pub(super) area: (i32, i32),
    pub(super) cards: Vec<Card>,
    pub(super) scaled: Vec<Option<Canvas>>,
    /// Vertical drag offset per card (<= 0).
    offset: Vec<i32>,
    /// A close request is out for this card (since).
    pub(super) closing: Vec<Option<Ms>>,
    scroll: i32,
    drag: Drag,
    pub(super) pressed: Option<usize>,
}

impl Recents {
    pub fn new(area: (i32, i32), cards: Vec<Card>) -> Recents {
        let n = cards.len();
        let mut r = Recents {
            area,
            cards,
            scaled: Vec::new(),
            offset: vec![0; n],
            closing: vec![None; n],
            scroll: 0,
            drag: Drag::None,
            pressed: None,
        };
        r.rescale();
        r
    }

    /// The visible area changed (rotation): lay the cards out again.
    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.drag = Drag::None;
        self.pressed = None;
        self.offset.iter_mut().for_each(|o| *o = 0);
        self.rescale();
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
    pub fn apps(&self) -> Vec<&str> {
        self.cards.iter().map(|c| c.app.as_str()).collect()
    }
    pub fn scroll(&self) -> i32 {
        self.scroll
    }
    /// A close request for this app is out.
    pub fn is_closing(&self, app: &str) -> bool {
        self.cards
            .iter()
            .zip(&self.closing)
            .any(|(c, s)| c.app == app && s.is_some())
    }

    fn geo(&self) -> Geometry {
        geometry(self.area, self.cards.len())
    }

    fn rescale(&mut self) {
        let g = self.geo();
        self.scaled = self
            .cards
            .iter()
            .map(|c| {
                c.thumb
                    .as_ref()
                    .and_then(|t| t.scaled(g.thumb_w.max(1) as u32, g.thumb_h.max(1) as u32))
            })
            .collect();
        self.scroll = self.scroll.clamp(0, g.max_scroll);
    }

    /// Screen rect of card `i` (without its drag offset).
    pub fn card_rect(&self, i: usize) -> Rect {
        let g = self.geo();
        Rect::new(
            g.left + i as i32 * (g.card_w + GAP) - self.scroll,
            g.top,
            g.card_w,
            g.card_h,
        )
    }

    /// The close button inside a card rect.
    pub fn close_rect(&self, card: Rect) -> Rect {
        let g = self.geo();
        Rect::new(
            card.right() - PAD - CLOSE_BTN,
            card.y + PAD + g.thumb_h + (LABEL_H - CLOSE_BTN) / 2,
            CLOSE_BTN,
            CLOSE_BTN,
        )
    }

    fn visible_rect(&self, i: usize) -> Rect {
        let r = self.card_rect(i);
        Rect::new(r.x, r.y + self.offset[i], r.w, r.h)
    }

    /// The card under a point (cards with an answer pending are gone as far as touch goes).
    pub fn card_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.cards.len())
            .filter(|i| self.closing[*i].is_none())
            .find(|i| self.visible_rect(*i).contains(x, y))
    }

    pub fn on_gesture(&mut self, g: &Gesture, now: Ms) -> Vec<Action> {
        match *g {
            Gesture::Pressed { x, y } => self.pressed = self.card_at(x, y),
            Gesture::Cancel => self.pressed = None,
            Gesture::Tap { x, y } => {
                self.pressed = None;
                return match self.card_at(x, y) {
                    Some(i) => {
                        let card = self.visible_rect(i);
                        if self.close_rect(card).contains(x, y) {
                            self.start_close(i, now)
                        } else {
                            vec![Action::Focus(self.cards[i].app.clone())]
                        }
                    }
                    None => vec![Action::Dismiss],
                };
            }
            Gesture::LongPress { .. } => self.pressed = None,
            Gesture::DragStart { x, y } => {
                self.pressed = None;
                self.drag = Drag::Start(self.card_at(x, y));
            }
            Gesture::Drag { dx, dy } => {
                if let Drag::Start(card) = self.drag {
                    self.drag = match card {
                        Some(i) if dy.abs() > dx.abs() => Drag::Vertical(i),
                        _ => Drag::Horizontal,
                    };
                }
                match self.drag {
                    Drag::Vertical(i) => self.offset[i] = (self.offset[i] + dy).min(0),
                    Drag::Horizontal => {
                        let max = self.geo().max_scroll;
                        self.scroll = (self.scroll - dx).clamp(0, max);
                    }
                    _ => {}
                }
            }
            Gesture::DragEnd => {
                let d = std::mem::replace(&mut self.drag, Drag::None);
                if let Drag::Vertical(i) = d {
                    if self.offset[i] <= -CLOSE_DRAG_PX {
                        return self.start_close(i, now);
                    }
                    self.offset[i] = 0;
                }
            }
        }
        Vec::new()
    }

    fn start_close(&mut self, i: usize, now: Ms) -> Vec<Action> {
        self.closing[i] = Some(now);
        vec![Action::Close(self.cards[i].app.clone())]
    }

    /// The core answered the close request: the card goes away (`ok`) or comes back.
    pub fn close_result(&mut self, app: &str, ok: bool) {
        let Some(i) = self.cards.iter().position(|c| c.app == app) else {
            return;
        };
        if ok {
            self.cards.remove(i);
            self.offset.remove(i);
            self.closing.remove(i);
            self.rescale();
        } else {
            self.closing[i] = None;
            self.offset[i] = 0;
        }
    }

    /// Put back the cards whose close request is overdue; returns their app ids.
    pub fn tick(&mut self, now: Ms) -> Vec<String> {
        let mut back = Vec::new();
        for i in 0..self.cards.len() {
            if self.closing[i].is_some_and(|t| now.saturating_sub(t) >= CLOSE_TIMEOUT_MS) {
                self.closing[i] = None;
                self.offset[i] = 0;
                back.push(self.cards[i].app.clone());
            }
        }
        back
    }

    pub fn next_deadline(&self) -> Option<Ms> {
        self.closing
            .iter()
            .flatten()
            .map(|t| t + CLOSE_TIMEOUT_MS)
            .min()
    }
}
