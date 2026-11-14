use super::*;

#[test]
fn rule2_decide_table() {
    let c = Cfg::default();
    for (x, y, want) in [
        (0, 0, None),
        (2999, 100, None),
        (4000, 100, Some(90)),
        (-4000, 100, Some(270)),
        (100, 4000, Some(0)),
        (100, -4000, Some(180)),
        (4000, 3500, None), // inside the ratio dead zone
        (3000, 0, Some(90)),
        (1350, 1000, None), // below tilt_min
        (5400, 4000, None), // exactly at ratio: not strictly greater
        (5401, 4000, Some(90)),
    ] {
        assert_eq!(decide(x, y, &c), want, "x {x} y {y}");
    }
}

#[test]
fn rule2_flip() {
    let c = Cfg {
        rotate_flip: true,
        ..Cfg::default()
    };
    assert_eq!(decide(100, 4000, &c), Some(180));
    assert_eq!(decide(100, -4000, &c), Some(0));
    assert_eq!(decide(4000, 100, &c), Some(270));
    assert_eq!(decide(-4000, 100, &c), Some(90));
}

#[test]
fn rule3_debounce() {
    let mut r = Rotation::new(Cfg::default());
    let g = Some((4000, 100, 0));
    assert!(r.sample(g).is_empty());
    assert!(r.sample(g).is_empty());
    assert_eq!(
        r.sample(g),
        vec![
            Act::WriteOrientation(90),
            Act::SetStruts(90),
            Act::DockKeyboard(90)
        ]
    );
    assert_eq!(r.orientation(), 90);
}

#[test]
fn rule3_no_change_sample_resets_candidate() {
    let mut r = Rotation::new(Cfg::default());
    let g = Some((4000, 100, 0));
    r.sample(g);
    r.sample(g);
    r.sample(Some((0, 0, 9000))); // flat: resets
    assert!(r.sample(g).is_empty());
    assert!(r.sample(g).is_empty());
    assert!(!r.sample(g).is_empty());
    // a different candidate restarts the count
    let mut r = Rotation::new(Cfg::default());
    r.sample(g);
    r.sample(g);
    assert!(r.sample(Some((-4000, 100, 0))).is_empty());
    assert_eq!(r.orientation(), 0);
}

#[test]
fn missing_sample_does_nothing() {
    let mut r = Rotation::new(Cfg::default());
    assert!(r.sample(None).is_empty());
}

#[test]
fn rule1_sampling_conditions() {
    let mut r = Rotation::new(Cfg::default());
    assert!(r.wants_sample(false));
    assert!(!r.wants_sample(true));
    r.command(RotateMode::Lock);
    assert!(!r.wants_sample(false));
    let r2 = Rotation::new(Cfg {
        auto_rotate: false,
        ..Cfg::default()
    });
    assert!(!r2.wants_sample(false));
}

#[test]
fn rule4_5_geometry_and_struts() {
    assert_eq!(strut_partial(0), [0, 480, 0, 0, 0, 0, 0, 1279, 0, 0, 0, 0]);
    assert_eq!(strut_partial(180), strut_partial(0));
    assert_eq!(strut_partial(90), [0, 0, 0, 480, 0, 0, 0, 0, 0, 0, 0, 1279]);
    assert_eq!(strut_partial(270), strut_partial(90));
    assert_eq!(strut(0), [0, 480, 0, 0]);
    assert_eq!(strut(90), [0, 0, 0, 480]);
    assert_eq!(keyboard_geometry(0), (0, 896, 800, 384));
    assert_eq!(keyboard_geometry(90), (0, 560, 1280, 240));
}

#[test]
fn rule5_starts_at_zero_and_writes_file() {
    let r = Rotation::new(Cfg::default());
    assert_eq!(r.orientation(), 0);
    assert_eq!(r.init()[0], Act::WriteOrientation(0));
}

#[test]
fn rule6_commands() {
    let mut r = Rotation::new(Cfg::default());
    r.command(RotateMode::Toggle);
    assert!(r.locked());
    r.command(RotateMode::Toggle);
    assert!(!r.locked());
    r.command(RotateMode::Lock);
    r.command(RotateMode::Auto);
    assert!(!r.locked());
    let acts = r.command(RotateMode::R180);
    assert!(r.locked());
    assert_eq!(r.orientation(), 180);
    assert_eq!(acts[0], Act::WriteOrientation(180));
    assert!(
        r.command(RotateMode::R180).is_empty(),
        "same angle: nothing to apply"
    );
}
