//! Software canvas on a tiny-skia pixmap. Opaque RGBA (premultiplied = straight, alpha is 255).
//! Drawing calls are offset by `origin` (scrolled screens draw in content coordinates straight into
//! the frame, no off-screen copy); `fill`, `blit`, `crop`, `copy_from` and `pixel` ignore it.
use crate::tokens::Color;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
    /// Shrink by `d` on every side.
    pub fn inset(&self, d: i32) -> Rect {
        Rect::new(
            self.x + d,
            self.y + d,
            (self.w - 2 * d).max(0),
            (self.h - 2 * d).max(0),
        )
    }
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

pub struct Canvas {
    pub(crate) pm: Pixmap,
    origin: (i32, i32),
}

fn paint(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c.sk());
    p.anti_alias = true;
    p
}

fn rounded_rect_path(r: Rect, radius: f32) -> Option<Path> {
    rounded_rect_f(r.x as f32, r.y as f32, r.w as f32, r.h as f32, radius)
}

pub(crate) fn rounded_rect_f(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
    let rad = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let k = 0.552_284_8 * rad;
    let mut pb = PathBuilder::new();
    pb.move_to(x + rad, y);
    pb.line_to(x + w - rad, y);
    pb.cubic_to(x + w - rad + k, y, x + w, y + rad - k, x + w, y + rad);
    pb.line_to(x + w, y + h - rad);
    pb.cubic_to(
        x + w,
        y + h - rad + k,
        x + w - rad + k,
        y + h,
        x + w - rad,
        y + h,
    );
    pb.line_to(x + rad, y + h);
    pb.cubic_to(x + rad - k, y + h, x, y + h - rad + k, x, y + h - rad);
    pb.line_to(x, y + rad);
    pb.cubic_to(x, y + rad - k, x + rad - k, y, x + rad, y);
    pb.close();
    pb.finish()
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

    pub fn rect(&mut self, r: Rect, c: Color) {
        if r.w <= 0 || r.h <= 0 || !self.visible(r) {
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

    pub fn rrect(&mut self, r: Rect, radius: i32, c: Color) {
        if !self.visible(r) {
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

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, c: Color) {
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

    /// Draw another canvas (scroll areas and sheets are rendered off-screen then placed).
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32) {
        self.pm.draw_pixmap(
            x,
            y,
            src.pm.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    /// Copy of a region (clipped to the canvas) as a new canvas.
    pub fn crop(&self, r: Rect) -> Option<Canvas> {
        let r = Rect::new(
            r.x.max(0),
            r.y.max(0),
            r.w.min(self.width() - r.x.max(0)),
            r.h.min(self.height() - r.y.max(0)),
        );
        let sub = self.pm.clone_rect(tiny_skia::IntRect::from_xywh(
            r.x,
            r.y,
            r.w.max(1) as u32,
            r.h.max(1) as u32,
        )?)?;
        Some(Canvas {
            pm: sub,
            origin: (0, 0),
        })
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<Color> {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return None;
        }
        let i = (y as usize * self.width() as usize + x as usize) * 4;
        let d = self.pm.data();
        Some(Color {
            r: d[i],
            g: d[i + 1],
            b: d[i + 2],
        })
    }

    /// Premultiplied RGBA bytes (alpha is always 255 here).
    pub fn rgba(&self) -> &[u8] {
        self.pm.data()
    }

    /// Blend one glyph coverage value over the canvas (used by text).
    pub(crate) fn blend(&mut self, x: i32, y: i32, coverage: u8, c: Color) {
        let (x, y) = (x + self.origin.0, y + self.origin.1);
        if coverage == 0 || x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return;
        }
        let i = (y as usize * self.width() as usize + x as usize) * 4;
        let cov = coverage as u32;
        let d = self.pm.data_mut();
        d[i] = ((c.r as u32 * cov + d[i] as u32 * (255 - cov) + 127) / 255) as u8;
        d[i + 1] = ((c.g as u32 * cov + d[i + 1] as u32 * (255 - cov) + 127) / 255) as u8;
        d[i + 2] = ((c.b as u32 * cov + d[i + 2] as u32 * (255 - cov) + 127) / 255) as u8;
    }

    pub fn encode_png(&self) -> Option<Vec<u8>> {
        self.pm.encode_png().ok()
    }
    pub fn save_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        let bytes = self
            .encode_png()
            .ok_or_else(|| std::io::Error::other("png encode failed"))?;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::*;

    #[test]
    fn fills_and_reads_back() {
        let mut c = Canvas::new(20, 20, BG).unwrap();
        c.rect(Rect::new(5, 5, 10, 10), ACCENT);
        assert_eq!(c.pixel(0, 0), Some(BG));
        assert_eq!(c.pixel(10, 10), Some(ACCENT));
        assert_eq!(c.pixel(20, 20), None);
        assert_eq!(c.rgba().len(), 20 * 20 * 4);
    }

    #[test]
    fn rounded_rect_corners_are_cut_and_border_shows() {
        let mut c = Canvas::new(100, 60, BG).unwrap();
        c.rrect_border(Rect::new(10, 10, 80, 40), 16, PRESSED_FILL, DIVIDER);
        assert_eq!(
            c.pixel(11, 11),
            Some(BG),
            "corner pixel is outside the radius"
        );
        assert_eq!(
            c.pixel(50, 10),
            Some(DIVIDER),
            "top edge is the 1 px border"
        );
        assert_eq!(c.pixel(50, 30), Some(PRESSED_FILL));
    }

    #[test]
    fn blit_and_crop_roundtrip() {
        let mut a = Canvas::new(40, 40, BG).unwrap();
        let mut s = Canvas::new(10, 10, WATERMARK).unwrap();
        s.rect(Rect::new(0, 0, 5, 5), DANGER);
        a.blit(&s, 20, 20);
        assert_eq!(a.pixel(21, 21), Some(DANGER));
        assert_eq!(a.pixel(28, 28), Some(WATERMARK));
        let c = a.crop(Rect::new(20, 20, 10, 10)).unwrap();
        assert_eq!(c.pixel(1, 1), Some(DANGER));
    }

    #[test]
    fn origin_offsets_drawing_but_not_reads() {
        let mut c = Canvas::new(40, 40, BG).unwrap();
        c.set_origin(10, -100);
        assert!(!c.visible(Rect::new(0, 0, 5, 5)), "above the canvas");
        assert!(c.visible(Rect::new(0, 100, 5, 5)));
        c.rect(Rect::new(0, 100, 5, 5), DANGER);
        c.set_origin(0, 0);
        assert_eq!(c.pixel(12, 2), Some(DANGER));
        assert_eq!(c.pixel(2, 2), Some(BG));
    }

    #[test]
    fn outlines_leave_the_inside_untouched() {
        let mut c = Canvas::new(60, 60, BG).unwrap();
        c.rrect_stroke(Rect::new(4, 4, 52, 52), 16, OUTLINE_W, TEXT);
        assert_eq!(c.pixel(30, 30), Some(BG), "transparent inside");
        assert_ne!(c.pixel(30, 4), Some(BG), "top edge is drawn");
        assert_eq!(c.pixel(30, 2), Some(BG), "nothing outside the rect");
        let mut d = Canvas::new(60, 60, DANGER).unwrap();
        assert!(d.copy_from(&c));
        assert_eq!(d.pixel(30, 30), Some(BG));
        assert!(!d.copy_from(&Canvas::new(10, 10, BG).unwrap()));
        let mut e = Canvas::new(60, 60, DANGER).unwrap();
        assert!(e.copy_rect_from(&c, Rect::new(20, -5, 20, 15)));
        assert_eq!(e.pixel(30, 8), Some(BG), "copied (clipped at the top)");
        assert_eq!(e.pixel(30, 20), Some(DANGER), "outside the rect untouched");
        assert_eq!(e.pixel(10, 5), Some(DANGER));
    }

    #[test]
    fn rect_helpers() {
        let r = Rect::new(10, 10, 100, 50);
        assert!(r.contains(10, 10) && !r.contains(110, 10));
        assert_eq!(r.inset(5), Rect::new(15, 15, 90, 40));
        assert_eq!(r.center(), (60, 35));
        assert_eq!((r.right(), r.bottom()), (110, 60));
    }
}
