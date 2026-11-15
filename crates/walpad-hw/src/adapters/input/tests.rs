use super::*;
use crate::testutil::{put, tmp};

#[test]
fn discovers_by_name_not_number() {
    let r = Root::new(tmp());
    put(&r, "/sys/class/input/input7/name", "mtk-tpd\n");
    put(&r, "/sys/class/input/input7/event4/dev", "13:68\n");
    put(&r, "/sys/class/input/input2/name", "mtk-kpd\n");
    put(&r, "/sys/class/input/input2/event1/dev", "13:65\n");
    put(&r, "/sys/class/input/input3/name", "no-event-child\n");
    let d = discover(&r);
    assert_eq!(d.len(), 2);
    let t = find(&d, is_touch).unwrap();
    assert!(t.node.ends_with("dev/input/event4"));
    assert_eq!(t.dev, Some((13, 68)));
    assert!(find(&d, |n| n == "HALL_DEV").is_none());
}

#[test]
fn key_held_by_name_and_errors() {
    let r = Root::new(tmp());
    assert!(key_held(&r, 114).unwrap_err().contains("not found"));
    assert!(key_held(&r, 5).unwrap_err().contains("no known device"));
    put(&r, "/sys/class/input/input2/name", "mtk-kpd\n");
    put(&r, "/sys/class/input/input2/event1/dev", "13:65\n");
    let mut b = vec![0u8; 96];
    std::fs::create_dir_all(r.p("/dev/input")).unwrap();
    std::fs::write(r.p("/dev/input/event1"), &b).unwrap();
    assert_eq!(key_held(&r, 114), Ok(false));
    b[14] = 4;
    std::fs::write(r.p("/dev/input/event1"), &b).unwrap();
    assert_eq!(key_held(&r, 114), Ok(true));
    assert_eq!(key_held(&r, 115), Ok(false));
    assert_eq!(parse_key("VolumeDown"), Some(114));
    assert_eq!(parse_key("116"), Some(116));
    assert_eq!(parse_key("banana"), None);
}

#[test]
fn touch_names() {
    assert!(is_touch("mtk-tpd") && is_touch("gsl3680") && is_touch("Foo Touchscreen"));
    assert!(!is_touch("mtk-kpd"));
}

#[test]
fn existing_node_is_left_alone() {
    let r = Root::new(tmp());
    put(&r, "/dev/input/event4", "");
    let d = InputDevice {
        name: "x".into(),
        node: r.p("/dev/input/event4"),
        dev: Some((13, 68)),
    };
    assert!(!ensure_node(&d).unwrap());
    let missing = InputDevice {
        name: "x".into(),
        node: r.p("/dev/input/event9"),
        dev: None,
    };
    assert!(ensure_node(&missing).is_err());
}
