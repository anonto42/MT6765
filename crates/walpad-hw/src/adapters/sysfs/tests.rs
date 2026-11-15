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

#[test]
fn fb_blank_write() {
    let r = Root::new(tmp());
    // Missing file should return Ok (best-effort)
    assert!(write_fb_blank(&r, true).is_ok());

    put(&r, FB_BLANK, "0\n");
    write_fb_blank(&r, true).unwrap();
    assert_eq!(std::fs::read_to_string(r.p(FB_BLANK)).unwrap(), "4\n");
    write_fb_blank(&r, false).unwrap();
    assert_eq!(std::fs::read_to_string(r.p(FB_BLANK)).unwrap(), "0\n");
}

#[test]
fn cpufreq_governor_write() {
    let r = Root::new(tmp());
    assert_eq!(write_cpufreq_governor(&r, "powersave"), 0);

    put(&r, "/sys/devices/system/cpu/cpufreq/policy0/scaling_governor", "schedutil\n");
    put(&r, "/sys/devices/system/cpu/cpufreq/policy4/scaling_governor", "schedutil\n");
    let count = write_cpufreq_governor(&r, "powersave");
    assert_eq!(count, 2);
    assert_eq!(
        std::fs::read_to_string(r.p("/sys/devices/system/cpu/cpufreq/policy0/scaling_governor")).unwrap(),
        "powersave"
    );
}
