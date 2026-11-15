//! Frame layout (docs/rust/UI.md): a sidebar dock on the left, a header across the main area and
//! the content below it; the same in portrait and landscape.
use crate::canvas::Rect;
use crate::tokens::{CONTENT_PAD, HEADER_H, SIDEBAR_W};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub w: i32,
    pub h: i32,
    /// Full-height dock on the left.
    pub sidebar: Rect,
    /// Clock and the quick-settings pill, right of the sidebar.
    pub header: Rect,
    /// Content area without side padding (right of the sidebar, below the header).
    pub content: Rect,
}

impl Frame {
    pub fn new(w: i32, h: i32) -> Frame {
        Frame {
            w,
            h,
            sidebar: Rect::new(0, 0, SIDEBAR_W, h),
            header: Rect::new(SIDEBAR_W, 0, w - SIDEBAR_W, HEADER_H),
            content: Rect::new(SIDEBAR_W, HEADER_H, w - SIDEBAR_W, h - HEADER_H),
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
    /// The whole screen.
    pub fn screen(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
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
mod tests;
