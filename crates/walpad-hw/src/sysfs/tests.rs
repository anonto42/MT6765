use super::*;
use crate::testutil::{put, tmp};

#[test]
fn backlight_roundtrip_and_missing() {
    let r = Root::new(tmp());
    assert_eq!(read_backlight(&r), None);
    put(&r, BACKLIGHT, "120\n");
    assert_eq!(read_backlight(&r), Some(120));
    write_backlight(&r, 999).unwrap();
    assert_eq!(read_backlight(&r), Some(255));
    write_backlight(&r, 0).unwrap();
    assert_eq!(
        read_backlight(&r),
        Some(0),
        "shorter value must not leave trailing digits"
    );
}

#[test]
fn uptime() {
    let r = Root::new(tmp());
    put(&r, "/proc/uptime", "123.45 99.0\n");
    assert_eq!(uptime_s(&r), Some(123.45));
}
