use super::{Canvas, Rect};
use crate::tokens::Color;
use tiny_skia::{PixmapPaint, Transform};

impl Canvas {
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

    /// A `dst_w` x `dst_h` canvas from a `src_w` x `src_h` image in X ZPixmap order (B, G, R, pad;
    /// 4 bytes per pixel, rows contiguous), averaging the source pixels under each target pixel
    /// (a box filter: window thumbnails). None for empty sizes or a short buffer.
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn from_bgrx_scaled(
        src_w: usize,
        src_h: usize,
        data: &[u8],
        dst_w: u32,
        dst_h: u32,
    ) -> Option<Canvas> {
        if src_w == 0 || src_h == 0 || data.len() < src_w * src_h * 4 {
            return None;
        }
        let mut cv = Canvas::new(dst_w, dst_h, Color { r: 0, g: 0, b: 0 })?;
        let (dw, dh) = (dst_w as usize, dst_h as usize);
        let out = cv.pm.data_mut();
        for dy in 0..dh {
            let y0 = dy * src_h / dh;
            let y1 = ((dy + 1) * src_h / dh).max(y0 + 1).min(src_h);
            for dx in 0..dw {
                let x0 = dx * src_w / dw;
                let x1 = ((dx + 1) * src_w / dw).max(x0 + 1).min(src_w);
                let (mut b, mut g, mut r, mut n) = (0u32, 0u32, 0u32, 0u32);
                for y in y0..y1 {
                    for px in data[(y * src_w + x0) * 4..(y * src_w + x1) * 4].chunks_exact(4) {
                        b += px[0] as u32;
                        g += px[1] as u32;
                        r += px[2] as u32;
                        n += 1;
                    }
                }
                let o = (dy * dw + dx) * 4;
                out[o] = (r / n) as u8;
                out[o + 1] = (g / n) as u8;
                out[o + 2] = (b / n) as u8;
                out[o + 3] = 255;
            }
        }
        Some(cv)
    }

    /// This image resampled to `w` x `h` (box filter when shrinking, nearest when growing).
    #[allow(clippy::chunks_exact_to_as_chunks)]
    pub fn scaled(&self, w: u32, h: u32) -> Option<Canvas> {
        let (sw, sh) = (self.width() as usize, self.height() as usize);
        let mut bgrx = Vec::with_capacity(sw * sh * 4);
        for px in self.pm.data().chunks_exact(4) {
            bgrx.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        Canvas::from_bgrx_scaled(sw, sh, &bgrx, w, h)
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

    pub fn rgba_mut(&mut self) -> &mut [u8] {
        self.pm.data_mut()
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
