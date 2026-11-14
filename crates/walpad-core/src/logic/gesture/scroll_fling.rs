//! Scroll wheel emission, tap double-click snapping, release velocity, and fling/long-press ticks.

use super::recogniser::{dist, Fling, Gestures, Lock, Phase, St, P};
use super::{Out, BTN_LEFT, WHEEL_DOWN, WHEEL_LEFT, WHEEL_RIGHT, WHEEL_UP};
use crate::logic::Ms;

/// Release velocity window (ms): newest sample against the oldest one inside it.
const VEL_WINDOW_MS: Ms = 120;
const VEL_MIN_SPAN_MS: Ms = 20;

impl Gestures {
    /// Add finger travel `d` to `acc` and emit the whole wheel clicks (finger down = content
    /// down = wheel up).
    pub(super) fn scroll_step(
        &self,
        lock: Lock,
        d: (f32, f32),
        acc: &mut (f32, f32),
        out: &mut Vec<Out>,
    ) {
        let (dx, dy) = match lock {
            Lock::Free => d,
            Lock::Vertical => (0.0, d.1),
            Lock::Horizontal => (d.0, 0.0),
        };
        acc.0 += dx;
        acc.1 += dy;
        self.emit_clicks(acc, false, out);
    }

    pub(super) fn emit_clicks(&self, acc: &mut (f32, f32), ctrl: bool, out: &mut Vec<Out>) {
        let ppc = self.cfg.px_per_click.max(1) as f32;
        let mut budget = self.cfg.max_clicks;
        let mut axis = |a: &mut f32, pos: u8, neg: u8| {
            while a.abs() >= ppc && budget > 0 {
                let button = if *a > 0.0 { pos } else { neg };
                *a -= ppc.copysign(*a);
                budget -= 1;
                out.push(Out::Wheel { button, ctrl });
            }
        };
        axis(&mut acc.1, WHEEL_UP, WHEEL_DOWN);
        axis(&mut acc.0, WHEEL_LEFT, WHEEL_RIGHT);
    }

    pub(super) fn click(&mut self, p: P, now: Ms, out: &mut Vec<Out>) {
        let mut p = p;
        if let Some((q, t)) = self.last_click {
            if now.saturating_sub(t) <= self.cfg.double_tap_ms
                && dist(p, q) <= self.cfg.double_tap_px as f32
            {
                p = q;
            }
        }
        out.push(Out::Move(p.0, p.1));
        out.push(Out::Button(BTN_LEFT, true));
        out.push(Out::Button(BTN_LEFT, false));
        self.last_click = Some((p, now));
    }

    /// Release speed of the finger (px per ms), from the newest samples.
    pub(super) fn velocity(&self) -> (f32, f32) {
        let Some(&(t1, p1)) = self.samples.back() else {
            return (0.0, 0.0);
        };
        let old = self
            .samples
            .iter()
            .find(|(t, _)| t1.saturating_sub(*t) <= VEL_WINDOW_MS && t1 - *t >= VEL_MIN_SPAN_MS);
        match old {
            Some(&(t0, p0)) => {
                let dt = (t1 - t0) as f32;
                ((p1.0 - p0.0) as f32 / dt, (p1.1 - p0.1) as f32 / dt)
            }
            None => (0.0, 0.0),
        }
    }

    /// Timers: the long press, the fling.
    pub fn tick(&mut self, now: Ms) -> Vec<Out> {
        let mut out = Vec::new();
        if let St::Touch(t) = &mut self.st {
            if t.phase == Phase::Pending
                && now.saturating_sub(t.t0) >= self.cfg.long_press_ms
                && t.seen.saturating_sub(t.t0) >= self.cfg.long_press_ms
                && dist(t.start, t.cur) < self.cfg.long_press_px as f32
            {
                t.phase = Phase::Held;
            }
        }
        if let Some(mut f) = self.fling.take() {
            let tau = self.cfg.fling_tau_ms.max(1.0);
            let end = self.cfg.fling_ms;
            let (a, b) = (
                now.min(f.t0 + end).saturating_sub(f.t0) as f32,
                f.last.saturating_sub(f.t0) as f32,
            );
            // exact integral of v0 * exp(-t / tau) over the time since the last call
            let span = tau * ((-b / tau).exp() - (-a / tau).exp());
            let mut acc = f.acc;
            acc.0 += f.v.0 * span;
            acc.1 += f.v.1 * span;
            self.emit_clicks(&mut acc, false, &mut out);
            f.acc = acc;
            f.last = now;
            if now.saturating_sub(f.t0) < end {
                self.fling = Some(Fling {
                    v: f.v,
                    t0: f.t0,
                    last: f.last,
                    acc: f.acc,
                });
            }
        }
        out
    }
}
