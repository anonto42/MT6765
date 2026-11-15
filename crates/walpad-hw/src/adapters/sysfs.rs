use crate::Root;
use std::io::Write;

pub const BACKLIGHT: &str = "/sys/class/leds/lcd-backlight/brightness";
pub const FB_BLANK: &str = "/sys/class/graphics/fb0/blank";
/// cpufreq sysfs directory (all policy clusters share the same prefix on MT6765).
const CPUFREQ_BASE: &str = "/sys/devices/system/cpu/cpufreq";

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

/// Blank (4 = powerdown) or unblank (0) the framebuffer display controller.
/// Best-effort: ignores missing paths on test trees.
pub fn write_fb_blank(root: &Root, blank: bool) -> std::io::Result<()> {
    let path = root.p(FB_BLANK);
    if !path.exists() {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
    f.write_all(if blank { b"4\n" } else { b"0\n" })
}

/// Write `governor` (e.g. `"powersave"` / `"schedutil"`) to every cpufreq policy on the SoC.
/// Best-effort: missing or read-only paths are silently skipped (no kernel governor support, test
/// root, or read-only mount).  Returns the number of policies updated (0 is not an error).
pub fn write_cpufreq_governor(root: &Root, governor: &str) -> usize {
    let base = root.p(CPUFREQ_BASE);
    let Ok(rd) = std::fs::read_dir(&base) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in rd.flatten() {
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with("policy"))
        {
            continue;
        }
        let path = entry.path().join("scaling_governor");
        if std::fs::write(&path, governor).is_ok() {
            count += 1;
        }
    }
    count
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
mod tests;
