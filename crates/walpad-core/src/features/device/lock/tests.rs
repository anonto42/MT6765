use super::storage::{dec, enc, Stored};
use super::*;
use std::os::unix::fs::PermissionsExt;

fn root(name: &str) -> (std::path::PathBuf, Root) {
    let d = std::env::temp_dir().join(format!("walpad-lock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let r = Root::new(&d);
    (d, r)
}

fn fresh(name: &str) -> (std::path::PathBuf, Lock) {
    let (d, r) = root(name);
    let mut l = Lock::new(&r, 0);
    l.set_iterations(8);
    (d, l)
}

#[test]
fn letter_hex_round_trips_and_has_no_digits() {
    let b: Vec<u8> = (0..=255).collect();
    let e = enc(&b);
    assert!(e.bytes().all(|c| (b'a'..=b'p').contains(&c)));
    assert_eq!(dec(&e).unwrap(), b);
    assert!(dec("abc").is_none() && dec("ab1c").is_none());
}

#[test]
fn setting_a_pin_stores_a_private_salted_hash_without_the_pin() {
    let (d, mut l) = fresh("set");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    let f = d.join("var/lib/walpad/lock.json");
    let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(!text.contains("4321"), "{text}");
    assert!(text.contains("pbkdf2-sha256"));
    // salted: two locks with the same PIN differ
    let first: Stored = serde_json::from_str(&text).unwrap();
    l.set(LockKind::Pin, "4321", Some("4321"), 0).unwrap();
    let second: Stored = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
    assert_ne!(first.salt, second.salt);
    assert_ne!(first.hash, second.hash);
    assert!(l.verify("4321") && !l.verify("4322"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_new_secret_must_fit_its_kind_and_changing_needs_the_current_one() {
    let (d, mut l) = fresh("kind");
    assert!(matches!(
        l.set(LockKind::Pin, "12", None, 0),
        Err(LockError::Invalid(_))
    ));
    assert!(matches!(
        l.set(LockKind::Pattern, "0120", None, 0),
        Err(LockError::Invalid(_))
    ));
    l.set(LockKind::Password, "correct horse", None, 0).unwrap();
    assert_eq!(
        l.set(LockKind::Pin, "1234", None, 0),
        Err(LockError::WrongSecret { retry_in_s: None })
    );
    assert!(matches!(
        l.set(LockKind::Pin, "1234", Some("nope nope"), 0),
        Err(LockError::WrongSecret { .. })
    ));
    l.set(LockKind::Pin, "1234", Some("correct horse"), 0)
        .unwrap();
    assert_eq!(l.status(0).kind, Some(LockKind::Pin));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn five_wrong_secrets_refuse_for_30_s_even_the_right_one_then_double() {
    let (d, mut l) = fresh("rate");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    l.lock_now();
    let mut t = 1000;
    for i in 0..4 {
        assert_eq!(
            l.unlock("0000", t),
            Unlock::Wrong {
                attempts_left: 4 - i
            }
        );
        t += 100;
    }
    // the 5th wrong one starts the refusal
    assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 30 });
    assert_eq!(l.status(t).retry_in_s, Some(30));
    // the right PIN during the delay is refused and does not unlock
    assert_eq!(l.unlock("4321", t + 5000), Unlock::Wait { retry_in_s: 25 });
    assert!(l.locked());
    // refused attempts are not counted: after 30 s five more are allowed...
    t += 30_000;
    for _ in 0..4 {
        assert!(matches!(l.unlock("0000", t), Unlock::Wrong { .. }));
    }
    // ...and the next burst waits 60 s
    assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 60 });
    t += 60_000;
    for _ in 0..4 {
        l.unlock("0000", t);
    }
    assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 120 });
    // a right PIN after the delay unlocks and resets the count
    t += 120_000;
    assert_eq!(l.unlock("4321", t), Unlock::Unlocked);
    assert!(!l.locked());
    l.lock_now();
    assert_eq!(l.unlock("0000", t), Unlock::Wrong { attempts_left: 4 });
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn the_delay_is_capped_and_survives_a_restart() {
    let (d, mut l) = fresh("cap");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    l.lock_now();
    let mut t = 0;
    for _ in 0..80 {
        t += 3_600_000 + 1;
        l.unlock("0000", t);
    }
    assert!(l.status(t).retry_in_s.unwrap_or(0) <= 3600);
    // a restarted core (same boot) still refuses
    let r = Root::new(&d);
    let mut l2 = Lock::new(&r, t);
    l2.set_iterations(8);
    assert!(l2.locked() && l2.status(t).retry_in_s.is_some());
    assert!(matches!(l2.unlock("4321", t + 1), Unlock::Wait { .. }));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn changing_or_removing_counts_wrong_current_secrets_too() {
    let (d, mut l) = fresh("remove");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    for _ in 0..5 {
        assert!(l.remove("0000", 10).is_err());
    }
    assert!(matches!(
        l.remove("4321", 11),
        Err(LockError::Wait { retry_in_s: 30 })
    ));
    l.remove("4321", 31_000).unwrap();
    assert!(!l.configured());
    assert!(!d.join("var/lib/walpad/lock.json").exists());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn first_start_after_boot_is_locked_a_restart_keeps_the_state() {
    let (d, mut l) = fresh("boot");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    assert!(!l.locked(), "setting a lock does not lock the tablet");
    let r = Root::new(&d);
    // core restarted in the same boot while unlocked: stays unlocked
    let l2 = Lock::new(&r, 0);
    assert!(!l2.locked());
    // a reboot clears /run: the first start is locked
    std::fs::remove_dir_all(d.join("run")).unwrap();
    let mut l3 = Lock::new(&r, 0);
    l3.set_iterations(8);
    assert!(l3.locked());
    assert!(d.join("run/walpad/locked").exists());
    // a restart while locked stays locked; unlocking removes the marker
    let mut l4 = Lock::new(&r, 0);
    l4.set_iterations(8);
    assert!(l4.locked());
    assert_eq!(l4.unlock("4321", 1), Unlock::Unlocked);
    assert!(!d.join("run/walpad/locked").exists());
    assert!(!Lock::new(&r, 0).locked());
    // no lock set: never locked at boot
    std::fs::remove_dir_all(d.join("run")).unwrap();
    std::fs::remove_file(d.join("var/lib/walpad/lock.json")).unwrap();
    assert!(!Lock::new(&r, 0).locked());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn auto_lock_counts_from_screen_off_and_a_short_wake_does_not_lock() {
    let (d, mut l) = fresh("auto");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    l.set_auto_lock(30).unwrap();
    assert_eq!(l.status(0).auto_lock_s, 30);
    l.screen(true, 1000);
    assert!(!l.tick(20_000) && !l.locked());
    l.screen(false, 25_000);
    assert!(!l.tick(100_000) && !l.locked(), "woke before the delay");
    l.screen(true, 100_000);
    assert!(!l.tick(129_999));
    assert!(l.tick(130_000) && l.locked());
    // delay 0 = at once
    l.unlock("4321", 131_000);
    l.set_auto_lock(0).unwrap();
    l.screen(false, 131_000);
    l.screen(true, 132_000);
    assert!(l.tick(132_000) && l.locked());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn rescue_by_removing_the_file_unlocks_within_a_second() {
    let (d, mut l) = fresh("rescue");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    l.lock_now();
    std::fs::remove_file(d.join("var/lib/walpad/lock.json")).unwrap();
    assert!(!l.tick(500), "not looked at yet");
    assert!(l.tick(1500));
    assert!(!l.configured() && !l.locked());
    assert!(!d.join("run/walpad/locked").exists());
    // lock_now without a lock does nothing
    assert!(!l.lock_now());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_damaged_file_counts_as_no_lock_never_as_locked_forever() {
    let (d, mut l) = fresh("bad");
    l.set(LockKind::Pin, "4321", None, 0).unwrap();
    std::fs::write(d.join("var/lib/walpad/lock.json"), "{\"v\":1,").unwrap();
    let r = Root::new(&d);
    assert!(!Lock::new(&r, 0).configured());
    let _ = std::fs::remove_dir_all(d);
}
