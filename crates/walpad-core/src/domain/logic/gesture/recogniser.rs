//! Core touch gesture state machine (`Gestures`).

use super::{Cfg, Mode, Out, BTN_LEFT, WHEEL_DOWN, WHEEL_UP};
use crate::logic::touch_map::Act;
use crate::logic::Ms;
use std::collections::VecDeque;

pub(super) type P = (i32, i32);

pub(super) fn dist(a: P, b: P) -> f32 {
    let (dx, dy) = ((a.0 - b.0) as f32, (a.1 - b.1) as f32);
    (dx * dx + dy * dy).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Lock {
    Free,
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Phase {
    /// Down, not moved far, not held long: a tap, a long press or a scroll in the making.
    Pending,
    Scrolling {
        lock: Lock,
        prev: P,
        acc: (f32, f32),
    },
    /// Held still for the long-press time: a right click on lift, a selection drag on movement.
    Held,
    /// Button 1 is down (after a long press): a mouse drag.
    Select,
    Pinch {
        last: f32,
        acc: f32,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Touch {
    pub(super) phase: Phase,
    pub(super) start: P,
    pub(super) t0: Ms,
    pub(super) cur: P,
    /// Time of the newest report: the finger was seen down at least until then.
    pub(super) seen: Ms,
    /// The touch stopped a fling: it must not click.
    pub(super) no_click: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum St {
    Idle,
    /// Pointer actions pass through.
    Mouse,
    Touch(Touch),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Fling {
    /// Finger direction, px per ms.
    pub(super) v: (f32, f32),
    pub(super) t0: Ms,
    pub(super) last: Ms,
    pub(super) acc: (f32, f32),
}

pub struct Gestures {
    pub(super) cfg: Cfg,
    pub(super) pos: Option<P>,
    pub(super) st: St,
    pub(super) fling: Option<Fling>,
    pub(super) last_click: Option<(P, Ms)>,
    pub(super) samples: VecDeque<(Ms, P)>,
}

pub(super) const SAMPLES: usize = 8;

impl Gestures {
    pub fn new(cfg: Cfg) -> Self {
        Gestures {
            cfg,
            pos: None,
            st: St::Idle,
            fling: None,
            last_click: None,
            samples: VecDeque::new(),
        }
    }

    pub fn cfg(&self) -> &Cfg {
        &self.cfg
    }

    /// A finger is down or a fling runs: the caller should tick fast.
    pub fn active(&self) -> bool {
        !matches!(self.st, St::Idle) || self.fling.is_some()
    }

    /// Feed one pointer action from the touch map. `probe(x, y)` classifies the touch; it is
    /// called once, at the press.
    pub fn on_act(
        &mut self,
        act: Act,
        now: Ms,
        probe: &mut dyn FnMut(i32, i32) -> Mode,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        match act {
            Act::Move(x, y) => {
                self.pos = Some((x, y));
                match self.st {
                    St::Mouse => out.push(Out::Move(x, y)),
                    St::Touch(_) => self.touch_move((x, y), now, &mut out),
                    St::Idle => {}
                }
            }
            Act::Press => {
                if let Some(p) = self.pos {
                    self.begin(p, now, probe, &mut out);
                }
            }
            Act::Release => self.end(now, &mut out),
            Act::Pair(a, b) => self.pair(a, b, &mut out),
        }
        out
    }

    fn begin(
        &mut self,
        p: P,
        now: Ms,
        probe: &mut dyn FnMut(i32, i32) -> Mode,
        out: &mut Vec<Out>,
    ) {
        let was_flinging = self.fling.take().is_some();
        self.samples.clear();
        self.samples.push_back((now, p));
        let mode = if self.cfg.enabled {
            probe(p.0, p.1)
        } else {
            Mode::Mouse
        };
        match mode {
            Mode::Mouse => {
                self.st = St::Mouse;
                out.push(Out::Move(p.0, p.1));
                out.push(Out::Button(BTN_LEFT, true));
            }
            Mode::Scroll => {
                self.st = St::Touch(Touch {
                    phase: Phase::Pending,
                    start: p,
                    t0: now,
                    cur: p,
                    seen: now,
                    no_click: was_flinging,
                });
            }
        }
    }

    fn touch_move(&mut self, p: P, now: Ms, out: &mut Vec<Out>) {
        self.samples.push_back((now, p));
        while self.samples.len() > SAMPLES {
            self.samples.pop_front();
        }
        let St::Touch(mut t) = self.st else { return };
        t.cur = p;
        t.seen = now;
        let moved = dist(t.start, p);
        match t.phase {
            Phase::Pending if moved >= self.cfg.slop_px as f32 => {
                // the pointer goes to where the finger went down: the wheel acts on that window
                out.push(Out::Move(t.start.0, t.start.1));
                let (dx, dy) = ((p.0 - t.start.0) as f32, (p.1 - t.start.1) as f32);
                let lock = if dx.abs() > 2.0 * dy.abs() {
                    Lock::Horizontal
                } else if dy.abs() > 2.0 * dx.abs() {
                    Lock::Vertical
                } else {
                    Lock::Free
                };
                let mut acc = (0.0, 0.0);
                self.scroll_step(lock, (dx, dy), &mut acc, out);
                t.phase = Phase::Scrolling { lock, prev: p, acc };
            }
            Phase::Scrolling {
                lock,
                prev,
                mut acc,
            } => {
                let d = ((p.0 - prev.0) as f32, (p.1 - prev.1) as f32);
                self.scroll_step(lock, d, &mut acc, out);
                t.phase = Phase::Scrolling { lock, prev: p, acc };
            }
            Phase::Held if moved >= self.cfg.slop_px as f32 => {
                out.push(Out::Move(t.start.0, t.start.1));
                out.push(Out::Button(BTN_LEFT, true));
                out.push(Out::Move(p.0, p.1));
                t.phase = Phase::Select;
            }
            Phase::Select => out.push(Out::Move(p.0, p.1)),
            _ => {}
        }
        self.st = St::Touch(t);
    }

    /// Two confirmed fingers.
    fn pair(&mut self, a: P, b: P, out: &mut Vec<Out>) {
        let St::Touch(mut t) = self.st else { return };
        let d = dist(a, b);
        match t.phase {
            Phase::Pinch { last, mut acc } => {
                acc += d - last;
                let ppc = self.cfg.pinch_px_per_click.max(1) as f32;
                let mut n = 0;
                while acc.abs() >= ppc && n < self.cfg.max_clicks {
                    // fingers apart = zoom in = Ctrl + wheel up
                    let button = if acc > 0.0 { WHEEL_UP } else { WHEEL_DOWN };
                    out.push(Out::Wheel { button, ctrl: true });
                    acc -= ppc.copysign(acc);
                    n += 1;
                }
                t.phase = Phase::Pinch { last: d, acc };
            }
            phase => {
                if phase == Phase::Select {
                    out.push(Out::Button(BTN_LEFT, false));
                }
                // the wheel acts on the window between the fingers
                out.push(Out::Move((a.0 + b.0) / 2, (a.1 + b.1) / 2));
                t.phase = Phase::Pinch { last: d, acc: 0.0 };
                t.no_click = true;
            }
        }
        self.st = St::Touch(t);
    }

    fn end(&mut self, now: Ms, out: &mut Vec<Out>) {
        let st = std::mem::replace(&mut self.st, St::Idle);
        match st {
            St::Idle => {}
            St::Mouse => out.push(Out::Button(BTN_LEFT, false)),
            St::Touch(t) => match t.phase {
                Phase::Pending => {
                    if !t.no_click {
                        self.click(t.start, now, out);
                    }
                }
                Phase::Held => {
                    // a double click at the same point: most text widgets select the word under
                    // it on their own (standard X11/GTK/Qt behaviour), which raises the
                    // selection toolbar the same way any other selection does - no new wiring.
                    out.push(Out::Move(t.start.0, t.start.1));
                    out.push(Out::Button(BTN_LEFT, true));
                    out.push(Out::Button(BTN_LEFT, false));
                    out.push(Out::Button(BTN_LEFT, true));
                    out.push(Out::Button(BTN_LEFT, false));
                }
                Phase::Select => out.push(Out::Button(BTN_LEFT, false)),
                Phase::Scrolling { lock, .. } => {
                    let (mut vx, mut vy) = self.velocity();
                    match lock {
                        Lock::Vertical => vx = 0.0,
                        Lock::Horizontal => vy = 0.0,
                        Lock::Free => {}
                    }
                    let speed = (vx * vx + vy * vy).sqrt();
                    if speed >= self.cfg.fling_min {
                        let k = speed.min(self.cfg.fling_max) / speed;
                        self.fling = Some(Fling {
                            v: (vx * k, vy * k),
                            t0: now,
                            last: now,
                            acc: (0.0, 0.0),
                        });
                    }
                }
                Phase::Pinch { .. } => {}
            },
        }
        self.pos = None;
        self.samples.clear();
    }
}
