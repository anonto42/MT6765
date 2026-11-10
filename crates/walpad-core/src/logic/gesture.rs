//! Touch gestures (owner request 2026-11-10: "think like a mobile device"). Sits behind
//! `touch_map` (finger -> pointer actions) and turns a finger into what a phone app expects:
//!
//! - one-finger tap = left click at the touch point (no added delay: the click goes out when the
//!   touch ends; the bridge's 150 ms lift delay is the only wait);
//! - one-finger drag = scroll: wheel buttons 4/5 (vertical) and 6/7 (horizontal), one click per
//!   `px_per_click` of finger travel, then a fling that decays over `fling_ms`;
//! - hold 500 ms without moving, then lift = right click (context menu);
//! - hold 500 ms, then drag = a mouse drag (button 1 pressed at the touch point): text selection;
//! - taps close together in time and place = double (triple) click: the second click is snapped
//!   to the first one's position, the X toolkits do the counting;
//! - two fingers (only when the panel reports two contacts) = pinch, as Ctrl + wheel.
//!
//! A touch is classified once, when it starts, by `Mode`: `Mouse` passes the pointer actions
//! through unchanged (the shell's own windows, apps whose drag must stay a drag: apps.json
//! `"touch": "mouse"`), `Scroll` uses the recogniser. The caller answers the question "what is
//! under the finger" through the `probe` closure (an X query), once per touch.
use super::touch_map::Act;
use super::Ms;
use serde::Deserialize;
use std::collections::VecDeque;

pub const BTN_LEFT: u8 = 1;
pub const BTN_RIGHT: u8 = 3;
pub const WHEEL_UP: u8 = 4;
pub const WHEEL_DOWN: u8 = 5;
pub const WHEEL_LEFT: u8 = 6;
pub const WHEEL_RIGHT: u8 = 7;

/// What a touch does, decided when it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Finger = mouse (press at once, move, release at the lift).
    Mouse,
    /// Finger = touchscreen: tap, scroll, long press.
    #[default]
    Scroll,
}

impl Mode {
    /// apps.json `touch` value (case-insensitive); anything else is None (= the default).
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mouse" => Some(Mode::Mouse),
            "scroll" => Some(Mode::Scroll),
            _ => None,
        }
    }
}

/// What the recogniser asks the X bridge to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    Move(i32, i32),
    /// Button n pressed (true) or released (false).
    Button(u8, bool),
    /// One wheel click (button 4-7 press + release), with Control held when `ctrl`.
    Wheel {
        button: u8,
        ctrl: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    /// false = every touch is a mouse touch (the behaviour before gestures).
    pub enabled: bool,
    /// Finger travel before a touch stops being a tap/long press and becomes a scroll.
    pub slop_px: i32,
    pub long_press_ms: Ms,
    /// A long press needs less travel than this.
    pub long_press_px: i32,
    /// A tap this soon after a click (ms) and this near it (px) lands on the same spot.
    pub double_tap_ms: Ms,
    pub double_tap_px: i32,
    /// Finger travel per wheel click.
    pub px_per_click: i32,
    /// Fling: minimum / maximum release speed (px per ms), duration and time constant (ms).
    pub fling_min: f32,
    pub fling_max: f32,
    pub fling_ms: Ms,
    pub fling_tau_ms: f32,
    /// Wheel clicks per call at most (the rest waits for the next one).
    pub max_clicks: u32,
    /// Change of the distance between two fingers per zoom click.
    pub pinch_px_per_click: i32,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            enabled: true,
            slop_px: 12,
            long_press_ms: 500,
            long_press_px: 10,
            double_tap_ms: 450,
            double_tap_px: 40,
            px_per_click: 48,
            fling_min: 0.25,
            fling_max: 3.0,
            fling_ms: 400,
            fling_tau_ms: 150.0,
            max_clicks: 8,
            pinch_px_per_click: 40,
        }
    }
}

type P = (i32, i32);

