//! Long-press, double-tap, mouse-mode, and pinch gesture tests.

use super::tests::{is_click, Sim};
use super::*;
use crate::logic::Ms;

#[test]
fn long_press_then_lift_is_a_double_click() {
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
            && o[1] == Out::Button(BTN_LEFT, true)
            && o[2] == Out::Button(BTN_LEFT, false)
            && o[3] == Out::Button(BTN_LEFT, true)
            && o[4] == Out::Button(BTN_LEFT, false),
        "{o:?}"
    );
    assert_eq!(o.len(), 5);
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
