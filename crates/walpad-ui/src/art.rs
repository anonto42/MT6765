//! The background watermark: a line-art iced coffee (the owner's mock-up, SVG viewBox 200x300),
//! stroked at a constant screen width. Drawing it costs a few dozen path segments, but the shell
//! renders it once into a cached layer and never per frame.
use crate::canvas::{rounded_rect_f, Canvas, Rect};
use crate::tokens::Color;
use tiny_skia::{Path, PathBuilder, Transform};

/// Size of the drawing in its own units.
pub const VIEW_W: f32 = 200.0;
pub const VIEW_H: f32 = 300.0;

/// Where the cup goes inside `area`: centred, at most 62 % of either side, aspect kept.
pub fn coffee_rect(area: Rect) -> Rect {
    let s = (area.w as f32 * 0.62 / VIEW_W).min(area.h as f32 * 0.62 / VIEW_H);
    let (w, h) = ((VIEW_W * s) as i32, (VIEW_H * s) as i32);
    Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h)
}

/// A square of side `s` centred on (cx, cy), rotated by `deg`.
fn rotated_square(pb: &mut PathBuilder, x: f32, y: f32, s: f32, deg: f32) {
    let (cx, cy) = (x + s / 2.0, y + s / 2.0);
    if let Some(p) = rounded_rect_f(x, y, s, s, 2.0)
        .and_then(|p| p.transform(Transform::from_rotate_at(deg, cx, cy)))
    {
        pb.push_path(&p);
    }
}

fn cup_path() -> Option<Path> {
    let mut pb = PathBuilder::new();
    // straw and its three ticks
    pb.move_to(100.0, 10.0);
    pb.line_to(100.0, 280.0);
    for y in [30.0, 50.0, 70.0] {
        pb.move_to(94.0, y);
        pb.line_to(106.0, y + 10.0);
    }
    // dome lid and its rim
    pb.move_to(50.0, 100.0);
    pb.cubic_to(50.0, 40.0, 150.0, 40.0, 150.0, 100.0);
    if let Some(rim) = rounded_rect_f(45.0, 100.0, 110.0, 12.0, 3.0) {
        pb.push_path(&rim);
    }
    // tapered body with a rounded bottom
    pb.move_to(53.0, 112.0);
    pb.line_to(68.0, 280.0);
    pb.quad_to(70.0, 290.0, 100.0, 290.0);
    pb.quad_to(130.0, 290.0, 132.0, 280.0);
    pb.line_to(147.0, 112.0);
    // inner rim
    pb.move_to(55.0, 125.0);
    pb.cubic_to(55.0, 135.0, 145.0, 135.0, 145.0, 125.0);
    // ice cubes
    rotated_square(&mut pb, 70.0, 140.0, 22.0, 15.0);
    rotated_square(&mut pb, 110.0, 165.0, 20.0, -10.0);
    rotated_square(&mut pb, 75.0, 185.0, 24.0, 5.0);
    // liquid waves: "Q c p T e" continues with the control point mirrored through p
    for (y0, x0, cx, x1, x2) in [
        (210.0, 60.0, 80.0, 100.0, 140.0),
        (235.0, 62.0, 85.0, 100.0, 138.0),
        (260.0, 65.0, 90.0, 100.0, 135.0),
    ] {
        pb.move_to(x0, y0);
        pb.quad_to(cx, y0 + 20.0, x1, y0);
        pb.quad_to(2.0 * x1 - cx, y0 - 20.0, x2, y0);
    }
    pb.finish()
}

/// Stroke the cup into `r` (from `coffee_rect`) with lines `width` px wide on screen.
pub fn draw_coffee(cv: &mut Canvas, r: Rect, width: f32, color: Color) {
    let s = (r.w as f32 / VIEW_W).min(r.h as f32 / VIEW_H);
    let t = Transform::from_scale(s, s).post_translate(r.x as f32, r.y as f32);
    // transform the geometry, not the pen: the line width stays `width` at any scale
    if let Some(path) = cup_path().and_then(|p| p.transform(t)) {
        cv.stroke(&path, width, color, Transform::identity());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::*;

    #[test]
    fn the_cup_fits_both_orientations_and_keeps_its_aspect() {
        for area in [Rect::new(96, 64, 704, 1216), Rect::new(96, 64, 1184, 736)] {
            let r = coffee_rect(area);
            assert!(r.x >= area.x && r.right() <= area.right());
            assert!(r.y >= area.y && r.bottom() <= area.bottom());
            let aspect = r.w as f32 / r.h as f32;
            assert!((aspect - VIEW_W / VIEW_H).abs() < 0.01, "{aspect}");
        }
    }

    #[test]
    fn strokes_stay_inside_the_rect() {
        let mut c = Canvas::new(300, 400, BG).unwrap();
        let r = Rect::new(50, 50, 200, 300);
        draw_coffee(&mut c, r, 3.0, WATERMARK);
        let lit: Vec<(i32, i32)> = (0..300)
            .flat_map(|x| (0..400).map(move |y| (x, y)))
            .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
            .collect();
        assert!(lit.len() > 1500, "drew {}", lit.len());
        assert!(lit.iter().all(|&(x, y)| r.inset(-3).contains(x, y)));
    }
}

/// The owner's coffee wallpaper ("Grey and White Coffee Desktop Wallpaper", 2026-11-10): charcoal
/// (`BG`) with sketched iced coffees and beans along the bottom, prepared for the panel in both
/// orientations (landscape 1280x800, portrait 800x1280).
pub const WALLPAPER_LANDSCAPE: &[u8] = include_bytes!("../assets/wallpaper/landscape.png");
pub const WALLPAPER_PORTRAIT: &[u8] = include_bytes!("../assets/wallpaper/portrait.png");

/// Decoded wallpaper for a frame of `w` x `h` (landscape when wider than tall).
pub fn wallpaper(w: i32, h: i32) -> Option<crate::canvas::Canvas> {
    crate::canvas::Canvas::from_png(if w > h {
        WALLPAPER_LANDSCAPE
    } else {
        WALLPAPER_PORTRAIT
    })
}

/// Decoded wallpaper adapted for the active theme mode (`Dark` keeps the original charcoal art;
/// `Light` maps the charcoal base and white line-art to the light background and subtle dark ink).
pub fn wallpaper_for_theme(
    w: i32,
    h: i32,
    mode: crate::tokens::ThemeMode,
) -> Option<crate::canvas::Canvas> {
    let mut cv = wallpaper(w, h)?;
    if mode == crate::tokens::ThemeMode::Light {
        let pal = crate::tokens::LIGHT_PALETTE;
        let base_r = crate::tokens::BG.r as i32;
        let span = (255 - base_r).max(1);
        for px in cv.rgba_mut().as_chunks_mut::<4>().0 {
            let t = ((px[0] as i32 - base_r).clamp(0, span) * 100 / span) as u32;
            let c = pal.bg.mix(pal.text_secondary, t * 45 / 100);
            px[0] = c.r;
            px[1] = c.g;
            px[2] = c.b;
            px[3] = 255;
        }
    }
    Some(cv)
}

#[cfg(test)]
mod wallpaper_tests {
    use super::*;
    #[test]
    fn both_wallpapers_decode_at_panel_size_with_the_background_colour_on_top() {
        for (w, h) in [(1280, 800), (800, 1280)] {
            let c = wallpaper(w, h).expect("decodes");
            assert_eq!((c.width(), c.height()), (w, h));
            // the top is plain background (the art sits along the bottom)
            assert_eq!(c.pixel(w / 2, 10), Some(crate::tokens::BG));
        }
    }
}
