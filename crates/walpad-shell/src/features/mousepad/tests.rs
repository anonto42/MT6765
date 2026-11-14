use super::layout::*;
use super::marker::*;
use super::state::*;
use super::*;
use crate::keyboard::keyboard_rect;
use walpad_ui::tokens::{BG, TEXT, TOUCH_MIN};
use walpad_ui::{Color, Rect};

const P: (i32, i32) = (800, 1280);
const L: (i32, i32) = (1280, 800);

/// A point on the touch surface (root coordinates).
fn on_surface(area: (i32, i32)) -> (i32, i32) {
    let r = pad_rect(area);
    let s = pad_layout(area).surface;
    (r.x + s.x + 100, r.y + s.y + 60)
}

fn on_button(area: (i32, i32), b: PadButton) -> (i32, i32) {
    let r = pad_rect(area);
    let (c, _) = pad_layout(area)
        .buttons
        .into_iter()
        .find(|(_, x)| *x == b)
        .unwrap();
    (r.x + c.center().0, r.y + c.center().1)
}

#[test]
fn same_place_as_the_keyboard_with_a_row_of_big_buttons() {
    for area in [P, L] {
        let r = pad_rect(area);
        assert_eq!(r, keyboard_rect(area));
        assert_eq!(cursor_bounds(area), Rect::new(0, 0, area.0, r.y));
        let l = pad_layout(area);
        assert_eq!(l.buttons.len(), 5);
        for (b, _) in &l.buttons {
            assert!(b.h >= TOUCH_MIN && b.w >= 100, "{b:?}");
            assert!(b.bottom() <= r.h && b.right() <= r.w);
            assert!(b.y > l.surface.bottom());
        }
        assert_eq!(l.buttons.last().unwrap().0.right(), r.w - PAD);
        assert!(l.surface.h >= 150 && l.surface.w > 700);
        // the mini-map keeps the visible area's aspect and sits inside the surface
        assert!(l.surface.contains(l.map.x, l.map.y) && l.map.right() < l.surface.right());
        assert!(((l.map.w * area.1) - (l.map.h * area.0)).abs() <= area.1);
    }
}

#[test]
fn starts_in_the_middle_of_the_area_above_the_pad() {
    assert_eq!(PadState::new(P).cursor, (400, 448));
    assert_eq!(PadState::new(L).cursor, (640, 280));
}

/// Drag the finger from `from` by (dx, dy) in steps of at most 40 px (a real finger's
/// sample spacing), returning the end point.
fn drag(s: &mut PadState, from: (i32, i32), dx: i32, dy: i32) -> (i32, i32) {
    let n = (dx.abs().max(dy.abs()) + 39) / 40;
    let mut p = from;
    for i in 1..=n.max(1) {
        p = (from.0 + dx * i / n.max(1), from.1 + dy * i / n.max(1));
        s.motion(p.0, p.1);
    }
    p
}

#[test]
fn slop_then_the_whole_travel_counts_smoothed() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    let c0 = s.cursor;
    s.press(x, y, 1000);
    // within the slop: nothing moves yet
    assert!(!s.motion(x + 5, y + 5));
    assert!(!s.motion(x + 10, y));
    assert_eq!(s.cursor, c0);
    // past it: moves, low-passed (part of the 30 px now) ...
    assert!(s.motion(x + 20, y));
    let mid = s.cursor.0 - c0.0;
    assert!(mid > 0 && mid < 30, "smoothed step {mid}");
    // ... and the rest follows on the next samples / on release: the whole travel, x1.5
    assert!(s.motion(x + 20, y - 40));
    assert_eq!(s.release(1400), None, "a drag clicks nothing");
    assert_eq!(s.cursor, (c0.0 + 30, c0.1 - 60));
    // fractions carry: four 1 px steps move 6 px in total
    let before = s.cursor.0;
    s.press(x, y, 2000);
    drag(&mut s, (x, y), 12, 0);
    for i in 1..=4 {
        s.motion(x + 12 + i, y);
    }
    s.release(2500);
    assert_eq!(s.cursor.0, before + 24);
}

