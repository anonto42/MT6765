//! Software canvas on a tiny-skia pixmap. Opaque RGBA (premultiplied = straight, alpha is 255).
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
    let k = 0.552_284_75 * rad;
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
        Some(Canvas { pm })
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

    pub fn rect(&mut self, r: Rect, c: Color) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        if let Some(rect) =
            tiny_skia::Rect::from_xywh(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
        {
            let mut p = paint(c);
            p.anti_alias = false;
            self.pm.fill_rect(rect, &p, Transform::identity(), None);
        }
    }

    pub fn rrect(&mut self, r: Rect, radius: i32, c: Color) {
        if let Some(path) = rounded_rect_path(r, radius as f32) {
            self.pm.fill_path(
                &path,
                &paint(c),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    /// Filled rounded rect with a 1 px border (the only elevation cue in the design).
    pub fn rrect_border(&mut self, r: Rect, radius: i32, fill: Color, border: Color) {
        self.rrect(r, radius, border);
        self.rrect(r.inset(1), (radius - 1).max(0), fill);
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, c: Color) {
        if let Some(path) = PathBuilder::from_circle(cx, cy, radius) {
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

    pub(crate) fn stroke(&mut self, path: &Path, width: f32, c: Color, t: Transform) {
        let stroke = Stroke {
            width,
            line_cap: tiny_skia::LineCap::Round,
            line_join: tiny_skia::LineJoin::Round,
            ..Stroke::default()
        };
        self.pm.stroke_path(path, &paint(c), &stroke, t, None);
    }
    pub(crate) fn fill_path(&mut self, path: &Path, c: Color, t: Transform) {
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
        Some(Canvas { pm: sub })
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
        c.rrect_border(Rect::new(10, 10, 80, 40), 16, SURFACE, BORDER);
        assert_eq!(
            c.pixel(11, 11),
            Some(BG),
            "corner pixel is outside the radius"
        );
        assert_eq!(c.pixel(50, 10), Some(BORDER), "top edge is the 1 px border");
        assert_eq!(c.pixel(50, 30), Some(SURFACE));
    }

    #[test]
    fn blit_and_crop_roundtrip() {
        let mut a = Canvas::new(40, 40, BG).unwrap();
        let mut s = Canvas::new(10, 10, SURFACE_RAISED).unwrap();
        s.rect(Rect::new(0, 0, 5, 5), DANGER);
        a.blit(&s, 20, 20);
        assert_eq!(a.pixel(21, 21), Some(DANGER));
        assert_eq!(a.pixel(28, 28), Some(SURFACE_RAISED));
        let c = a.crop(Rect::new(20, 20, 10, 10)).unwrap();
        assert_eq!(c.pixel(1, 1), Some(DANGER));
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
