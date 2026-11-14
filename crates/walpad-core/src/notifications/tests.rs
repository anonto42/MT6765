use super::*;

fn root() -> Root {
    let d = std::env::temp_dir().join(format!(
        "walpad-notifications-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&d);
    Root::new(d)
}

fn charging(on: bool) -> Snapshot {
    Snapshot {
        charging: on,
        ..Snapshot::default()
    }
}

#[test]
fn first_tick_is_a_baseline_no_notification() {
    let mut n = Notifications::new(&root());
    let t0 = Instant::now();
    assert!(n.tick(t0, 1000, charging(true)).is_empty());
    assert_eq!(n.status().unread_count, 0);
}

#[test]
fn a_transition_fires_once_is_unread_and_bumps_the_badge() {
    let mut n = Notifications::new(&root());
    let t0 = Instant::now();
    n.tick(t0, 1000, charging(false));
    let fired = n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].title, "Charger connected");
    assert!(!fired[0].read);
    assert_eq!(n.status().unread_count, 1);
    assert_eq!(n.list()[0].title, "Charger connected");
    // not due yet: no new reading, no duplicate
    assert!(n
        .tick(t0 + Duration::from_secs(3), 3000, charging(true))
        .is_empty());
}

#[test]
fn mark_read_clear_and_clear_all() {
    let mut n = Notifications::new(&root());
    let t0 = Instant::now();
    n.tick(t0, 1000, charging(false));
    n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
    n.tick(t0 + Duration::from_secs(4), 4000, charging(false));
    assert_eq!(n.status().unread_count, 2);
    let ids: Vec<u64> = n.list().iter().map(|x| x.id).collect();
    assert!(!n.mark_read(999));
    assert!(n.mark_read(ids[0]));
    assert_eq!(n.status().unread_count, 1);
    assert_eq!(n.mark_all_read(), 1);
    assert_eq!(n.status().unread_count, 0);
    assert!(!n.clear(999));
    assert!(n.clear(ids[0]));
    assert_eq!(n.list().len(), 1);
    n.clear_all();
    assert!(n.list().is_empty());
}

#[test]
fn newest_first_and_ring_buffer_caps_at_100() {
    let mut n = Notifications::new(&root());
    let t0 = Instant::now();
    let mut on = false;
    n.tick(t0, 0, charging(on));
    for i in 1..205u64 {
        on = !on;
        n.tick(t0 + Duration::from_secs(2 * i), i * 1000, charging(on));
    }
    let list = n.list();
    assert_eq!(list.len(), 100);
    // newest first: the most recent id is at index 0
    assert!(list[0].id > list[1].id);
    assert!(list.windows(2).all(|w| w[0].id > w[1].id));
}

#[test]
fn persists_across_a_restart_and_tolerates_a_corrupt_file() {
    let r = root();
    {
        let mut n = Notifications::new(&r);
        let t0 = Instant::now();
        n.tick(t0, 1000, charging(false));
        n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
    }
    let again = Notifications::new(&r);
    assert_eq!(again.list().len(), 1);
    assert_eq!(again.status().unread_count, 1);

    // a corrupt file is tolerated: start empty rather than panic or fail to start
    std::fs::write(r.p(STORE_FILE), b"{ not json").unwrap();
    let after_corruption = Notifications::new(&r);
    assert!(after_corruption.list().is_empty());
    assert_eq!(after_corruption.status().unread_count, 0);
}
