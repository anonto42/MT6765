use super::*;

fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: 0,
        usec: 0,
        kind,
        code,
        value,
    }
}
fn touch(t: &mut TouchMap, x: i32, y: i32, now: Ms, o: u16) -> Vec<Act> {
    let mut out = t.on_event(&ev(EV_ABS, ABS_MT_POSITION_X, x), now, o);
    out.extend(t.on_event(&ev(EV_ABS, ABS_MT_POSITION_Y, y), now, o));
    out.extend(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), now, o));
    out
}

#[test]
fn rule2_rotation_table() {
    // panel point (100, 200)
    assert_eq!(to_logical(100, 200, 0), (100, 200));
    assert_eq!(to_logical(100, 200, 180), (699, 1079));
    assert_eq!(to_logical(100, 200, 90), (200, 699));
    assert_eq!(to_logical(100, 200, 270), (1079, 100));
}

#[test]
fn rule2_scale_clamp_flip_swap() {
    let t = TouchMap::new(Cfg::default());
    assert_eq!(t.calibrate(0, 0), (0, 0));
    assert_eq!(t.calibrate(800, 1280), (799, 1279));
    assert_eq!(t.calibrate(5000, -5), (799, 0));
    let t = TouchMap::new(Cfg {
        flipx: true,
        flipy: true,
        ..Cfg::default()
    });
    assert_eq!(t.calibrate(0, 0), (799, 1279));
    assert_eq!(t.calibrate(800, 1280), (0, 0));
    let t = TouchMap::new(Cfg {
        swap: true,
        xmax: 1280,
        ymax: 800,
        ..Cfg::default()
    });
    assert_eq!(t.calibrate(0, 0), (0, 0));
    // swap: x input feeds the y axis (range 0-800 mapped to 1280), y feeds x
    assert_eq!(t.calibrate(400, 0), (0, 639));
    let t = TouchMap::new(Cfg {
        xmin: 100,
        xmax: 700,
        ..Cfg::default()
    });
    assert_eq!(t.calibrate(100, 0).0, 0);
    assert_eq!(t.calibrate(700, 0).0, 799);
}

#[test]
fn rule1_3_press_on_first_report_then_only_moves() {
    let mut t = TouchMap::new(Cfg::default());
    assert_eq!(
        touch(&mut t, 400, 640, 10, 0),
        vec![Act::Move(399, 639), Act::Press]
    );
    assert_eq!(touch(&mut t, 500, 640, 20, 0), vec![Act::Move(499, 639)]);
}

#[test]
fn rule1_alt_codes_and_btn_touch_counts_as_movement() {
    let mut t = TouchMap::new(Cfg::default());
    t.on_event(&ev(EV_ABS, ABS_X, 10), 1, 0);
    t.on_event(&ev(EV_ABS, ABS_Y, 20), 1, 0);
    t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1, 0); // consumes moved
    assert!(
        t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 2, 0).is_empty(),
        "no movement: no report"
    );
    t.on_event(&ev(EV_KEY, BTN_TOUCH, 1), 3, 0);
    assert!(!t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 3, 0).is_empty());
}

#[test]
fn syn_without_both_coordinates_is_ignored() {
    let mut t = TouchMap::new(Cfg::default());
    t.on_event(&ev(EV_ABS, ABS_MT_POSITION_X, 5), 1, 0);
    assert!(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1, 0).is_empty());
    assert!(!t.is_down());
}

#[test]
fn rule4_lift_after_150ms() {
    let mut t = TouchMap::new(Cfg::default());
    touch(&mut t, 400, 640, 1000, 0);
    assert!(t.tick(1150).is_empty(), "strictly greater than 150 ms");
    assert_eq!(t.tick(1151), vec![Act::Release]);
    assert!(!t.is_down());
    // position forgotten: a lone SYN does nothing, next touch presses again
    assert!(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1200, 0).is_empty());
    assert_eq!(touch(&mut t, 10, 10, 2000, 0).last(), Some(&Act::Press));
}