#[test]
fn single_sample_jumps_are_ignored() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    let c0 = s.cursor;
    s.press(x, y, 0);
    s.motion(x + 20, y);
    // one wild sample (noise, or an echo we did not recognise): no movement
    let before = s.cursor;
    assert!(!s.motion(x + 300, y - 200));
    assert_eq!(s.cursor, before);
    // the finger goes on where it was
    s.motion(x + 30, y);
    s.release(100);
    assert_eq!(s.cursor, (c0.0 + 45, c0.1), "only the real 30 px count");
    // noise inside the slop does not start a drag nor spoil the tap
    s.press(x, y, 1000);
    assert!(!s.motion(x - 150, y + 90));
    assert!(!s.motion(x + 3, y + 2));
    assert_eq!(s.release(1000 + 100 + 150), Some(PadOut::Click(1)));
    assert_eq!(s.cursor, (c0.0 + 45, c0.1));
}

#[test]
fn two_agreeing_jumps_re_anchor_without_moving() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    let c0 = s.cursor;
    s.press(x, y, 0);
    s.motion(x + 20, y);
    // the finger really is over there now (lifted and put down between reports): re-anchor
    assert!(!s.motion(x + 220, y));
    assert!(!s.motion(x + 222, y));
    s.motion(x + 232, y);
    s.release(500);
    // 20 px before the jump, 10 after the confirming sample; the jump itself never counts
    assert_eq!(s.cursor, (c0.0 + 45, c0.1));
}

#[test]
fn every_press_re_anchors_on_the_finger() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    s.press(x, y, 0);
    s.note_warp();
    drag(&mut s, (x, y), 100, 0);
    s.release(400);
    s.note_warp(); // pointer back on the cursor after the batch
    let c1 = s.cursor;
    // the next touch lands 300 px away from the last finger sample: no delta from there
    s.press(x + 400, y + 50, 2000);
    assert!(!s.motion(x + 405, y + 50));
    drag(&mut s, (x + 405, y + 50), -25, 0);
    s.release(2300);
    assert_eq!(s.cursor, (c1.0 - 30, c1.1), "only this touch's 20 px x1.5");
    // the late echo of the old warp (at the cursor) is recognised, not a finger sample
    s.press(x, y, 3000);
    assert!(!s.motion(c1.0, c1.1));
}

#[test]
fn cursor_is_clamped_to_the_visible_area_above_the_pad() {
    for area in [P, L] {
        let mut s = PadState::new(area);
        let (x, y) = on_surface(area);
        s.press(x, y, 0);
        let p = drag(&mut s, (x, y), -1600, -1600);
        assert_eq!(s.cursor, (0, 0));
        let p = drag(&mut s, p, 3000, 3000);
        let b = cursor_bounds(area);
        assert_eq!(s.cursor, (b.right() - 1, b.bottom() - 1));
        assert_eq!(s.cursor, (area.0 - 1, pad_rect(area).y - 1));
        // and back out of the corner right away (no backlog or fraction pushing outwards)
        s.motion(p.0 - 2, p.1);
        s.release(10_000);
        assert!(s.cursor.0 < area.0 - 1, "{:?}", s.cursor);
    }
}

#[test]
fn echoes_of_our_warps_are_ignored() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    s.press(x, y, 0);
    s.note_warp();
    let c0 = s.cursor;
    assert!(s.motion(x + 20, y)); // finger
    s.note_warp(); // pointer back on the moved cursor
    let c1 = s.cursor;
    // the first warp's echo arrives late (after the finger sample): no jump
    assert!(!s.motion(c0.0, c0.1));
    // the newest echo too
    assert!(!s.motion(c1.0, c1.1));
    // the finger again: delta from its own last position
    s.motion(x + 30, y);
    s.release(500);
    assert_eq!(s.cursor, (c0.0 + 45, c0.1));
}

