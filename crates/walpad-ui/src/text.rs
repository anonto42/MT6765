//! Text: DejaVu faces through fontdue with a glyph cache. Fonts are loaded from a directory at
//! runtime (/usr/share/fonts/dejavu on the tablet); tests load the vendored copy in assets/fonts.
use crate::canvas::{Canvas, Rect};
use crate::tokens::Color;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Regular,
    Bold,
    ExtraLight,
    Mono,
}

impl Face {
    const ALL: [Face; 4] = [Face::Regular, Face::Bold, Face::ExtraLight, Face::Mono];
    fn file(self) -> &'static str {
        match self {
            Face::Regular => "DejaVuSans.ttf",
            Face::Bold => "DejaVuSans-Bold.ttf",
            Face::ExtraLight => "DejaVuSans-ExtraLight.ttf",
            Face::Mono => "DejaVuSansMono.ttf",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }
}

/// Face, size and colour of a piece of text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub face: Face,
    pub px: f32,
    pub color: Color,
}

impl Style {
    pub const fn new(face: Face, px: f32, color: Color) -> Self {
        Style { face, px, color }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

struct Glyph {
    xmin: i32,
    ymin: i32,
    w: usize,
    h: usize,
    advance: f32,
    cov: Vec<u8>,
}

pub struct Fonts {
    fonts: Vec<Font>,
    cache: HashMap<(usize, char, u32), Glyph>,
}

#[derive(Debug)]
pub enum FontError {
    Io(String),
    Parse(String),
}

impl Fonts {
    pub fn load(dir: &Path) -> Result<Fonts, FontError> {
        let mut fonts = Vec::new();
        for f in Face::ALL {
            let bytes = std::fs::read(dir.join(f.file()))
                .map_err(|e| FontError::Io(format!("{}: {e}", f.file())))?;
            fonts.push(
                Font::from_bytes(bytes, FontSettings::default())
                    .map_err(|e| FontError::Parse(format!("{}: {e}", f.file())))?,
            );
        }
        Ok(Fonts {
            fonts,
            cache: HashMap::new(),
        })
    }

    /// The tablet's font directory, falling back to the repository copy (host runs).
    pub fn load_default() -> Result<Fonts, FontError> {
        Fonts::load(Path::new("/usr/share/fonts/dejavu"))
            .or_else(|_| Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")))
    }

    pub fn cached_glyphs(&self) -> usize {
        self.cache.len()
    }

    fn glyph(&mut self, face: Face, ch: char, px: f32) -> &Glyph {
        let key = (face.index(), ch, px.to_bits());
        let fonts = &self.fonts;
        self.cache.entry(key).or_insert_with(|| {
            let (m, cov) = fonts[face.index()].rasterize(ch, px);
            Glyph {
                xmin: m.xmin,
                ymin: m.ymin,
                w: m.width,
                h: m.height,
                advance: m.advance_width,
                cov,
            }
        })
    }

    /// Width of `s` in px.
    pub fn measure(&mut self, s: &str, face: Face, px: f32) -> f32 {
        s.chars().map(|c| self.glyph(face, c, px).advance).sum()
    }

    /// (ascent, descent) in px; descent is negative.
    pub fn line_metrics(&self, face: Face, px: f32) -> (f32, f32) {
        self.fonts[face.index()]
            .horizontal_line_metrics(px)
            .map(|m| (m.ascent, m.descent))
            .unwrap_or((px * 0.9, -px * 0.2))
    }

    /// Shorten `s` with an ellipsis so it fits `max_w`.
    pub fn fit(&mut self, s: &str, face: Face, px: f32, max_w: f32) -> String {
        if self.measure(s, face, px) <= max_w {
            return s.to_string();
        }
        let ell = self.measure("…", face, px);
        let mut out = String::new();
        let mut w = 0.0;
        for ch in s.chars() {
            let a = self.glyph(face, ch, px).advance;
            if w + a + ell > max_w {
                break;
            }
            w += a;
            out.push(ch);
        }
        out.push('…');
        out
    }

    /// Draw `s` with its baseline at `baseline`, starting at `x`. Returns the advance width.
    pub fn draw_at(
        &mut self,
        cv: &mut Canvas,
        s: &str,
        style: Style,
        x: f32,
        baseline: f32,
    ) -> f32 {
        let Style { face, px, color } = style;
        let mut pen = x;
        for ch in s.chars() {
            let (xmin, ymin, w, h, adv) = {
                let g = self.glyph(face, ch, px);
                (g.xmin, g.ymin, g.w, g.h, g.advance)
            };
            let gx = (pen + xmin as f32).round() as i32;
            let gy = (baseline - h as f32 - ymin as f32).round() as i32;
            if w > 0 && h > 0 {
                let cov = &self.glyph(face, ch, px).cov;
                for row in 0..h {
                    for col in 0..w {
                        cv.blend(gx + col as i32, gy + row as i32, cov[row * w + col], color);
                    }
                }
            }
            pen += adv;
        }
        pen - x
    }

    /// Draw `s` inside `r`, vertically centred, with horizontal alignment and ellipsis.
    pub fn draw_in(&mut self, cv: &mut Canvas, s: &str, style: Style, r: Rect, align: Align) {
        let Style { face, px, color: _ } = style;
        let s = self.fit(s, face, px, r.w as f32);
        let w = self.measure(&s, face, px);
        let (asc, desc) = self.line_metrics(face, px);
        let baseline = r.y as f32 + (r.h as f32 - (asc - desc)) / 2.0 + asc;
        let x = match align {
            Align::Left => r.x as f32,
            Align::Center => r.x as f32 + (r.w as f32 - w) / 2.0,
            Align::Right => r.right() as f32 - w,
        };
        self.draw_at(cv, &s, style, x, baseline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::*;

    fn fonts() -> Fonts {
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")).unwrap()
    }

    #[test]
    fn measures_and_caches_glyphs() {
        let mut f = fonts();
        let w1 = f.measure("WalPad", Face::Regular, 18.0);
        let n = f.cached_glyphs();
        assert!(w1 > 40.0 && w1 < 120.0, "{w1}");
        let w2 = f.measure("WalPad", Face::Regular, 18.0);
        assert_eq!(w1, w2);
        assert_eq!(f.cached_glyphs(), n, "second measure hits the cache");
        assert!(f.measure("WalPad", Face::Bold, 18.0) > w1, "bold is wider");
        assert!(
            f.measure("iiii", Face::Mono, 15.0) > f.measure("iiii", Face::Regular, 15.0),
            "mono is fixed width"
        );
    }

    #[test]
    fn ellipsis_fits_the_box() {
        let mut f = fonts();
        let long = "A very long network name that cannot fit";
        let fitted = f.fit(long, Face::Regular, 18.0, 150.0);
        assert!(fitted.ends_with('…'));
        assert!(f.measure(&fitted, Face::Regular, 18.0) <= 150.0);
        assert_eq!(f.fit("Home", Face::Regular, 18.0, 150.0), "Home");
    }

    #[test]
    fn drawing_changes_pixels_only_inside_the_box() {
        let mut f = fonts();
        let mut c = Canvas::new(200, 60, BG).unwrap();
        f.draw_in(
            &mut c,
            "Home",
            Style::new(Face::Regular, 18.0, TEXT),
            Rect::new(10, 10, 120, 40),
            Align::Left,
        );
        let lit = (0..200)
            .flat_map(|x| (0..60).map(move |y| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
            .collect::<Vec<_>>();
        assert!(lit.len() > 100, "some pixels drawn");
        assert!(
            lit.iter()
                .all(|&(x, y)| (10..130).contains(&x) && (10..50).contains(&y)),
            "nothing outside the box"
        );
    }

    #[test]
    fn utf8_and_missing_glyphs_do_not_panic() {
        let mut f = fonts();
        let mut c = Canvas::new(120, 40, BG).unwrap();
        f.draw_in(
            &mut c,
            "Café ✓ 日本",
            Style::new(Face::Regular, 18.0, TEXT),
            Rect::new(0, 0, 120, 40),
            Align::Center,
        );
    }

    #[test]
    fn missing_font_dir_is_an_error() {
        assert!(matches!(
            Fonts::load(Path::new("/nonexistent")),
            Err(FontError::Io(_))
        ));
    }
}
