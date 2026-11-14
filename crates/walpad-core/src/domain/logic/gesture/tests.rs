//! Gesture test harness plus tap, scroll, and fling tests.

use super::recogniser::P;
use super::*;
use crate::logic::touch_map::{Act, Cfg as TmCfg, TouchMap};
use crate::logic::Ms;
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
pub(super) struct Sim {
    pub(super) tm: TouchMap,
    pub(super) g: Gestures,
    pub(super) mode: Mode,
    pub(super) probed: Vec<P>,
    pub(super) log: Vec<(Ms, Out)>,
}

impl Sim {
    pub(super) fn new() -> Sim {
        Sim::with(Cfg::default())
    }

    pub(super) fn with(cfg: Cfg) -> Sim {
        Sim {
            tm: TouchMap::new(TmCfg::default()),
            g: Gestures::new(cfg),
            mode: Mode::Scroll,
            probed: Vec::new(),
            log: Vec::new(),
        }
    }

    pub(super) fn acts(&mut self, acts: Vec<Act>, now: Ms) {
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
    pub(super) fn at(&mut self, x: i32, y: i32, now: Ms) {
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

    pub(super) fn two(&mut self, a: (i32, i32), b: (i32, i32), now: Ms) {
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
    pub(super) fn run(&mut self, from: Ms, until: Ms) {
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

    pub(super) fn outs(&self) -> Vec<Out> {
        self.log.iter().map(|(_, o)| *o).collect()
    }

    pub(super) fn wheels(&self, button: u8) -> usize {
        self.outs()
            .iter()
            .filter(|o| matches!(o, Out::Wheel { button: b, ctrl: false } if *b == button))
            .count()
    }
}

const CLICK: [Out; 2] = [Out::Button(1, true), Out::Button(1, false)];

pub(super) fn is_click(o: &[Out]) -> bool {
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
