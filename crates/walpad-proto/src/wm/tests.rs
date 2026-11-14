use super::*;

const L: WRect = WRect::new(0, 0, 1280, 800);
const P: WRect = WRect::new(0, 0, 800, 1280);

#[test]
fn workarea_subtracts_the_largest_strut_per_side() {
    // the rotation strut hides the unused part of the 1280x1280 screen; the dock and the
    // window bar add their own
    let a = workarea(
        (1280, 1280),
        &[[0, 0, 0, 480], [0, 0, 0, 56], [0, 0, 44, 0]],
    );
    assert_eq!(a, WRect::new(0, 44, 1280, 1280 - 480 - 44));
    let a = workarea((1280, 1280), &[[0, 480, 0, 0]]);
    assert_eq!(a, WRect::new(0, 0, 800, 1280));
    assert_eq!(workarea((1280, 1280), &[]), WRect::new(0, 0, 1280, 1280));
}

#[test]
fn workarea_never_collapses() {
    let a = workarea((800, 1280), &[[0, 0, 0, 5000]]);
    assert!(a.h >= MIN_H && a.bottom() <= 1280);
}

#[test]
fn split_is_side_by_side_in_landscape_and_stacked_in_portrait() {
    let [a, b] = tile_split(L, 50, false);
    assert_eq!((a.y, a.h, b.y, b.h), (0, 800, 0, 800));
    assert_eq!(a.right() + DIVIDER, b.x);
    assert_eq!(b.right(), 1280);
    let [a, b] = tile_split(P, 50, false);
    assert_eq!((a.x, a.w, b.x, b.w), (0, 800, 0, 800));
    assert_eq!(a.bottom() + DIVIDER, b.y);
    assert_eq!(b.bottom(), 1280);
}

#[test]
fn split_ratios_and_swap() {
    let [a, b] = tile_split(L, 33, false);
    assert!(a.w < b.w);
    assert_eq!(a.w, (1280 - DIVIDER) * 33 / 100);
    let [a2, b2] = tile_split(L, 67, true);
    assert!(a2.w < b2.w, "swap puts the 2/3 tile second");
    assert_eq!(b2.w, (1280 - DIVIDER) * 67 / 100);
    // an unknown ratio counts as 50
    assert_eq!(tile_split(L, 10, false), tile_split(L, 50, false));
}

#[test]
fn snap_zones() {
    assert_eq!(snap_rect(L, SnapZone::Full), L);
    let l = snap_rect(L, SnapZone::Left);
    let r = snap_rect(L, SnapZone::Right);
    assert_eq!((l.x, l.h, r.right(), r.h), (0, 800, 1280, 800));
    assert!(l.right() < r.x);
    let t = snap_rect(P, SnapZone::Top);
    let b = snap_rect(P, SnapZone::Bottom);
    assert_eq!((t.y, t.w, b.bottom(), b.w), (0, 800, 1280, 800));
    assert!(t.bottom() < b.y);
}

#[test]
fn clamp_keeps_inside_and_enforces_the_minimum() {
    let a = WRect::new(0, 44, 800, 1000);
    assert_eq!(
        clamp_rect(WRect::new(-50, 0, 400, 300), a),
        WRect::new(0, 44, 400, 300)
    );
    assert_eq!(
        clamp_rect(WRect::new(700, 1000, 400, 300), a),
        WRect::new(400, 744, 400, 300)
    );
    let r = clamp_rect(WRect::new(10, 100, 10, 10), a);
    assert_eq!((r.w, r.h), (MIN_W, MIN_H));
    // bigger than the area: shrinks to it
    assert_eq!(clamp_rect(WRect::new(0, 0, 5000, 5000), a), a);
}

#[test]
fn move_and_resize_are_clamped() {
    let s = WRect::new(100, 100, 500, 400);
    assert_eq!(move_rect(s, 50, -30, P), WRect::new(150, 70, 500, 400));
    assert_eq!(move_rect(s, 5000, 5000, P), WRect::new(300, 880, 500, 400));
    assert_eq!(move_rect(s, -5000, -5000, P), WRect::new(0, 0, 500, 400));
    assert_eq!(resize_rect(s, 100, 100, P), WRect::new(100, 100, 600, 500));
    assert_eq!(
        resize_rect(s, -900, -900, P),
        WRect::new(100, 100, MIN_W, MIN_H)
    );
    // cannot grow past the right/bottom edge
    let r = resize_rect(s, 9000, 9000, P);
    assert_eq!((r.right(), r.bottom()), (800, 1280));
}

#[test]
fn cascade_stays_inside_and_offsets() {
    for i in 0..8 {
        let r = cascade(L, i);
        assert_eq!(clamp_rect(r, L), r);
    }
    assert_ne!(cascade(L, 0), cascade(L, 1));
}

#[test]
fn slots_keep_sides_and_replace_the_one_that_left() {
    let s = assign_slots([None, None], &[5, 7, 9]);
    assert_eq!(s, [Some(5), Some(7)]);
    // focus moves to 7: same sides
    assert_eq!(assign_slots(s, &[7, 5]), [Some(5), Some(7)]);
    // a new window 11 in front: the one further back (7) is replaced, 5 keeps its side
    assert_eq!(assign_slots(s, &[11, 5, 7]), [Some(5), Some(11)]);
    // 5 closed: 7 keeps its side, 9 takes the free one
    assert_eq!(assign_slots(s, &[7, 9]), [Some(9), Some(7)]);
    // a single window
    assert_eq!(assign_slots(s, &[7]), [None, Some(7)]);
    assert_eq!(assign_slots(s, &[]), [None, None]);
}

#[test]
fn modes_cycle_and_parse() {
    assert_eq!(LayoutMode::Single.next().next().next(), LayoutMode::Single);
    assert_eq!(LayoutMode::parse("split"), Some(LayoutMode::Split));
    assert_eq!(LayoutMode::parse("grid"), None);
    assert_eq!(next_ratio(33), 50);
    assert_eq!(next_ratio(67), 33);
    assert_eq!(next_ratio(12), 33);
    assert_eq!(panel(90), L);
    assert_eq!(panel(0), P);
}