fn dist(a: P, b: P) -> f32 {
    let (dx, dy) = ((a.0 - b.0) as f32, (a.1 - b.1) as f32);
    (dx * dx + dy * dy).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Lock {
    Free,
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
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
struct Touch {
    phase: Phase,
    start: P,
    t0: Ms,
    cur: P,
    /// Time of the newest report: the finger was seen down at least until then.
    seen: Ms,
    /// The touch stopped a fling: it must not click.
    no_click: bool,
}

#[derive(Debug, Clone, Copy)]
enum St {
    Idle,
    /// Pointer actions pass through.
    Mouse,
    Touch(Touch),
}

#[derive(Debug, Clone, Copy)]
struct Fling {
    /// Finger direction, px per ms.
    v: (f32, f32),
    t0: Ms,
    last: Ms,
    acc: (f32, f32),
}

pub struct Gestures {
    cfg: Cfg,
    pos: Option<P>,
    st: St,
    fling: Option<Fling>,
    last_click: Option<(P, Ms)>,
    samples: VecDeque<(Ms, P)>,
}

const SAMPLES: usize = 8;
/// Release velocity window (ms): newest sample against the oldest one inside it.
const VEL_WINDOW_MS: Ms = 120;
const VEL_MIN_SPAN_MS: Ms = 20;

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

    /// Add finger travel `d` to `acc` and emit the whole wheel clicks (finger down = content
    /// down = wheel up).
    fn scroll_step(&self, lock: Lock, d: (f32, f32), acc: &mut (f32, f32), out: &mut Vec<Out>) {
        let (dx, dy) = match lock {
            Lock::Free => d,
            Lock::Vertical => (0.0, d.1),
            Lock::Horizontal => (d.0, 0.0),
        };
        acc.0 += dx;
        acc.1 += dy;
        self.emit_clicks(acc, false, out);
    }

    fn emit_clicks(&self, acc: &mut (f32, f32), ctrl: bool, out: &mut Vec<Out>) {
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

    fn click(&mut self, p: P, now: Ms, out: &mut Vec<Out>) {
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
    fn velocity(&self) -> (f32, f32) {
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
                    out.push(Out::Move(t.start.0, t.start.1));
                    out.push(Out::Button(BTN_RIGHT, true));
                    out.push(Out::Button(BTN_RIGHT, false));
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
                self.fling = Some(f);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::touch_map::{Cfg as TmCfg, TouchMap};
    use walpad_hw::evdev::*;

    fn raw(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            sec: 0,
            usec: 0,
            kind,
            code,
            value,
        }
    }

    /// Touch map + recogniser, driven like the daemon does (events, then a tick).
    struct Sim {
        tm: TouchMap,
        g: Gestures,
        mode: Mode,
        probed: Vec<P>,
        log: Vec<(Ms, Out)>,
    }
    impl Sim {
        fn new() -> Sim {
            Sim::with(Cfg::default())
        }
        fn with(cfg: Cfg) -> Sim {
            Sim {
                tm: TouchMap::new(TmCfg::default()),
                g: Gestures::new(cfg),
                mode: Mode::Scroll,
                probed: Vec::new(),
                log: Vec::new(),
            }
        }
        fn acts(&mut self, acts: Vec<Act>, now: Ms) {
            for a in acts {
                let mode = self.mode;
                let probed = &mut self.probed;
                let mut probe = |x, y| {
                    probed.push((x, y));
                    mode
                };
                for o in self.g.on_act(a, now, &mut probe) {
                    self.log.push((now, o));
                }
            }
        }
        /// A finger report at panel (x, y); 800x1280 maps 1:1 (x-1 / y-1 after calibrate scaling
        /// is avoided by working in whatever the map returns: positions are asserted relative).
        fn at(&mut self, x: i32, y: i32, now: Ms) {
            let mut acts = Vec::new();
            for e in [
                raw(EV_ABS, ABS_MT_POSITION_X, x),
                raw(EV_ABS, ABS_MT_POSITION_Y, y),
                raw(EV_SYN, SYN_MT_REPORT, 0),
                raw(EV_SYN, SYN_REPORT, 0),
            ] {
                acts.extend(self.tm.on_event(&e, now, 0));
            }
            self.acts(acts, now);
        }
        fn two(&mut self, a: (i32, i32), b: (i32, i32), now: Ms) {
            let mut acts = Vec::new();
            for e in [
                raw(EV_ABS, ABS_MT_POSITION_X, a.0),
                raw(EV_ABS, ABS_MT_POSITION_Y, a.1),
                raw(EV_SYN, SYN_MT_REPORT, 0),
                raw(EV_ABS, ABS_MT_POSITION_X, b.0),
                raw(EV_ABS, ABS_MT_POSITION_Y, b.1),
                raw(EV_SYN, SYN_MT_REPORT, 0),
                raw(EV_SYN, SYN_REPORT, 0),
            ] {
                acts.extend(self.tm.on_event(&e, now, 0));
            }
            self.acts(acts, now);
        }
        /// Run the timers up to `until` in 20 ms steps (the touch map lifts 150 ms after the
        /// last report).
        fn run(&mut self, from: Ms, until: Ms) {
            let mut t = from;
            while t <= until {
                let a = self.tm.tick(t);
                self.acts(a, t);
                for o in self.g.tick(t) {
                    self.log.push((t, o));
                }
                t += 20;
            }
        }
        fn outs(&self) -> Vec<Out> {
            self.log.iter().map(|(_, o)| *o).collect()
        }
        fn wheels(&self, button: u8) -> usize {
            self.outs()
                .iter()
                .filter(|o| matches!(o, Out::Wheel { button: b, ctrl: false } if *b == button))
                .count()
        }
    }

    const CLICK: [Out; 2] = [Out::Button(1, true), Out::Button(1, false)];

    fn is_click(o: &[Out]) -> bool {
        o.len() == 3 && matches!(o[0], Out::Move(..)) && o[1..] == CLICK
    }

    #[test]
    fn tap_clicks_at_the_touch_point_after_the_lift_delay_only() {
        let mut s = Sim::new();
        s.at(300, 400, 1000);
        s.at(302, 401, 1030);
        assert!(s.log.is_empty(), "nothing before the lift: {:?}", s.log);
        s.run(1040, 1300);
        let o = s.outs();
        assert!(is_click(&o), "{o:?}");
        let want = s.tm.calibrate(300, 400);
        assert_eq!(o[0], Out::Move(want.0, want.1));
        // the click time is the lift detection (> 150 ms after the last report), not later
        let t = s.log[1].0;
        assert!((1181..=1200).contains(&t), "click at {t}");
        assert_eq!(s.probed.len(), 1, "one probe per touch");
    }

    #[test]
    fn tap_position_is_the_first_sample() {
        let mut s = Sim::new();
        s.at(100, 200, 0);
        s.at(105, 203, 30);
        s.run(40, 400);
        let first = s.log.iter().find_map(|(_, o)| match o {
            Out::Move(x, y) => Some((*x, *y)),
            _ => None,
        });
        let want = s.tm.calibrate(100, 200);
        assert_eq!(first, Some(want));
    }

    #[test]
    fn vertical_drag_emits_wheel_clicks_proportional_to_the_distance() {
        let mut s = Sim::new();
        // finger moves up (y decreases) 240 px in 12 steps: 5 clicks of 48 px, content scrolls down
        let mut y = 900;
        let mut t = 0;
        s.at(400, y, t);
        for _ in 0..12 {
            y -= 20;
            t += 16;
            s.at(400, y, t);
        }
        let o = s.outs();
        assert!(matches!(o[0], Out::Move(..)), "pointer to the start first");
        // calibrate() scales 800x1280 panel -> 800x1280: 1:1 here, within rounding
        assert!((4..=5).contains(&s.wheels(WHEEL_DOWN)), "{o:?}");
        assert_eq!(s.wheels(WHEEL_UP), 0);
        assert_eq!(
            s.wheels(WHEEL_LEFT) + s.wheels(WHEEL_RIGHT),
            0,
            "axis locked"
        );
        assert!(
            !o.contains(&Out::Button(1, true)),
            "no button press while scrolling"
        );
        // no click at the end
        s.run(t + 20, t + 300);
        assert!(!s.outs().iter().any(|o| matches!(o, Out::Button(..))));
    }

    #[test]
    fn drag_down_scrolls_up_and_horizontal_uses_buttons_6_and_7() {
        let mut s = Sim::new();
        s.at(400, 300, 0);
        for i in 1..=10 {
            s.at(400, 300 + i * 20, i as Ms * 16);
        }
        assert!(s.wheels(WHEEL_UP) >= 3);
        s.run(200, 600);
        s.log.clear();
        s.at(600, 500, 1000);
        for i in 1..=10 {
            s.at(600 - i * 20, 500, 1000 + i as Ms * 16);
        }
        // finger left = content left = view scrolls right
        assert!(s.wheels(WHEEL_RIGHT) >= 3, "{:?}", s.outs());
        assert_eq!(s.wheels(WHEEL_LEFT), 0);
    }

    #[test]
    fn small_jitter_is_a_tap_not_a_scroll() {
        let mut s = Sim::new();
        s.at(300, 300, 0);
        for i in 1..6 {
            s.at(300 + (i % 3), 300 - (i % 2) * 4, i as Ms * 16);
        }
        s.run(100, 400);
        assert!(is_click(&s.outs()), "{:?}", s.outs());
    }

    #[test]
    fn scroll_threshold_is_12_px() {
        let mut s = Sim::new();
        s.at(300, 300, 0);
        s.at(300, 311, 16);
        s.run(20, 300);
        assert!(is_click(&s.outs()), "11 px is still a tap");
        let mut s = Sim::new();
        s.at(300, 300, 0);
        s.at(300, 313, 16);
        s.at(300, 380, 32);
        assert!(s.wheels(WHEEL_UP) >= 1);
    }

    #[test]
    fn the_wheel_rate_is_capped_per_call() {
        let mut s = Sim::new();
        s.at(400, 100, 0);
        s.at(400, 1200, 16); // 1100 px in one report
        assert_eq!(s.wheels(WHEEL_UP), 8, "max_clicks per call");
        s.at(400, 1200, 32);
        assert!(s.wheels(WHEEL_UP) > 8, "the rest follows");
    }

    #[test]
    fn fling_continues_after_the_lift_and_decays_within_400_ms() {
        let mut s = Sim::new();
        // fast flick: 400 px in 100 ms = 4 px/ms (capped to 3)
        s.at(400, 1000, 0);
        for i in 1..=5 {
            s.at(400, 1000 - i * 80, i as Ms * 20);
        }
        let during = s.wheels(WHEEL_DOWN);
        s.run(120, 2000);
        let total = s.wheels(WHEEL_DOWN);
        assert!(
            total > during + 3,
            "fling added clicks: {during} -> {total}"
        );
        // the fling's clicks happen within fling_ms (400) of the lift (last report at 100 + 150)
        let lift = s.log.iter().map(|(t, _)| *t).max().unwrap();
        let lift_at = 100 + 150;
        assert!(lift <= lift_at + 20 + 400 + 20, "last click at {lift}");
        // decay: more clicks in the first half than in the second
        let tail: Vec<Ms> = s
            .log
            .iter()
            .filter(|(t, o)| *t > lift_at && matches!(o, Out::Wheel { .. }))
            .map(|(t, _)| *t)
            .collect();
        let mid = lift_at + 200;
        let (a, b) = (
            tail.iter().filter(|t| **t <= mid).count(),
            tail.iter().filter(|t| **t > mid).count(),
        );
        assert!(a > b, "decaying: {a} vs {b}");
        assert!(!s.g.active(), "fling over");
    }

    #[test]
    fn slow_drag_has_no_fling() {
        let mut s = Sim::new();
        s.at(400, 800, 0);
        for i in 1..=10 {
            s.at(400, 800 - i * 6, i as Ms * 60); // 0.1 px/ms
        }
        let before = s.wheels(WHEEL_DOWN);
        s.run(600, 1500);
        assert_eq!(s.wheels(WHEEL_DOWN), before);
    }

    #[test]
    fn a_touch_stops_a_fling_and_does_not_click() {
        let mut s = Sim::new();
        s.at(400, 1000, 0);
        for i in 1..=5 {
            s.at(400, 1000 - i * 80, i as Ms * 20);
        }
        s.run(120, 300); // flinging now
        assert!(s.g.active());
        let n = s.log.len();
        s.at(300, 300, 320);
        s.run(340, 700);
        let after: Vec<Out> = s.log[n..].iter().map(|(_, o)| *o).collect();
        assert!(
            !after.iter().any(|o| matches!(o, Out::Button(..))),
            "the stopping tap does not click: {after:?}"
        );
        assert!(
            !after.iter().any(|o| matches!(o, Out::Wheel { .. })),
            "fling stopped"
        );
    }

    #[test]
    fn long_press_then_lift_is_a_right_click() {
        let mut s = Sim::new();
        s.at(300, 400, 0);
        for i in 1..=30 {
            s.at(301 + i % 2, 400, i as Ms * 20); // held 600 ms with a little noise
            s.run(i as Ms * 20 + 1, i as Ms * 20 + 19);
        }
        assert!(s.log.is_empty(), "nothing until the lift: {:?}", s.log);
        s.run(700, 1000);
        let o = s.outs();
        assert!(
            matches!(o[0], Out::Move(..))
                && o[1] == Out::Button(3, true)
                && o[2] == Out::Button(3, false),
            "{o:?}"
        );
        assert_eq!(o.len(), 3);
    }

    #[test]
    fn a_hold_shorter_than_500_ms_is_a_left_click() {
        let mut s = Sim::new();
        s.at(300, 400, 0);
        for i in 1..=20 {
            s.at(300, 400, i as Ms * 20);
            s.run(i as Ms * 20 + 1, i as Ms * 20 + 19);
        }
        s.run(420, 800);
        assert!(is_click(&s.outs()), "{:?}", s.outs());
    }

    #[test]
    fn long_press_then_drag_selects() {
        let mut s = Sim::new();
        s.at(300, 400, 0);
        for i in 1..=28 {
            s.at(300, 400, i as Ms * 20);
            s.run(i as Ms * 20 + 1, i as Ms * 20 + 19);
        }
        assert!(s.log.is_empty());
        // now drag right
        s.at(330, 402, 600);
        s.at(380, 402, 620);
        s.at(440, 402, 640);
        let o = s.outs();
        let start = s.tm.calibrate(300, 400);
        assert_eq!(o[0], Out::Move(start.0, start.1), "back to the press point");
        assert_eq!(o[1], Out::Button(1, true));
        assert!(matches!(o[2], Out::Move(..)));
        assert!(
            !o.iter().any(|x| matches!(x, Out::Wheel { .. })),
            "a selection never scrolls"
        );
        let moves = o.iter().filter(|x| matches!(x, Out::Move(..))).count();
        assert!(moves >= 4, "{o:?}");
        s.run(641, 900);
        assert_eq!(
            s.outs().last(),
            Some(&Out::Button(1, false)),
            "released at the lift"
        );
        assert_eq!(
            s.outs()
                .iter()
                .filter(|x| **x == Out::Button(1, true))
                .count(),
            1
        );
    }

    #[test]
    fn two_quick_taps_are_a_double_click_on_the_same_spot() {
        let mut s = Sim::new();
        s.at(300, 400, 0);
        s.run(20, 200);
        s.log.clear();
        // second tap 12 px away, 120 ms after the first click
        s.at(312, 406, 320);
        s.run(340, 560);
        let o = s.outs();
        assert!(is_click(&o), "{o:?}");
        let first = s.tm.calibrate(300, 400);
        assert_eq!(
            o[0],
            Out::Move(first.0, first.1),
            "snapped onto the first click"
        );
    }

    #[test]
    fn a_late_or_distant_second_tap_is_a_separate_click() {
        let mut s = Sim::new();
        s.at(300, 400, 0);
        s.run(20, 200);
        s.log.clear();
        s.at(500, 700, 320);
        s.run(340, 560);
        let want = s.tm.calibrate(500, 700);
        assert_eq!(s.outs()[0], Out::Move(want.0, want.1));
        s.log.clear();
        s.at(502, 703, 2000);
        s.run(2020, 2300);
        let want = s.tm.calibrate(502, 703);
        assert_eq!(
            s.outs()[0],
            Out::Move(want.0, want.1),
            "no snap after 450 ms"
        );
    }

    #[test]
    fn mouse_mode_passes_the_pointer_through_unchanged() {
        let mut s = Sim::new();
        s.mode = Mode::Mouse;
        s.at(300, 400, 0);
        s.at(300, 500, 16);
        s.at(300, 700, 32);
        let p0 = s.tm.calibrate(300, 400);
        assert_eq!(
            s.outs(),
            vec![
                Out::Move(p0.0, p0.1),
                Out::Button(1, true),
                Out::Move(p0.0, s.tm.calibrate(300, 500).1),
                Out::Move(p0.0, s.tm.calibrate(300, 700).1),
            ]
        );
        s.run(100, 400);
        assert_eq!(s.outs().last(), Some(&Out::Button(1, false)));
        assert!(!s.outs().iter().any(|o| matches!(o, Out::Wheel { .. })));
        assert!(!s.g.active());
    }

    #[test]
    fn disabled_gestures_never_probe() {
        let mut s = Sim::with(Cfg {
            enabled: false,
            ..Cfg::default()
        });
        s.at(300, 400, 0);
        assert!(s.probed.is_empty());
        assert!(s.outs().contains(&Out::Button(1, true)));
    }

    #[test]
    fn the_probe_sees_the_press_point_once_per_touch() {
        let mut s = Sim::new();
        s.at(100, 200, 0);
        s.at(100, 300, 16);
        s.run(30, 300);
        s.at(500, 600, 400);
        s.run(420, 700);
        let a = s.tm.calibrate(100, 200);
        let b = s.tm.calibrate(500, 600);
        assert_eq!(s.probed, vec![a, b]);
    }

    #[test]
    fn mode_parses_apps_json_values() {
        assert_eq!(Mode::parse("mouse"), Some(Mode::Mouse));
        assert_eq!(Mode::parse(" Scroll "), Some(Mode::Scroll));
        assert_eq!(Mode::parse("drag"), None);
    }

    #[test]
    fn pinch_zooms_with_ctrl_wheel_and_never_clicks() {
        let mut s = Sim::new();
        s.at(300, 600, 0);
        s.two((300, 600), (400, 600), 16); // first two-finger frame: not confirmed yet
                                           // fingers move apart: 100 -> 260 px
        for i in 1..=8 {
            s.two((300 - i * 10, 600), (400 + i * 10, 600), 16 + i as Ms * 16);
        }
        let zoom_in = s
            .outs()
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    Out::Wheel {
                        button: WHEEL_UP,
                        ctrl: true
                    }
                )
            })
            .count();
        assert!((3..=4).contains(&zoom_in), "{:?}", s.outs());
        // and back together: zoom out
        for i in 1..=8 {
            s.two((220 + i * 10, 600), (480 - i * 10, 600), 200 + i as Ms * 16);
        }
        assert!(s.outs().iter().any(|o| matches!(
            o,
            Out::Wheel {
                button: WHEEL_DOWN,
                ctrl: true
            }
        )));
        s.run(400, 800);
        assert!(
            !s.outs().iter().any(|o| matches!(o, Out::Button(..))),
            "no click after a pinch"
        );
        assert!(!s
            .outs()
            .iter()
            .any(|o| matches!(o, Out::Wheel { ctrl: false, .. })));
    }

    #[test]
    fn two_fingers_in_a_mouse_touch_are_ignored() {
        let mut s = Sim::new();
        s.mode = Mode::Mouse;
        s.at(300, 600, 0);
        s.two((300, 600), (400, 600), 16);
        s.two((290, 600), (410, 600), 32);
        s.two((250, 600), (450, 600), 48);
        assert!(!s.outs().iter().any(|o| matches!(o, Out::Wheel { .. })));
    }
}