#[test]
fn taps_tolerate_jitter_and_click_where_the_touch_began() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    let c0 = s.cursor;
    // the bridge releases 150 ms after the finger lifted
    s.press(x, y, 10_000);
    assert_eq!(s.release(10_000 + 120 + 150), Some(PadOut::Click(1)));
    // 15 px of jitter (past the 10 px slop, the cursor twitched): still a tap, and the click
    // lands where the cursor was when the finger went down
    s.press(x, y, 20_000);
    s.motion(x + 8, y - 6);
    s.motion(x + 15, y - 4);
    assert_ne!(s.cursor, c0);
    assert_eq!(s.release(20_000 + 250 + 150), Some(PadOut::Click(1)));
    assert_eq!(s.cursor, c0);
    // too slow for a tap, too short for a long press: nothing
    s.press(x, y, 30_000);
    assert_eq!(s.release(30_000 + 400 + 150), None);
    // a long press within the tolerance: right click at the start position
    s.press(x, y, 40_000);
    s.motion(x + 12, y + 5);
    assert_eq!(s.release(40_000 + 600 + 150), Some(PadOut::Click(3)));
    assert_eq!(s.cursor, c0);
    // 20 px or more is a drag, however quick
    s.press(x, y, 50_000);
    s.motion(x + 10, y);
    s.motion(x + 22, y);
    assert_eq!(s.release(50_000 + 100), None);
    assert_ne!(s.cursor, c0);
    // X time wraps around
    s.press(x, y, u32::MAX - 50);
    assert_eq!(s.release(100), Some(PadOut::Click(1)));
    // a release without a press does nothing
    assert_eq!(s.release(5), None);
}

#[test]
fn marker_shape_is_the_arrow_with_the_tip_at_the_origin() {
    let shape = marker_shape();
    let (w, h) = marker_size();
    assert_eq!((w, h), (24, 38));
    assert!(shape.iter().any(|r| r.contains(0, 0)), "hotspot is drawn");
    assert!(shape.iter().all(|r| r.right() <= w && r.bottom() <= h));
    // the region is not the whole box (a rectangle would hide what is under it)
    let area: i32 = shape.iter().map(|r| r.w * r.h).sum();
    assert!(area < w * h * 2 / 3, "{area}");
    assert!(
        !shape.iter().any(|r| r.contains(w - 1, 0)),
        "top-right is see-through"
    );
    let mut cv = Canvas::new(w as u32, h as u32, BG).unwrap();
    paint_marker(&mut cv);
    assert_eq!(cv.pixel(0, 0), Some(Color::hex(0)), "black edge at the tip");
    assert_eq!(cv.pixel(3, 6), Some(TEXT), "white body");
}

#[test]
fn button_row() {
    let mut s = PadState::new(L);
    let c0 = s.cursor;
    for (b, out) in [
        (PadButton::Left, PadOut::Click(1)),
        (PadButton::Right, PadOut::Click(3)),
        (PadButton::ScrollUp, PadOut::Click(4)),
        (PadButton::ScrollDown, PadOut::Click(5)),
        (PadButton::Close, PadOut::Hide),
    ] {
        let (x, y) = on_button(L, b);
        s.press(x, y, 0);
        assert_eq!(s.pressed_button(), Some(b));
        // motion on a button never moves the cursor; a long hold is still that button
        assert!(!s.motion(x + 3, y + 2));
        assert_eq!(s.release(2000), Some(out), "{b:?}");
        assert_eq!(s.pressed_button(), None);
    }
    assert_eq!(s.cursor, c0);
    // sliding off the button cancels it
    let (x, y) = on_button(L, PadButton::Left);
    s.press(x, y, 0);
    s.motion(x, y - 100);
    assert_eq!(s.release(100), None);
}

#[test]
fn rotation_recentres_and_forgets_the_touch() {
    let mut s = PadState::new(P);
    let (x, y) = on_surface(P);
    s.press(x, y, 0);
    s.motion(x + 100, y);
    s.set_area(L);
    assert_eq!(s.cursor, (640, 280));
    assert_eq!(s.release(10), None);
}
