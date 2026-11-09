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
mod tests {
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
}
