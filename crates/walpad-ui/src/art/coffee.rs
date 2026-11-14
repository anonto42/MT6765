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
