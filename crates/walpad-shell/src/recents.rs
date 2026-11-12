//! Recents overview (`app-switcher-navigation`): one card per running app (thumbnail + name),
//! most recent first, over the whole visible area. Tap a card = focus, swipe a card up = close,
//! drag sideways = scroll, tap the empty background = dismiss. Pure state + drawing; the X side
//! (window, thumbnails, focusing) is `nav.rs`.
use crate::gesture::{Gesture, Ms};
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Icon, Rect, Style};

/// Gap between cards and to the screen edge.
pub const GAP: i32 = SP_5;
/// Card padding around the thumbnail.
pub const PAD: i32 = SP_2;
/// Label strip under the thumbnail (icon, name, close button).
pub const LABEL_H: i32 = 56;
/// Hit size of the close button in the label strip.
pub const CLOSE_BTN: i32 = 48;
/// A card dragged up this far (px) and released is closed.
pub const CLOSE_DRAG_PX: i32 = 96;
/// A close request that gets no answer in this long puts the card back.
pub const CLOSE_TIMEOUT_MS: Ms = 3000;
/// Cards that fit side by side before the strip scrolls.
pub const FIT: i32 = 3;

pub struct Card {
    pub app: String,
    pub name: String,
    pub icon: Icon,
    /// Last window capture (None: the icon is drawn instead).
    pub thumb: Option<Canvas>,
}

/// What the strip asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Focus this app and close the strip.
    Focus(String),
    /// Send `close_app` for this app (the card is hidden until the answer comes).
    Close(String),
    /// Close the strip.
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub card_w: i32,
    pub thumb_w: i32,
    pub thumb_h: i32,
    pub card_h: i32,
    pub top: i32,
    /// x of the first card at scroll 0 (centres the row when it fits).
    pub left: i32,
    pub content_w: i32,
    pub max_scroll: i32,
}

