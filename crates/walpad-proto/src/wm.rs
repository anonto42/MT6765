//! Window geometry (change `multitasking-windows`): pure functions the core and the shell share.
//! The core is the authority (it clamps every request again); the shell uses the same functions
//! so a drag previews what the core will accept.
use serde::{Deserialize, Serialize};

/// Smallest size a window may get (px).
pub const MIN_W: i32 = 320;
pub const MIN_H: i32 = 240;
/// Space between the two tiles of a split.
pub const DIVIDER: i32 = 4;
/// Split ratios offered by the UI (percent of the first tile).
pub const SPLIT_RATIOS: [u8; 3] = [33, 50, 67];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl WRect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        WRect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    /// Every window maximised (the behaviour before this change).
    #[default]
    Single,
    /// The two topmost app windows tile the work area.
    Split,
    /// Floating windows, moved and resized by touch.
    Free,
}

impl LayoutMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutMode::Single => "single",
            LayoutMode::Split => "split",
            LayoutMode::Free => "free",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "single" => Some(LayoutMode::Single),
            "split" => Some(LayoutMode::Split),
            "free" => Some(LayoutMode::Free),
            _ => None,
        }
    }
    /// The next mode when the Settings button is tapped.
    pub fn next(self) -> Self {
        match self {
            LayoutMode::Single => LayoutMode::Split,
            LayoutMode::Split => LayoutMode::Free,
            LayoutMode::Free => LayoutMode::Single,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapZone {
    Left,
    Right,
    Top,
    Bottom,
    Full,
}

/// The next split ratio (33 -> 50 -> 67 -> 33); anything else goes to 33.
pub fn next_ratio(r: u8) -> u8 {
    match r {
        33 => 50,
        50 => 67,
        _ => 33,
    }
}

/// A ratio the core accepts (the offered ones only).
pub fn valid_ratio(r: u8) -> bool {
    SPLIT_RATIOS.contains(&r)
}

/// The visible panel for an orientation in degrees (CONTRACTS: 800x1280 portrait, 1280x800
/// landscape); the fallback when no strut information is available.
pub fn panel(orientation: u16) -> WRect {
    if matches!(orientation, 90 | 270) {
        WRect::new(0, 0, 1280, 800)
    } else {
        WRect::new(0, 0, 800, 1280)
    }
}

/// Work area of a screen: the screen minus the largest strut on each side
/// (`[left, right, top, bottom]` as in `_NET_WM_STRUT`). Never smaller than the minimum size.
pub fn workarea(screen: (i32, i32), struts: &[[u32; 4]]) -> WRect {
    let m = |i: usize| struts.iter().map(|s| s[i] as i32).max().unwrap_or(0);
    let (l, r, t, b) = (m(0), m(1), m(2), m(3));
    let w = (screen.0 - l - r).max(MIN_W.min(screen.0));
    let h = (screen.1 - t - b).max(MIN_H.min(screen.1));
    WRect::new(l.min(screen.0 - w), t.min(screen.1 - h), w, h)
}

/// Two tiles: side by side when the area is landscape, stacked when portrait. `ratio` is the
/// percent of the first tile (left or top); `swap` exchanges the tiles.
pub fn tile_split(area: WRect, ratio: u8, swap: bool) -> [WRect; 2] {
    let ratio = if valid_ratio(ratio) { ratio } else { 50 } as i32;
    let (a, b) = if area.w > area.h {
        let w1 = (area.w - DIVIDER) * ratio / 100;
        (
            WRect::new(area.x, area.y, w1, area.h),
            WRect::new(area.x + w1 + DIVIDER, area.y, area.w - w1 - DIVIDER, area.h),
        )
    } else {
        let h1 = (area.h - DIVIDER) * ratio / 100;
        (
            WRect::new(area.x, area.y, area.w, h1),
            WRect::new(area.x, area.y + h1 + DIVIDER, area.w, area.h - h1 - DIVIDER),
        )
    };
    if swap {
        [b, a]
    } else {
        [a, b]
    }
}

/// Where a window goes for a snap zone (halves of the work area; Full = all of it).
pub fn snap_rect(area: WRect, zone: SnapZone) -> WRect {
    let (hw, hh) = ((area.w - DIVIDER) / 2, (area.h - DIVIDER) / 2);
    match zone {
        SnapZone::Full => area,
        SnapZone::Left => WRect::new(area.x, area.y, hw, area.h),
        SnapZone::Right => WRect::new(area.right() - hw, area.y, hw, area.h),
        SnapZone::Top => WRect::new(area.x, area.y, area.w, hh),
        SnapZone::Bottom => WRect::new(area.x, area.bottom() - hh, area.w, hh),
    }
}

/// Keep a rectangle inside the area and at least the minimum size (or the whole area when the
/// area itself is smaller).
pub fn clamp_rect(r: WRect, area: WRect) -> WRect {
    let w = r.w.max(MIN_W).min(area.w);
    let h = r.h.max(MIN_H).min(area.h);
    let x = r.x.max(area.x).min(area.right() - w);
    let y = r.y.max(area.y).min(area.bottom() - h);
    WRect::new(x, y, w, h)
}

/// A title-bar drag: the start rectangle moved by (dx, dy), kept inside the area.
pub fn move_rect(start: WRect, dx: i32, dy: i32, area: WRect) -> WRect {
    clamp_rect(
        WRect::new(start.x + dx, start.y + dy, start.w, start.h),
        area,
    )
}

/// A corner-grip drag (bottom-right corner): the top-left corner stays, the size changes.
pub fn resize_rect(start: WRect, dx: i32, dy: i32, area: WRect) -> WRect {
    clamp_rect(
        WRect::new(start.x, start.y, start.w + dx, start.h + dy),
        area,
    )
}

/// The first floating geometry of the `index`-th window: 70 % of the area, cascaded by 48 px.
pub fn cascade(area: WRect, index: usize) -> WRect {
    let step = 48 * (index % 5) as i32;
    clamp_rect(
        WRect::new(
            area.x + 24 + step,
            area.y + 24 + step,
            area.w * 7 / 10,
            area.h * 7 / 10,
        ),
        area,
    )
}

/// Which window sits in which tile. `prev` is the last assignment, `top_first` the candidate
/// windows from the front of the stack backwards. The two frontmost windows are the pair; a
/// window that already has a tile keeps it, a newcomer takes a free tile.
pub fn assign_slots(prev: [Option<u32>; 2], top_first: &[u32]) -> [Option<u32>; 2] {
    let pair: Vec<u32> = top_first.iter().copied().take(2).collect();
    let mut out = [None, None];
    for (i, p) in prev.iter().enumerate() {
        if let Some(w) = p.filter(|w| pair.contains(w)) {
            out[i] = Some(w);
        }
    }
    for w in &pair {
        if out.contains(&Some(*w)) {
            continue;
        }
        if let Some(slot) = out.iter().position(|s| s.is_none()) {
            out[slot] = Some(*w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
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
}
