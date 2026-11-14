use super::layout::{
    cursor_bounds, pad_layout, pad_rect, PadButton, PadOut, ACCEL, BRIDGE_LIFT_MS, BUTTON_LEFT,
    BUTTON_RIGHT, BUTTON_WHEEL_DOWN, BUTTON_WHEEL_UP, JUMP_PX, LONG_PRESS_MS, SMOOTH, TAP_MAX_MS,
    TAP_SLOP_PX, TAP_TRAVEL_PX,
};
use std::collections::VecDeque;

/// Warp positions remembered for echo suppression.
const PENDING_MAX: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Touch {
    start: (i32, i32),
    /// Finger position the relative motion is measured from (root coordinates).
    last: (i32, i32),
    /// Last accepted finger sample (jump filter reference).
    prev: (i32, i32),
    /// A sample that jumped more than `JUMP_PX`: dropped unless the next one confirms it.
    spike: Option<(i32, i32)>,
    t0: u32,
    /// Largest distance (either axis) of an accepted sample from `start`.
    travel: i32,
    /// Travel exceeded the slop: this touch moves the cursor.
    moving: bool,
    /// The virtual cursor when the finger went down (a tap clicks there).
    cursor0: (i32, i32),
    /// The touch began on a button of the row.
    button: Option<PadButton>,
}

