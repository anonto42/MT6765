use crate::canvas::rounded_rect_f;
use tiny_skia::PathBuilder;

pub(super) struct Pen {
    pub(super) stroke: PathBuilder,
    pub(super) dots: PathBuilder,
}

impl Pen {
    pub(super) fn new() -> Self {
        Pen {
            stroke: PathBuilder::new(),
            dots: PathBuilder::new(),
        }
    }
    pub(super) fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        self.stroke.move_to(a.0, a.1);
        self.stroke.line_to(b.0, b.1);
    }
    pub(super) fn poly(&mut self, pts: &[(f32, f32)], closed: bool) {
        if let Some((first, rest)) = pts.split_first() {
            self.stroke.move_to(first.0, first.1);
            for p in rest {
                self.stroke.line_to(p.0, p.1);
            }
            if closed {
                self.stroke.close();
            }
        }
    }
    pub(super) fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        if let Some(p) = rounded_rect_f(x, y, w, h, r) {
            self.stroke.push_path(&p);
        }
    }
    pub(super) fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.stroke.push_circle(cx, cy, r);
    }
    pub(super) fn dot(&mut self, cx: f32, cy: f32, r: f32) {
        self.dots.push_circle(cx, cy, r);
    }
    /// Angles in degrees: 0 = right, 90 = down.
    pub(super) fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = ((a1 - a0).abs() / 8.0).ceil().max(4.0) as usize;
        for i in 0..=n {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
            let (x, y) = (cx + r * a.cos(), cy + r * a.sin());
            if i == 0 {
                self.stroke.move_to(x, y);
            } else {
                self.stroke.line_to(x, y);
            }
        }
    }
    pub(super) fn quad(&mut self, from: (f32, f32), ctl: (f32, f32), to: (f32, f32)) {
        self.stroke.move_to(from.0, from.1);
        self.stroke.quad_to(ctl.0, ctl.1, to.0, to.1);
    }
    pub(super) fn quad_to(&mut self, ctl: (f32, f32), to: (f32, f32)) {
        self.stroke.quad_to(ctl.0, ctl.1, to.0, to.1);
    }
    pub(super) fn rays(&mut self, cx: f32, cy: f32, r0: f32, r1: f32, n: usize) {
        for i in 0..n {
            let a = (360.0 * i as f32 / n as f32).to_radians();
            self.line(
                (cx + r0 * a.cos(), cy + r0 * a.sin()),
                (cx + r1 * a.cos(), cy + r1 * a.sin()),
            );
        }
    }
    pub(super) fn arrow_head(&mut self, tip: (f32, f32), a: (f32, f32), b: (f32, f32)) {
        self.poly(&[a, tip, b], false);
    }
}