/// Card sizes for `n` cards on a visible area: up to three side by side (one or two get larger
/// cards, never wider than half the screen), the thumbnail keeps the screen's aspect.
pub fn geometry(area: (i32, i32), n: usize) -> Geometry {
    let n = n.max(1) as i32;
    let slots = n.min(FIT);
    let card_w = ((area.0 - (slots + 1) * GAP) / slots).min(area.0 / 2);
    let thumb_w = card_w - 2 * PAD;
    let thumb_h = thumb_w * area.1 / area.0;
    let card_h = PAD + thumb_h + LABEL_H;
    let content_w = n * card_w + (n + 1) * GAP;
    let max_scroll = (content_w - area.0).max(0);
    Geometry {
        card_w,
        thumb_w,
        thumb_h,
        card_h,
        top: ((area.1 - card_h) / 2 - SP_5).max(SP_5),
        left: ((area.0 - content_w) / 2).max(0) + GAP,
        content_w,
        max_scroll,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    None,
    /// Undecided: the first movement picks the axis.
    Start(Option<usize>),
    Vertical(usize),
    Horizontal,
}

pub struct Recents {
    area: (i32, i32),
    cards: Vec<Card>,
    scaled: Vec<Option<Canvas>>,
    /// Vertical drag offset per card (<= 0).
    offset: Vec<i32>,
    /// A close request is out for this card (since).
    closing: Vec<Option<Ms>>,
    scroll: i32,
    drag: Drag,
    pressed: Option<usize>,
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

    /// Draw the whole strip (`p.cv` is the visible area).
    pub fn render(&self, p: &mut Painter) {
        let (w, h) = self.area;
        p.cv.rect(Rect::new(0, 0, w, h), SURFACE_STRONG.over(BG));
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
            Style::new(Face::Regular, SIZE_CAPTION, TEXT_SECONDARY),
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
                p.cv.rect(thumb, SURFACE_ICON.over(BG));
                let big = (g.thumb_w / 3).min(96);
                let c = thumb.center();
                p.cv.icon(
                    self.cards[i].icon,
                    c.0 - big / 2,
                    c.1 - big / 2,
                    big,
                    TEXT_SECONDARY,
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
            TEXT,
        );
        p.text(
            &self.cards[i].name,
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT),
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
            TEXT_SECONDARY,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: (i32, i32) = (800, 1280);
    const L: (i32, i32) = (1280, 800);

    fn cards(n: usize) -> Vec<Card> {
        ["browser", "terminal", "files", "kali", "settings"]
            .iter()
            .take(n)
            .map(|id| Card {
                app: id.to_string(),
                name: id.to_string(),
                icon: Icon::Apps,
                thumb: None,
            })
            .collect()
    }

    fn tap_at(r: &mut Recents, x: i32, y: i32) -> Vec<Action> {
        r.on_gesture(&Gesture::Pressed { x, y }, 0);
        r.on_gesture(&Gesture::Tap { x, y }, 50)
    }

    fn drag(r: &mut Recents, from: (i32, i32), dx: i32, dy: i32, now: Ms) -> Vec<Action> {
        r.on_gesture(
            &Gesture::Pressed {
                x: from.0,
                y: from.1,
            },
            0,
        );
        r.on_gesture(&Gesture::Cancel, 1);
        r.on_gesture(
            &Gesture::DragStart {
                x: from.0,
                y: from.1,
            },
            2,
        );
        r.on_gesture(&Gesture::Drag { dx, dy }, 3);
        r.on_gesture(&Gesture::DragEnd, now)
    }

    #[test]
    fn three_cards_fit_in_both_orientations_and_keep_the_screen_aspect() {
        for area in [P, L] {
            let g = geometry(area, 3);
            assert_eq!(g.max_scroll, 0, "{area:?}");
            assert!(g.content_w <= area.0);
            assert_eq!(g.thumb_h, g.thumb_w * area.1 / area.0);
            assert!(g.top + g.card_h < area.1, "fits vertically {area:?}");
        }
        assert!(
            geometry(P, 1).card_w <= 400,
            "one card is capped at half the screen"
        );
        assert!(geometry(P, 5).max_scroll > 0, "more than three scroll");
    }

    #[test]
    fn cards_are_laid_out_in_order_left_to_right() {
        let r = Recents::new(P, cards(3));
        let (a, b, c) = (r.card_rect(0), r.card_rect(1), r.card_rect(2));
        assert!(a.right() < b.x && b.right() < c.x);
        assert_eq!(a.y, b.y);
        assert_eq!(
            r.apps(),
            vec!["browser", "terminal", "files"],
            "most recent first"
        );
    }

    #[test]
    fn tapping_a_card_focuses_it_and_the_background_dismisses() {
        let mut r = Recents::new(P, cards(3));
        let c = r.card_rect(1);
        assert_eq!(
            tap_at(&mut r, c.x + 40, c.y + 40),
            vec![Action::Focus("terminal".into())]
        );
        assert_eq!(tap_at(&mut r, 5, 5), vec![Action::Dismiss]);
        let gap_x = r.card_rect(0).right() + 3;
        assert_eq!(tap_at(&mut r, gap_x, c.y + 40), vec![Action::Dismiss]);
    }

    #[test]
    fn the_close_button_closes_without_focusing() {
        let mut r = Recents::new(L, cards(2));
        let c = r.card_rect(0);
        let b = r.close_rect(c);
        assert!(c.contains(b.x, b.y) && c.contains(b.right() - 1, b.bottom() - 1));
        assert_eq!(
            tap_at(&mut r, b.x + 4, b.y + 4),
            vec![Action::Close("browser".into())]
        );
        assert!(r.is_closing("browser"));
        // a card waiting for its answer takes no touches
        assert_eq!(tap_at(&mut r, c.x + 40, c.y + 40), vec![Action::Dismiss]);
    }

    #[test]
    fn swiping_a_card_up_far_enough_closes_it_and_the_answer_removes_it() {
        let mut r = Recents::new(P, cards(3));
        let c = r.card_rect(0);
        let acts = drag(&mut r, (c.x + 60, c.y + 200), 0, -CLOSE_DRAG_PX - 20, 100);
        assert_eq!(acts, vec![Action::Close("browser".into())]);
        assert_eq!(r.len(), 3, "kept until the core answers");
        r.close_result("browser", true);
        assert_eq!(r.apps(), vec!["terminal", "files"]);
        // the remaining cards were laid out again for two
        assert_eq!(r.card_rect(0).w, geometry(P, 2).card_w);
    }

    #[test]
    fn a_short_upward_drag_springs_back() {
        let mut r = Recents::new(P, cards(3));
        let c = r.card_rect(1);
        let acts = drag(&mut r, (c.x + 60, c.y + 200), 0, -40, 100);
        assert!(acts.is_empty());
        assert_eq!(r.visible_rect(1), r.card_rect(1));
        assert!(!r.is_closing("terminal"));
    }

    #[test]
    fn a_refused_or_unanswered_close_brings_the_card_back() {
        let mut r = Recents::new(P, cards(3));
        let c = r.card_rect(0);
        drag(&mut r, (c.x + 60, c.y + 200), 0, -150, 100);
        r.close_result("browser", false);
        assert_eq!(r.len(), 3);
        assert!(!r.is_closing("browser"));
        assert_eq!(r.visible_rect(0), r.card_rect(0));
        // no answer at all
        drag(&mut r, (c.x + 60, c.y + 200), 0, -150, 200);
        assert_eq!(r.next_deadline(), Some(200 + CLOSE_TIMEOUT_MS));
        assert!(r.tick(200 + CLOSE_TIMEOUT_MS - 1).is_empty());
        assert_eq!(r.tick(200 + CLOSE_TIMEOUT_MS), vec!["browser".to_string()]);
        assert!(!r.is_closing("browser"));
        assert_eq!(r.next_deadline(), None);
    }

    #[test]
    fn sideways_drag_scrolls_a_long_strip_within_bounds() {
        let mut r = Recents::new(P, cards(5));
        let max = geometry(P, 5).max_scroll;
        let c = r.card_rect(0);
        assert!(drag(&mut r, (c.x + 60, c.y + 200), -150, 10, 100).is_empty());
        assert_eq!(r.scroll(), 150);
        drag(&mut r, (c.x + 60, c.y + 200), -5000, 0, 100);
        assert_eq!(r.scroll(), max);
        drag(&mut r, (c.x + 60, c.y + 200), 9000, 0, 100);
        assert_eq!(r.scroll(), 0);
        // nothing moved vertically
        assert_eq!(r.visible_rect(0), r.card_rect(0));
    }

    #[test]
    fn empty_strip_dismisses_on_any_tap() {
        let mut r = Recents::new(P, Vec::new());
        assert!(r.is_empty());
        assert_eq!(tap_at(&mut r, 400, 600), vec![Action::Dismiss]);
    }

    #[test]
    fn thumbnails_are_scaled_to_the_card() {
        let t = Canvas::from_bgrx_scaled(8, 8, &[200u8; 8 * 8 * 4], 8, 8);
        let mut cs = cards(1);
        cs[0].thumb = t;
        let r = Recents::new(P, cs);
        let g = geometry(P, 1);
        let s = r.scaled[0].as_ref();
        assert_eq!(
            s.map(|c| (c.width(), c.height())),
            Some((g.thumb_w, g.thumb_h))
        );
    }
}
