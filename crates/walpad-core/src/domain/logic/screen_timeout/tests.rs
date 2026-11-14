use super::*;

fn s() -> Screen {
    Screen::new(Cfg::default(), Some(150), 0)
}

#[test]
fn rule5_startup_brightness() {
    assert_eq!(Screen::new(Cfg::default(), Some(150), 0).brightness(), 150);
    assert_eq!(Screen::new(Cfg::default(), Some(0), 0).brightness(), 120);
    assert_eq!(Screen::new(Cfg::default(), None, 0).brightness(), 120);
}

#[test]
fn rule1_dim_then_off() {
    let mut sc = s();
    assert!(sc.tick(90_000).is_empty(), "strictly greater");
    assert_eq!(sc.tick(90_001), vec![Act::Backlight(20)]);
    assert_eq!(sc.state(), ScreenState::Dim);
    // idle timer restarts at the dim step (screen() resets last_act), off follows 240 s later
    assert!(sc.tick(90_001 + 240_000).is_empty());
    assert_eq!(sc.tick(90_001 + 240_001), vec![Act::Backlight(0)]);
    assert_eq!(sc.state(), ScreenState::Off);
}

#[test]
fn rule1_zero_disables_steps() {
    let mut sc = Screen::new(
        Cfg {
            dim_after_s: 0,
            off_after_s: 0,
            ..Cfg::default()
        },
        Some(150),
        0,
    );
    assert!(sc.tick(10_000_000).is_empty());
    assert_eq!(sc.state(), ScreenState::On);
}

#[test]
fn rule2_save_and_restore() {
    let mut sc = s();
    sc.screen(ScreenMode::Off, 1);
    assert_eq!(sc.brightness(), 0);
    assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(150)]);
    // leaving `on` with brightness 0 saves the default
    let mut sc = s();
    sc.brightness = 0;
    sc.screen(ScreenMode::Off, 1);
    assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(120)]);
}

#[test]
fn toggle_flips_off_and_on() {
    let mut sc = s();
    sc.screen(ScreenMode::Toggle, 1);
    assert_eq!(sc.state(), ScreenState::Off);
    sc.screen(ScreenMode::Toggle, 2);
    assert_eq!(sc.state(), ScreenState::On);
}

#[test]
fn rule3_activity_wakes_only_when_cover_open_and_swallows_key() {
    let mut sc = s();
    sc.screen(ScreenMode::Off, 1);
    let (acts, swallow) = sc.on_activity(Activity::Wakes, true, 10);
    assert_eq!(acts, vec![Act::Backlight(150)]);
    assert!(swallow);
    // touch wake does not swallow
    sc.screen(ScreenMode::Off, 20);
    assert!(!sc.on_activity(Activity::Wakes, false, 30).1);
    // key abs never wakes
    sc.screen(ScreenMode::Off, 40);
    let (acts, _) = sc.on_activity(Activity::NoWake, true, 50);
    assert!(acts.is_empty());
    assert_eq!(sc.state(), ScreenState::Off);
    // closed cover blocks wake
    sc.on_cover(true, 60);
    assert!(sc.on_activity(Activity::Wakes, false, 70).0.is_empty());
    assert_eq!(sc.state(), ScreenState::Off);
}

#[test]
fn activity_resets_idle_timer() {
    let mut sc = s();
    sc.on_activity(Activity::Wakes, false, 80_000);
    assert!(sc.tick(100_000).is_empty());
}

#[test]
fn rule4_brightness_commands() {
    let mut sc = s();
    assert_eq!(
        sc.brightness_cmd(Brightness::Delta(200)),
        vec![Act::Backlight(255)]
    );
    assert_eq!(
        sc.brightness_cmd(Brightness::Delta(-300)),
        vec![Act::Backlight(0)]
    );
    assert_eq!(sc.state(), ScreenState::Off);
    sc.brightness_cmd(Brightness::Absolute(90));
    assert_eq!((sc.state(), sc.brightness()), (ScreenState::On, 90));
    // saved value follows: off then on restores 90
    sc.screen(ScreenMode::Off, 1);
    assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(90)]);
}

#[test]
fn cover_only_when_lid_off() {
    let mut sc = Screen::new(
        Cfg {
            lid_off: false,
            ..Cfg::default()
        },
        Some(150),
        0,
    );
    assert!(sc.on_cover(true, 1).is_empty());
    assert_eq!(sc.state(), ScreenState::On);
    let mut sc = s();
    assert_eq!(sc.on_cover(true, 1), vec![Act::Backlight(0)]);
    assert!(sc.lid_closed());
    assert_eq!(sc.on_cover(false, 2), vec![Act::Backlight(150)]);
}
