use super::*;

#[test]
fn rule2_short_press_toggles_or_activates() {
    let mut k = PowerKey::default();
    k.on_key(1, 0, false);
    assert_eq!(k.on_key(0, 999, false), vec![Act::ToggleScreen]);
    k.on_key(1, 2000, true);
    assert_eq!(k.on_key(0, 2100, true), vec![Act::ActivateMenuItem]);
}

#[test]
fn rule1_long_press_opens_menu_once() {
    let mut k = PowerKey::default();
    k.on_key(1, 0, false);
    assert!(k.tick(999, false, true).is_empty());
    assert_eq!(k.tick(1000, false, true), vec![Act::OpenPowerMenu]);
    assert!(k.tick(1100, false, true).is_empty(), "only once");
    assert!(
        k.on_key(0, 1200, true).is_empty(),
        "release after long does nothing"
    );
}

#[test]
fn rule1_long_press_wakes_screen_and_respects_open_menu() {
    let mut k = PowerKey::default();
    k.on_key(1, 0, false);
    assert_eq!(
        k.tick(1000, false, false),
        vec![Act::WakeScreen, Act::OpenPowerMenu]
    );
    let mut k = PowerKey::default();
    k.on_key(1, 0, true);
    assert!(k.tick(1000, true, true).is_empty());
}

#[test]
fn rule3_poll_interval() {
    let mut k = PowerKey::default();
    assert_eq!(k.poll_interval_ms(), 400);
    k.on_key(1, 0, false);
    assert_eq!(k.poll_interval_ms(), 100);
    k.on_key(0, 10, false);
    assert_eq!(k.poll_interval_ms(), 400);
}

#[test]
fn stray_release_and_repeat_ignored() {
    let mut k = PowerKey::default();
    assert!(k.on_key(0, 5, false).is_empty());
    k.on_key(1, 0, false);
    assert!(k.on_key(2, 50, false).is_empty());
}
