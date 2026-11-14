use super::*;

fn put(root: &Root, path: &str, body: &str) {
    let p = root.p(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// The touch controller registers a few seconds after the keys; it must still be picked up
/// (the first release only retried while nothing was open, so touch never worked).
#[test]
fn late_touch_device_is_picked_up() {
    let dir = std::env::temp_dir().join(format!("walpad-inputs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = Root::new(&dir);
    put(&root, "/sys/class/input/input1/name", "mtk-kpd\n");
    put(&root, "/sys/class/input/input1/event1/dev", "13:65\n");
    put(&root, "/dev/input/event1", "");
    let mut i = Inputs::new(&root, "test");
    assert_eq!(i.device_count(), 1);
    put(&root, "/sys/class/input/input7/name", "mtk-tpd\n");
    put(&root, "/sys/class/input/input7/event4/dev", "13:68\n");
    put(&root, "/dev/input/event4", "");
    i.next_discover = Instant::now();
    i.poll(&root);
    assert_eq!(i.device_count(), 2);
    assert!(i.readers.iter().any(|(r, _)| *r == Role::Touch));
    i.next_discover = Instant::now();
    i.poll(&root);
    assert_eq!(i.device_count(), 2, "no duplicate readers");
    let _ = std::fs::remove_dir_all(&dir);
}
