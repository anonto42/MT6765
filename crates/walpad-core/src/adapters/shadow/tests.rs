use super::*;
use walpad_hw::evdev::{
    ABS_MT_POSITION_X, ABS_MT_POSITION_Y, EV_ABS, EV_KEY, EV_SW, EV_SYN, KEY_POWER, KEY_VOLUMEUP,
    SYN_REPORT,
};

fn ev(ms: i64, kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: ms / 1000,
        usec: (ms % 1000) * 1000,
        kind,
        code,
        value,
    }
}
fn new() -> Shadow {
    Shadow::new("", "", "", Some(150), 0)
}
fn lns(v: Vec<(Ms, String)>) -> Vec<String> {
    v.into_iter().map(|(_, l)| l).collect()
}
fn no_g() -> impl FnMut() -> Option<(i32, i32, i32)> {
    || None
}

#[test]
fn volume_single_tap_would_send_key_up() {
    let mut sh = new();
    let evs = vec![
        (Role::Key, ev(1000, EV_KEY, KEY_VOLUMEUP, 1)),
        (Role::Key, ev(1050, EV_KEY, KEY_VOLUMEUP, 0)),
    ];
    assert_eq!(
        lns(replay(&mut sh, &evs, 2000, &mut no_g())),
        vec!["WOULD action key:Up"]
    );
}

#[test]
fn power_short_press_toggles_screen_and_long_opens_menu() {
    let mut sh = new();
    let evs = vec![
        (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
        (Role::Key, ev(1200, EV_KEY, KEY_POWER, 0)),
    ];
    assert_eq!(
        lns(replay(&mut sh, &evs, 1500, &mut no_g())),
        vec!["WOULD backlight 0"]
    );
    let mut sh = new();
    let evs = vec![
        (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
        (Role::Key, ev(2500, EV_KEY, KEY_POWER, 0)),
    ];
    assert_eq!(
        lns(replay(&mut sh, &evs, 3000, &mut no_g())),
        vec!["WOULD powermenu open"]
    );
}

#[test]
fn touch_becomes_pointer_press_move_release() {
    let mut sh = new();
    let evs = vec![
        (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_X, 400)),
        (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_Y, 640)),
        (Role::Touch, ev(1000, EV_SYN, SYN_REPORT, 0)),
    ];
    assert_eq!(
        lns(replay(&mut sh, &evs, 1400, &mut no_g())),
        vec![
            "WOULD pointer move 399 639",
            "WOULD pointer press",
            "WOULD pointer release"
        ]
    );
}

#[test]
fn idle_dims_then_touch_wakes() {
    let mut sh = new();
    let evs = vec![
        (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_X, 1)),
        (Role::Touch, ev(100_000, EV_ABS, ABS_MT_POSITION_X, 1)),
    ];
    let out = lns(replay(&mut sh, &evs, 100_100, &mut no_g()));
    assert!(out.contains(&"WOULD backlight 20".to_string()), "{out:?}");
    assert!(out.contains(&"WOULD backlight 150".to_string()), "{out:?}");
}

#[test]
fn cover_close_turns_screen_off() {
    let mut sh = new();
    let evs = vec![(Role::Hall, ev(1000, EV_SW, 0, 1))];
    assert_eq!(
        lns(replay(&mut sh, &evs, 1100, &mut no_g())),
        vec!["WOULD cover closed", "WOULD backlight 0"]
    );
}

#[test]
fn tilt_rotates_after_three_samples() {
    let mut sh = new();
    let mut g = || Some((4000, 100, 0));
    let evs = vec![(Role::Key, ev(0, EV_ABS, 0, 0))];
    let out = lns(replay(&mut sh, &evs, 1500, &mut g));
    assert!(out.contains(&"WOULD orientation 90".to_string()), "{out:?}");
    assert_eq!(sh.orientation(), 90);
}
