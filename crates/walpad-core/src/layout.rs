//! Window layout engine (change multitasking-windows): decides, from the app windows on screen and
//! the work area, which windows to un-maximise, maximise and place. Pure: the daemon feeds it
//! the X facts and applies the returned operations (`xbridge`). Geometry math is
//! `walpad_proto::wm`.
use std::collections::{HashMap, HashSet};
use walpad_proto::wm::{self, LayoutMode, WRect};

/// What the engine knows about one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinView {
    pub id: u32,
    pub rect: WRect,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutOp {
    Maximize(u32),
    Unmaximize(u32),
    Place(u32, WRect),
}

pub struct Layout {
    pub mode: LayoutMode,
    pub ratio: u8,
    pub swap: bool,
    /// Which window sits in which tile of a split.
    slots: [Option<u32>; 2],
    /// Windows this engine un-maximised (to be maximised again in single mode).
    managed: HashSet<u32>,
    /// The last rectangle sent for a window (never send the same one twice).
    sent: HashMap<u32, WRect>,
    /// Mode and area of the last plan, to notice changes.
    last: Option<(LayoutMode, WRect)>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            mode: LayoutMode::Single,
            ratio: 50,
            swap: false,
            slots: [None, None],
            managed: HashSet::new(),
            sent: HashMap::new(),
            last: None,
        }
    }
}

impl Layout {
    pub fn new(mode: LayoutMode, ratio: u8, swap: bool) -> Self {
        Layout {
            mode,
            ratio: if wm::valid_ratio(ratio) { ratio } else { 50 },
            swap,
            ..Layout::default()
        }
    }

    /// Anything left to put back (single mode still has windows to maximise)?
    pub fn busy(&self) -> bool {
        self.mode != LayoutMode::Single || !self.managed.is_empty()
    }

    /// Forget what was sent (a setting changed: send everything again).
    pub fn invalidate(&mut self) {
        self.sent.clear();
    }

    /// A window the owner placed by hand (free mode): remember it as managed and sent.
    pub fn placed(&mut self, id: u32, r: WRect) {
        self.managed.insert(id);
        self.sent.insert(id, r);
    }

    /// Operations that bring the windows (back to front) in line with the mode.
    pub fn plan(&mut self, wins: &[WinView], area: WRect) -> Vec<LayoutOp> {
        let live: HashSet<u32> = wins.iter().map(|w| w.id).collect();
        self.managed.retain(|w| live.contains(w));
        self.sent.retain(|w, _| live.contains(w));
        for s in self.slots.iter_mut() {
            if s.is_some_and(|w| !live.contains(&w)) {
                *s = None;
            }
        }
        if self.last.is_some_and(|(m, a)| m != self.mode || a != area) {
            // a new mode or a new work area (rotation, keyboard): everything is placed again
            self.sent.clear();
        }
        self.last = Some((self.mode, area));
        let mut ops = Vec::new();
        match self.mode {
            LayoutMode::Single => {
                let mut back: Vec<u32> = self.managed.drain().collect();
                back.sort_unstable();
                for id in back {
                    ops.push(LayoutOp::Maximize(id));
                }
                self.sent.clear();
                self.slots = [None, None];
            }
            LayoutMode::Split => {
                let top_first: Vec<u32> = wins.iter().rev().map(|w| w.id).collect();
                self.slots = wm::assign_slots(self.slots, &top_first);
                let tiles = wm::tile_split(area, self.ratio, self.swap);
                let filled = self.slots.iter().flatten().count();
                for (i, slot) in self.slots.clone().iter().enumerate() {
                    let Some(id) = slot else { continue };
                    // a lone window takes the whole work area
                    let want = if filled == 1 { area } else { tiles[i] };
                    if filled == 1 && wins.iter().any(|w| w.id == *id && w.maximized) {
                        continue; // already fills the work area: leave it maximised
                    }
                    self.place(wins, *id, want, &mut ops);
                }
            }
            LayoutMode::Free => {
                for w in wins {
                    if !self.managed.contains(&w.id) {
                        // first time in free mode: a floating, cascaded window
                        let want = wm::cascade(area, self.managed.len());
                        self.place(wins, w.id, want, &mut ops);
                    } else {
                        // keep it inside the work area (rotation, keyboard, strips)
                        let want = wm::clamp_rect(w.rect, area);
                        if want != w.rect {
                            self.place(wins, w.id, want, &mut ops);
                        }
                    }
                }
            }
        }
        ops
    }

    fn place(&mut self, wins: &[WinView], id: u32, want: WRect, ops: &mut Vec<LayoutOp>) {
        let Some(w) = wins.iter().find(|w| w.id == id) else {
            return;
        };
        if w.maximized {
            ops.push(LayoutOp::Unmaximize(id));
        }
        self.managed.insert(id);
        if self.sent.get(&id) != Some(&want) && (w.rect != want || w.maximized) {
            ops.push(LayoutOp::Place(id, want));
        }
        self.sent.insert(id, want);
    }
}

#[cfg(test)]
mod tests {
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
}
