use crate::Root;
use std::io::Write;

pub const BACKLIGHT: &str = "/sys/class/leds/lcd-backlight/brightness";

/// Read a sysfs/proc file, trimmed. None if missing or unreadable.
pub fn read_trim(root: &Root, path: &str) -> Option<String> {
    std::fs::read_to_string(root.p(path))
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn read_backlight(root: &Root) -> Option<u32> {
    read_trim(root, BACKLIGHT)?.parse().ok()
}

/// Write the backlight (clamped to 0-255).
pub fn write_backlight(root: &Root, value: u32) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(root.p(BACKLIGHT))?;
    f.write_all(value.min(255).to_string().as_bytes())
}

/// Uptime in seconds from /proc/uptime.
pub fn uptime_s(root: &Root) -> Option<f64> {
    read_trim(root, "/proc/uptime")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
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
}
