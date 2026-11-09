//! Frame layout (docs/rust/UI.md): status bar, content, bottom navigation; portrait and landscape.
use crate::canvas::Rect;
use crate::tokens::{CONTENT_PAD, NAV_H, STATUS_BAR_H};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub w: i32,
    pub h: i32,
    pub status: Rect,
    /// Content area without side padding.
    pub content: Rect,
    pub nav: Rect,
}

impl Frame {
    pub fn new(w: i32, h: i32) -> Frame {
        Frame {
            w,
            h,
            status: Rect::new(0, 0, w, STATUS_BAR_H),
            content: Rect::new(0, STATUS_BAR_H, w, h - STATUS_BAR_H - NAV_H),
            nav: Rect::new(0, h - NAV_H, w, NAV_H),
        }
    }
    pub fn landscape(&self) -> bool {
        self.w > self.h
    }
    /// Content rectangle with the 24 px side padding.
    pub fn padded(&self) -> Rect {
        Rect::new(
            self.content.x + CONTENT_PAD,
            self.content.y,
            self.content.w - 2 * CONTENT_PAD,
            self.content.h,
        )
    }
    /// Columns for tile grids: 2 in portrait, 4 in landscape.
    pub fn columns(&self) -> i32 {
        if self.landscape() {
            4
        } else {
            2
        }
    }
}

/// Split `r` into `n` equal columns separated by `gap`.
pub fn columns(r: Rect, n: i32, gap: i32) -> Vec<Rect> {
    let w = (r.w - gap * (n - 1)) / n;
    (0..n)
        .map(|i| Rect::new(r.x + i * (w + gap), r.y, w, r.h))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portrait_and_landscape_frames_match_the_spec() {
        let p = Frame::new(800, 1280);
        assert_eq!(
            (p.status, p.nav),
            (Rect::new(0, 0, 800, 40), Rect::new(0, 1192, 800, 88))
        );
        assert_eq!(p.content, Rect::new(0, 40, 800, 1152));
        assert_eq!(p.padded(), Rect::new(24, 40, 752, 1152));
        assert_eq!(p.columns(), 2);
        let l = Frame::new(1280, 800);
        assert_eq!((l.content.y, l.content.bottom(), l.nav.y), (40, 712, 712));
        assert_eq!(l.columns(), 4);
        assert!(l.landscape() && !p.landscape());
    }

    #[test]
    fn two_columns_make_364_wide_tiles() {
        let c = columns(Frame::new(800, 1280).padded(), 2, 24);
        assert_eq!((c[0].w, c[1].w, c[1].x), (364, 364, 412));
    }
}
