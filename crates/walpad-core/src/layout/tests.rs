use super::*;

const L: WRect = WRect::new(0, 0, 1280, 800);
const P: WRect = WRect::new(0, 0, 800, 1280);

fn win(id: u32, rect: WRect, maximized: bool) -> WinView {
    WinView {
        id,
        rect,
        maximized,
    }
}
fn max(id: u32, area: WRect) -> WinView {
    win(id, area, true)
}
fn places(ops: &[LayoutOp]) -> Vec<(u32, WRect)> {
    ops.iter()
        .filter_map(|o| match o {
            LayoutOp::Place(i, r) => Some((*i, *r)),
            _ => None,
        })
        .collect()
}

#[test]
fn single_mode_does_nothing_to_maximised_windows() {
    let mut l = Layout::default();
    assert!(l.plan(&[max(1, L), max(2, L)], L).is_empty());
    assert!(!l.busy());
}

#[test]
fn split_tiles_the_two_frontmost_windows() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    // 3 is behind, 2 in the middle, 1 in front? back to front: 3, 2, 1
    let ops = l.plan(&[max(3, L), max(2, L), max(1, L)], L);
    let tiles = wm::tile_split(L, 50, false);
    assert!(ops.contains(&LayoutOp::Unmaximize(1)));
    assert!(ops.contains(&LayoutOp::Unmaximize(2)));
    assert!(!ops.contains(&LayoutOp::Unmaximize(3)));
    let p = places(&ops);
    assert_eq!(p.len(), 2);
    // 1 (front) got the first free slot after the order of assignment: 1 then 2
    assert!(p.contains(&(1, tiles[0])));
    assert!(p.contains(&(2, tiles[1])));
}

#[test]
fn split_is_idempotent_and_follows_rotation_and_strut_changes() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    let t = wm::tile_split(L, 50, false);
    l.plan(&[max(2, L), max(1, L)], L);
    let settled = [win(2, t[1], false), win(1, t[0], false)];
    assert!(l.plan(&settled, L).is_empty(), "nothing to do when placed");
    // rotation: portrait, stacked
    let ops = l.plan(&settled, P);
    let tp = wm::tile_split(P, 50, false);
    let p = places(&ops);
    assert!(p.contains(&(1, tp[0])) && p.contains(&(2, tp[1])), "{p:?}");
    // the keyboard comes up: the work area is shorter, the tiles follow
    let short = WRect::new(0, 0, 800, 900);
    let ts = wm::tile_split(short, 50, false);
    let p = places(&l.plan(&[win(2, tp[1], false), win(1, tp[0], false)], short));
    assert!(p.contains(&(1, ts[0])) && p.contains(&(2, ts[1])));
}

#[test]
fn a_lone_window_fills_the_work_area_and_a_second_one_takes_the_other_tile() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    // a lone maximised window is left alone
    assert!(l.plan(&[max(1, L)], L).is_empty());
    let t = wm::tile_split(L, 50, false);
    let ops = l.plan(&[win(1, L, false), max(2, L)], L);
    let p = places(&ops);
    // 1 keeps the first tile, the newcomer 2 takes the second
    assert!(p.contains(&(1, t[0])) && p.contains(&(2, t[1])), "{p:?}");
}

#[test]
fn ratio_and_swap_retile() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    l.plan(&[max(2, L), max(1, L)], L);
    l.ratio = 67;
    l.swap = true;
    let t = wm::tile_split(L, 67, true);
    let cur = [
        win(2, wm::tile_split(L, 50, false)[1], false),
        win(1, wm::tile_split(L, 50, false)[0], false),
    ];
    let p = places(&l.plan(&cur, L));
    // the tile each window holds is unchanged (slot 0 = window 1), but the tiles moved
    assert!(p.contains(&(1, t[0])) && p.contains(&(2, t[1])), "{p:?}");
}

#[test]
fn leaving_split_puts_windows_back_to_maximised() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    l.plan(&[max(2, L), max(1, L)], L);
    assert!(l.busy());
    l.mode = LayoutMode::Single;
    let ops = l.plan(&[win(2, L, false), win(1, L, false)], L);
    assert_eq!(ops, vec![LayoutOp::Maximize(1), LayoutOp::Maximize(2)]);
    assert!(!l.busy());
}

#[test]
fn free_mode_floats_new_windows_and_keeps_them_inside() {
    let mut l = Layout::new(LayoutMode::Free, 50, false);
    let ops = l.plan(&[max(1, L)], L);
    assert!(ops.contains(&LayoutOp::Unmaximize(1)));
    let p = places(&ops);
    assert_eq!(p, vec![(1, wm::cascade(L, 0))]);
    // a second window cascades further
    let ops = l.plan(&[win(1, wm::cascade(L, 0), false), max(2, L)], L);
    assert_eq!(places(&ops), vec![(2, wm::cascade(L, 1))]);
    // a window dragged out of the work area comes back
    let out = WRect::new(1200, 700, 500, 400);
    let ops = l.plan(&[win(1, out, false), win(2, wm::cascade(L, 1), false)], L);
    assert_eq!(places(&ops), vec![(1, wm::clamp_rect(out, L))]);
    // rotation: all windows are clamped into the new area
    let ops = l.plan(
        &[
            win(1, wm::clamp_rect(out, L), false),
            win(2, wm::cascade(L, 1), false),
        ],
        P,
    );
    for (_, r) in places(&ops) {
        assert_eq!(wm::clamp_rect(r, P), r);
    }
}

#[test]
fn closed_windows_are_forgotten() {
    let mut l = Layout::new(LayoutMode::Split, 50, false);
    l.plan(&[max(2, L), max(1, L)], L);
    l.plan(&[win(1, L, false)], L);
    l.mode = LayoutMode::Single;
    // only window 1 is still managed
    let ops = l.plan(&[win(1, L, false)], L);
    assert_eq!(ops, vec![LayoutOp::Maximize(1)]);
}

#[test]
fn hand_placed_windows_are_not_replaced() {
    let mut l = Layout::new(LayoutMode::Free, 50, false);
    let r = WRect::new(10, 10, 500, 400);
    l.placed(7, r);
    assert!(l.plan(&[win(7, r, false)], L).is_empty());
}
