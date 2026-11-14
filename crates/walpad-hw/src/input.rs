use crate::Root;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    /// Name from /sys/class/input/inputN/name.
    pub name: String,
    /// /dev/input/eventN under the root.
    pub node: PathBuf,
    /// major:minor from the sysfs `dev` file.
    pub dev: Option<(u32, u32)>,
}

/// List input devices by reading /sys/class/input/input*/name (never rely on event numbers).
pub fn discover(root: &Root) -> Vec<InputDevice> {
    let base = root.p("/sys/class/input");
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(&base) else {
        return out;
    };
    for e in rd.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        if !fname.starts_with("input") {
            continue;
        }
        let dir = e.path();
        let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
            continue;
        };
        let Ok(subs) = std::fs::read_dir(&dir) else {
            continue;
        };
        for s in subs.flatten() {
            let sn = s.file_name().to_string_lossy().to_string();
            if sn.starts_with("event")
                && sn[5..].chars().all(|c| c.is_ascii_digit())
                && sn.len() > 5
            {
                let dev = std::fs::read_to_string(s.path().join("dev"))
                    .ok()
                    .and_then(|d| parse_majmin(&d));
                out.push(InputDevice {
                    name: name.trim().to_string(),
                    node: root.p(&format!("/dev/input/{sn}")),
                    dev,
                });
            }
        }
    }
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

pub fn parse_majmin(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.trim().split_once(':')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// Match names for each role (contract: mtk-tpd also accepts gsl3680 and anything containing "touchscreen").
pub fn is_touch(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "mtk-tpd" || n.contains("gsl3680") || n.contains("touchscreen")
}
pub fn find(devs: &[InputDevice], pred: impl Fn(&str) -> bool) -> Option<&InputDevice> {
    devs.iter().find(|d| pred(&d.name))
}

/// Which input device carries a key: the volume keys are on "mtk-kpd", the power key on
/// "mtk-pmic-keys" (contract names).  None for any other code.
pub fn key_device_name(code: u16) -> Option<&'static str> {
    match code {
        crate::evdev::KEY_VOLUMEDOWN | crate::evdev::KEY_VOLUMEUP => Some("mtk-kpd"),
        crate::evdev::KEY_POWER => Some("mtk-pmic-keys"),
        _ => None,
    }
}

/// Key by name (`volumedown`, `volumeup`, `power`) or by decimal code.
pub fn parse_key(s: &str) -> Option<u16> {
    match s.to_ascii_lowercase().as_str() {
        "volumedown" | "volume-down" | "voldown" => Some(crate::evdev::KEY_VOLUMEDOWN),
        "volumeup" | "volume-up" | "volup" => Some(crate::evdev::KEY_VOLUMEUP),
        "power" => Some(crate::evdev::KEY_POWER),
        n => n.parse().ok(),
    }
}

/// Is `code` held right now?  Discovers the device by name (never by event number).  Err means
/// "unknown key, no such device or unreadable": callers must treat it as not held.  The real
/// ioctl only runs on the tablet.
pub fn key_held(root: &Root, code: u16) -> Result<bool, String> {
    let name = key_device_name(code).ok_or_else(|| format!("no known device for key {code}"))?;
    let devs = discover(root);
    let dev = find(&devs, |n| n == name).ok_or_else(|| format!("input device {name} not found"))?;
    // early in boot there is no udev: create the node from major:minor like the core does
    ensure_node(dev).map_err(|e| format!("{}: {e}", dev.node.display()))?;
    let bits = crate::evdev::read_key_bitmap(&dev.node)
        .map_err(|e| format!("{}: {e}", dev.node.display()))?;
    Ok(crate::evdev::key_bit(&bits, code))
}

/// Create the /dev/input/eventN node from major:minor when missing (there is no udev).
/// Returns Ok(true) if it was created, Ok(false) if it already existed.
pub fn ensure_node(dev: &InputDevice) -> std::io::Result<bool> {
    if dev.node.exists() {
        return Ok(false);
    }
    let (maj, min) = dev
        .dev
        .ok_or_else(|| std::io::Error::other("no major:minor"))?;
    if let Some(parent) = dev.node.parent() {
        std::fs::create_dir_all(parent)?;
    }
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &dev.node,
        rustix::fs::FileType::CharacterDevice,
        rustix::fs::Mode::from_raw_mode(0o600),
        rustix::fs::makedev(maj, min),
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests;
