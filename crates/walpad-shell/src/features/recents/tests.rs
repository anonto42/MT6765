use super::*;
use walpad_ui::Icon;

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