#[test]
fn btn_touch_zero_releases_immediately_for_fast_consecutive_taps() {
    let mut t = TouchMap::new(Cfg::default());
    touch(&mut t, 100, 1000, 1000, 0);
    assert!(t.is_down());
    assert_eq!(
        t.on_event(&ev(EV_KEY, BTN_TOUCH, 0), 1020, 0),
        vec![Act::Release]
    );
    assert!(!t.is_down());
    // Next tap 10 ms later presses immediately instead of being swallowed as a Move
    assert_eq!(
        touch(&mut t, 200, 1000, 1030, 0),
        vec![Act::Move(199, 999), Act::Press]
    );
}

#[test]
fn rule4_events_extend_the_touch() {
    let mut t = TouchMap::new(Cfg::default());
    touch(&mut t, 400, 640, 1000, 0);
    touch(&mut t, 401, 640, 1100, 0);
    assert!(t.tick(1200).is_empty());
}

fn two(t: &mut TouchMap, a: (i32, i32), b: (i32, i32), now: Ms) -> Vec<Act> {
    let mut out = Vec::new();
    for e in [
        ev(EV_ABS, ABS_MT_POSITION_X, a.0),
        ev(EV_ABS, ABS_MT_POSITION_Y, a.1),
        ev(EV_SYN, SYN_MT_REPORT, 0),
        ev(EV_ABS, ABS_MT_POSITION_X, b.0),
        ev(EV_ABS, ABS_MT_POSITION_Y, b.1),
        ev(EV_SYN, SYN_MT_REPORT, 0),
        ev(EV_SYN, SYN_REPORT, 0),
    ] {
        out.extend(t.on_event(&e, now, 0));
    }
    out
}

#[test]
fn protocol_a_single_contact_frames_are_unchanged() {
    let mut t = TouchMap::new(Cfg::default());
    let mut out = Vec::new();
    for e in [
        ev(EV_ABS, ABS_MT_POSITION_X, 400),
        ev(EV_ABS, ABS_MT_POSITION_Y, 640),
        ev(EV_SYN, SYN_MT_REPORT, 0),
        ev(EV_SYN, SYN_REPORT, 0),
    ] {
        out.extend(t.on_event(&e, 10, 0));
    }
    assert_eq!(out, vec![Act::Move(399, 639), Act::Press]);
    // an empty frame (finger up) reports nothing; the lift timer releases
    let mut out = t.on_event(&ev(EV_SYN, SYN_MT_REPORT, 0), 30, 0);
    out.extend(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 30, 0));
    assert!(out.is_empty());
    assert_eq!(t.tick(200), vec![Act::Release]);
}

#[test]
fn two_contacts_become_a_pair_after_two_agreeing_frames() {
    let mut t = TouchMap::new(Cfg::default());
    assert_eq!(
        two(&mut t, (100, 200), (300, 200), 10),
        vec![Act::Move(99, 199), Act::Press],
        "first frame: the first contact is the pointer"
    );
    assert_eq!(
        two(&mut t, (100, 200), (300, 200), 30),
        vec![Act::Pair((99, 199), (299, 199))],
        "the second two-finger frame confirms it"
    );
    assert_eq!(
        two(&mut t, (90, 200), (310, 200), 50),
        vec![Act::Pair((89, 199), (309, 199))]
    );
    // back to one finger: the pair count starts over
    assert_eq!(touch(&mut t, 90, 200, 70, 0), vec![Act::Move(89, 199)]);
    assert_eq!(
        two(&mut t, (90, 200), (310, 200), 90),
        vec![Act::Move(89, 199)],
        "a single frame in between unconfirms the pair"
    );
}

#[test]
fn multitouch_off_keeps_the_single_pointer() {
    let mut t = TouchMap::new(Cfg {
        multitouch: false,
        ..Cfg::default()
    });
    two(&mut t, (100, 200), (300, 200), 10);
    two(&mut t, (100, 200), (300, 200), 30);
    let out = two(&mut t, (90, 200), (310, 200), 50);
    assert!(out.iter().all(|a| matches!(a, Act::Move(..))), "{out:?}");
}

#[test]
fn rule5_orientation_cache() {
    let mut c = OrientationCache::default();
    assert_eq!(c.get(0, || Some("90\n".into())), 90);
    assert_eq!(c.get(299, || Some("180".into())), 90, "cached");
    assert_eq!(c.get(301, || Some("180".into())), 180);
    assert_eq!(c.get(700, || Some("45".into())), 0, "invalid -> 0");
    assert_eq!(c.get(1100, || None), 0, "unreadable -> 0");
}