fn cheb(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The pad's gesture state (pure: fed root coordinates and X timestamps).
///
/// Robust against the panel's noisy touch: a single sample that jumps more than `JUMP_PX` from the
/// previous one is dropped (two in a row that agree re-anchor without moving the cursor); deltas
/// go through a low-pass that keeps the total travel (`acc` is spent `SMOOTH` per sample and the
/// rest on release); nothing moves inside the 10 px slop, then the slop travel is included; a tap
/// tolerates 20 px of jitter and clicks where the cursor was when the finger went down.
#[derive(Debug, Clone, PartialEq)]
pub struct PadState {
    area: (i32, i32),
    pub cursor: (i32, i32),
    rem: (f32, f32),
    /// Accelerated travel not yet applied (the low-pass backlog).
    acc: (f32, f32),
    touch: Option<Touch>,
    pending: VecDeque<(i32, i32)>,
}

impl PadState {
    pub fn new(area: (i32, i32)) -> PadState {
        let mut s = PadState {
            area,
            cursor: (0, 0),
            rem: (0.0, 0.0),
            acc: (0.0, 0.0),
            touch: None,
            pending: VecDeque::new(),
        };
        s.recenter();
        s
    }

    pub fn area(&self) -> (i32, i32) {
        self.area
    }

    /// Cursor back to the middle of the area above the pad.
    pub fn recenter(&mut self) {
        let b = cursor_bounds(self.area);
        self.cursor = b.center();
        self.rem = (0.0, 0.0);
        self.acc = (0.0, 0.0);
    }

    /// Rotation: new area, the cursor recentred, any touch forgotten.
    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.touch = None;
        self.pending.clear();
        self.recenter();
    }

    /// Forget a touch in progress (the pad was hidden).
    pub fn reset_touch(&mut self) {
        self.touch = None;
        self.pending.clear();
        self.acc = (0.0, 0.0);
    }

    pub fn pressed_button(&self) -> Option<PadButton> {
        self.touch.and_then(|t| t.button)
    }

    fn button_at(&self, root: (i32, i32)) -> Option<PadButton> {
        let r = pad_rect(self.area);
        let (lx, ly) = (root.0 - r.x, root.1 - r.y);
        pad_layout(self.area)
            .buttons
            .iter()
            .find(|(b, _)| b.contains(lx, ly))
            .map(|(_, b)| *b)
    }

    /// The real pointer was put on the cursor: its MotionNotify is ours, not the finger's.
    pub fn note_warp(&mut self) {
        if self.pending.back() != Some(&self.cursor) {
            self.pending.push_back(self.cursor);
            if self.pending.len() > PENDING_MAX {
                self.pending.pop_front();
            }
        }
    }

    /// Finger down at a root position, X time `t` (ms). Every press re-anchors: the motion is
    /// measured from here, never from where the pointer was warped to after the last batch.
    pub fn press(&mut self, x: i32, y: i32, t: u32) {
        self.touch = Some(Touch {
            start: (x, y),
            last: (x, y),
            prev: (x, y),
            spike: None,
            t0: t,
            travel: 0,
            moving: false,
            cursor0: self.cursor,
            button: self.button_at((x, y)),
        });
        self.rem = (0.0, 0.0);
        self.acc = (0.0, 0.0);
    }

    /// Move the cursor by whole pixels of `d` (+ the carried fraction), clamped to the bounds.
    fn apply(&mut self, d: (f32, f32)) -> bool {
        let dx = d.0 + self.rem.0;
        let dy = d.1 + self.rem.1;
        let (ix, iy) = (dx.trunc(), dy.trunc());
        self.rem = (dx - ix, dy - iy);
        let b = cursor_bounds(self.area);
        let nx = (self.cursor.0 + ix as i32).clamp(b.x, b.right() - 1);
        let ny = (self.cursor.1 + iy as i32).clamp(b.y, b.bottom() - 1);
        if nx != self.cursor.0 + ix as i32 {
            self.rem.0 = 0.0;
            self.acc.0 = 0.0;
        }
        if ny != self.cursor.1 + iy as i32 {
            self.rem.1 = 0.0;
            self.acc.1 = 0.0;
        }
        let moved = (nx, ny) != self.cursor;
        self.cursor = (nx, ny);
        moved
    }

    /// Pointer motion at a root position. Returns true when the virtual cursor moved.
    pub fn motion(&mut self, x: i32, y: i32) -> bool {
        // the echo of one of our warps: drop it and every older one
        if let Some(i) = self.pending.iter().position(|p| *p == (x, y)) {
            self.pending.drain(..=i);
            return false;
        }
        let Some(t) = self.touch.as_mut() else {
            return false;
        };
        let p = (x, y);
        if t.button.is_some() {
            t.last = p;
            return false;
        }
        // jump filter: one wild sample is noise; two that agree mean the finger is really there
        if cheb(p, t.prev) > JUMP_PX {
            match t.spike {
                Some(s) if cheb(p, s) <= JUMP_PX => {
                    t.spike = None;
                    t.prev = p;
                    if t.moving {
                        t.last = p;
                    } else {
                        // still a tap candidate: start over from here
                        t.start = p;
                        t.last = p;
                    }
                }
                _ => t.spike = Some(p),
            }
            return false;
        }
        t.spike = None;
        t.prev = p;
        t.travel = t.travel.max(cheb(p, t.start));
        if !t.moving {
            if cheb(p, t.start) <= TAP_SLOP_PX {
                return false;
            }
            // past the slop: the whole travel from the start counts (t.last is still the start)
            t.moving = true;
        }
        let d = ((x - t.last.0) as f32 * ACCEL, (y - t.last.1) as f32 * ACCEL);
        t.last = p;
        self.acc.0 += d.0;
        self.acc.1 += d.1;
        let out = (self.acc.0 * SMOOTH, self.acc.1 * SMOOTH);
        self.acc.0 -= out.0;
        self.acc.1 -= out.1;
        self.apply(out)
    }

    /// Finger up at X time `t`. The release position is not used: the pointer sits on the cursor
    /// by then, the finger's last position is what counts. A drag spends its low-pass backlog.
    pub fn release(&mut self, t: u32) -> Option<PadOut> {
        let touch = self.touch.take()?;
        if let Some(b) = touch.button {
            // released off the button it started on: nothing
            if self.button_at(touch.last) != Some(b) {
                return None;
            }
            return Some(match b {
                PadButton::Left => PadOut::Click(BUTTON_LEFT),
                PadButton::Right => PadOut::Click(BUTTON_RIGHT),
                PadButton::ScrollUp => PadOut::Click(BUTTON_WHEEL_UP),
                PadButton::ScrollDown => PadOut::Click(BUTTON_WHEEL_DOWN),
                PadButton::Close => PadOut::Hide,
            });
        }
        let held = t.wrapping_sub(touch.t0).saturating_sub(BRIDGE_LIFT_MS);
        if touch.travel < TAP_TRAVEL_PX {
            let click = if held < TAP_MAX_MS {
                Some(BUTTON_LEFT)
            } else if held >= LONG_PRESS_MS {
                Some(BUTTON_RIGHT)
            } else {
                None
            };
            if let Some(b) = click {
                // jitter may have nudged the cursor: click where it was when the finger went down
                self.cursor = touch.cursor0;
                self.acc = (0.0, 0.0);
                self.rem = (0.0, 0.0);
                return Some(PadOut::Click(b));
            }
        }
        let rest = std::mem::take(&mut self.acc);
        if touch.moving {
            self.apply(rest);
        }
        None
    }
}
