use super::*;

fn key(code: u16, value: i32) -> InputEvent {
    InputEvent {
        sec: 0,
        usec: 0,
        kind: EV_KEY,
        code,
        value,
    }
}
fn brain() -> Brain {
    Brain::new("", "", "", Some(120), 0)
}
fn tick(b: &mut Brain, now: Ms) -> Vec<Effect> {
    b.tick(now, &mut || None, &mut RawBattery::default)
}

#[test]
fn long_power_press_opens_the_menu_once_and_short_press_activates_it() {
    let mut b = brain();
    assert!(b.on_event(Role::Key, &key(KEY_POWER, 1), 0).is_empty());
    assert!(!tick(&mut b, 900).contains(&Effect::PowerMenuOpen));
    assert!(tick(&mut b, 1000).contains(&Effect::PowerMenuOpen));
    assert!(!tick(&mut b, 1100).contains(&Effect::PowerMenuOpen));
    b.on_event(Role::Key, &key(KEY_POWER, 0), 1200);
    // menu open: a short press activates the focused item instead of toggling the screen
    b.menu_open = true;
    b.on_event(Role::Key, &key(KEY_POWER, 1), 2000);
    let fx = b.on_event(Role::Key, &key(KEY_POWER, 0), 2100);
    assert_eq!(fx, vec![Effect::PowerMenuActivate]);
}

#[test]
fn volume_keys_go_to_the_shell_while_its_menu_is_open() {
    let mut b = brain();
    b.shell_keys = true;
    let down = b.on_event(Role::Key, &key(KEY_VOLUMEDOWN, 1), 10);
    let up = b.on_event(Role::Key, &key(KEY_VOLUMEUP, 1), 20);
    assert!(down.contains(&Effect::ShellKey(ShellInput::VolumeDown)));
    assert!(up.contains(&Effect::ShellKey(ShellInput::VolumeUp)));
    // releases and the later tick produce no volume gesture
    let mut all = b.on_event(Role::Key, &key(KEY_VOLUMEDOWN, 0), 60);
    all.extend(b.on_event(Role::Key, &key(KEY_VOLUMEUP, 0), 80));
    all.extend(tick(&mut b, 800));
    assert!(
        !all.iter().any(|e| matches!(e, Effect::Action(_))),
        "{all:?}"
    );
}

#[test]
fn volume_keys_follow_the_gestures_when_no_menu_is_open() {
    let mut b = brain();
    let fx = b.on_event(Role::Key, &key(KEY_VOLUMEUP, 1), 10);
    assert!(!fx.iter().any(|e| matches!(e, Effect::ShellKey(_))));
    b.on_event(Role::Key, &key(KEY_VOLUMEUP, 0), 60);
    let fx = tick(&mut b, 600);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::Action(_))),
        "single press after the tap window is a volume action: {fx:?}"
    );
}

#[test]
fn a_short_power_press_that_turns_the_screen_off_locks_and_waking_does_not() {
    let mut b = brain();
    b.on_event(Role::Key, &key(KEY_POWER, 1), 1000);
    let mut fx = b.on_event(Role::Key, &key(KEY_POWER, 0), 1100);
    fx.extend(tick(&mut b, 1200));
    assert_eq!(b.screen.state(), ScreenState::Off);
    assert!(fx.contains(&Effect::LockNow), "{fx:?}");
    // the press that wakes it again locks nothing
    b.on_event(Role::Key, &key(KEY_POWER, 1), 5000);
    let mut fx = b.on_event(Role::Key, &key(KEY_POWER, 0), 5100);
    fx.extend(tick(&mut b, 5200));
    assert_ne!(b.screen.state(), ScreenState::Off);
    assert!(!fx.contains(&Effect::LockNow), "{fx:?}");
}
