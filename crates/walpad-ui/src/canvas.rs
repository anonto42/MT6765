//! Software canvas on a tiny-skia pixmap. Opaque RGBA (premultiplied = straight, alpha is 255).
//! Fills take `impl Into<Tint>`: an opaque `Color` is a plain write (tiny-skia's opaque fast path),
//! a translucent `Tint` is blended source-over onto the opaque pixels, which stay opaque.
//! Drawing calls are offset by `origin` (scrolled screens draw in content coordinates straight into
//! the frame, no off-screen copy); `fill`, `blit`, `crop`, `copy_from` and `pixel` ignore it.
use crate::tokens::{Color, Tint};
use tiny_skia::{FillRule, Path, PathBuilder, Pixmap, Stroke, Transform};

mod image;
mod rect;
#[cfg(test)]
mod tests;

pub(crate) use rect::rounded_rect_f;
pub use rect::Rect;
use rect::{paint, rounded_rect_path};

pub struct Canvas {
    pub(crate) pm: Pixmap,
    origin: (i32, i32),
}

impl Canvas {
    /// New canvas filled with `bg`.
    pub fn new(w: u32, h: u32, bg: Color) -> Option<Self> {
        let mut pm = Pixmap::new(w, h)?;
        pm.fill(bg.sk());
        Some(Canvas { pm, origin: (0, 0) })
    }
    /// Canvas from PNG bytes (opaque images; None if it cannot be decoded).
    pub fn from_png(bytes: &[u8]) -> Option<Self> {
        let pm = Pixmap::decode_png(bytes).ok()?;
        Some(Canvas { pm, origin: (0, 0) })
    }
    pub fn width(&self) -> i32 {
        self.pm.width() as i32
    }
    pub fn height(&self) -> i32 {
        self.pm.height() as i32
    }
    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width(), self.height())
    }
    pub fn fill(&mut self, c: Color) {
        self.pm.fill(c.sk());
    }

    /// Offset added to every drawing call until reset (content coordinates -> screen).
    pub fn set_origin(&mut self, dx: i32, dy: i32) {
        self.origin = (dx, dy);
    }
    pub fn origin(&self) -> (i32, i32) {
        self.origin
    }
    fn at(&self, r: Rect) -> Rect {
        Rect::new(r.x + self.origin.0, r.y + self.origin.1, r.w, r.h)
    }
    fn t(&self) -> Transform {
        Transform::from_translate(self.origin.0 as f32, self.origin.1 as f32)
    }
    /// True when `r` (in drawing coordinates) touches the pixmap: callers skip invisible work.
    pub fn visible(&self, r: Rect) -> bool {
        let r = self.at(r);
        r.right() > 0 && r.bottom() > 0 && r.x < self.width() && r.y < self.height()
    }

    /// Copy every pixel of `src` (same size) into this canvas: one memcpy, for cached layers.
    /// Returns false (and copies nothing) when the sizes differ.
    pub fn copy_from(&mut self, src: &Canvas) -> bool {
        if src.pm.width() != self.pm.width() || src.pm.height() != self.pm.height() {
            return false;
        }
        self.pm.data_mut().copy_from_slice(src.pm.data());
        true
    }

    /// Copy the pixels of `r` (clipped) from `src` (same size): one memcpy per row.
    pub fn copy_rect_from(&mut self, src: &Canvas, r: Rect) -> bool {
        if src.pm.width() != self.pm.width() || src.pm.height() != self.pm.height() {
            return false;
        }
        let (w, h) = (self.width(), self.height());
        let (x0, x1) = (r.x.clamp(0, w) as usize, r.right().clamp(0, w) as usize);
        let (y0, y1) = (r.y.clamp(0, h) as usize, r.bottom().clamp(0, h) as usize);
        let stride = w as usize * 4;
        let s = src.pm.data();
        let d = self.pm.data_mut();
        for y in y0..y1 {
            let (a, b) = (y * stride + x0 * 4, y * stride + x1 * 4);
            d[a..b].copy_from_slice(&s[a..b]);
        }
        true
    }

    /// Copy the pixels of `r` (screen coordinates, clipped) from `src`, which holds the screen
    /// region starting at `src_at` (a cached strip smaller than this canvas).
    pub fn copy_rect_from_at(&mut self, src: &Canvas, src_at: (i32, i32), r: Rect) {
        let (w, h) = (self.width(), self.height());
        let x0 = r.x.max(0).max(src_at.0);
        let x1 = r.right().min(w).min(src_at.0 + src.width());
        let y0 = r.y.max(0).max(src_at.1);
        let y1 = r.bottom().min(h).min(src_at.1 + src.height());
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let (ds, ss) = (w as usize * 4, src.width() as usize * 4);
        let n = (x1 - x0) as usize * 4;
        let s = src.pm.data();
        let d = self.pm.data_mut();
        for y in y0..y1 {
            let di = y as usize * ds + x0 as usize * 4;
            let si = (y - src_at.1) as usize * ss + (x0 - src_at.0) as usize * 4;
            d[di..di + n].copy_from_slice(&s[si..si + n]);
        }
    }

    pub fn rect(&mut self, r: Rect, c: impl Into<Tint>) {
        let c = c.into();
        if c.a == 0 || r.w <= 0 || r.h <= 0 || !self.visible(r) {
            return;
        }
        let r = self.at(r);
        if let Some(rect) =
            tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
        {
            let mut p = paint(c);
            p.anti_alias = false;
            self.pm.fill_rect(rect, &p, Transform::identity(), None);
        }
    }

    pub fn rrect(&mut self, r: Rect, radius: i32, c: impl Into<Tint>) {
        let c = c.into();
        if c.a == 0 || !self.visible(r) {
            return;
        }
        if let Some(path) = rounded_rect_path(self.at(r), radius as f32) {
            self.pm.fill_path(
                &path,
                &paint(c),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    /// Filled rounded rect with a 1 px border.
    pub fn rrect_border(&mut self, r: Rect, radius: i32, fill: Color, border: Color) {
        self.rrect(r, radius, border);
        self.rrect(r.inset(1), (radius - 1).max(0), fill);
    }

    /// Rounded-rect outline of `width` px drawn inside `r` (the line-art controls).
    pub fn rrect_stroke(&mut self, r: Rect, radius: i32, width: f32, c: Color) {
        if !self.visible(r) {
            return;
        }
        let h = width / 2.0;
        if let Some(path) = rounded_rect_f(
            r.x as f32 + h,
            r.y as f32 + h,
            r.w as f32 - width,
            r.h as f32 - width,
            radius as f32 - h,
        ) {
            let stroke = Stroke {
                width,
                ..Stroke::default()
            };
            self.pm
                .stroke_path(&path, &paint(c), &stroke, self.t(), None);
        }
    }

    /// Straight line with round caps.
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: Color) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(path) = pb.finish() {
            self.stroke(&path, width, c, Transform::identity());
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, c: impl Into<Tint>) {
        let c = c.into();
        if c.a == 0 {
            return;
        }
        let (ox, oy) = (self.origin.0 as f32, self.origin.1 as f32);
        if let Some(path) = PathBuilder::from_circle(cx + ox, cy + oy, radius) {
            self.pm.fill_path(
                &path,
                &paint(c),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, c: Color) {
        self.rect(Rect::new(x, y, w, 1), c);
    }

    /// Stroke `path` (round caps and joins); `t` maps path to drawing coordinates.
    pub(crate) fn stroke(&mut self, path: &Path, width: f32, c: Color, t: Transform) {
        let stroke = Stroke {
            width,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Stroke::default()
        };
        let t = t.post_concat(self.t());
        self.pm.stroke_path(path, &paint(c), &stroke, t, None);
    }
    pub(crate) fn fill_path(&mut self, path: &Path, c: Color, t: Transform) {
        let t = t.post_concat(self.t());
        self.pm
            .fill_path(path, &paint(c), FillRule::Winding, t, None);
    }
}
