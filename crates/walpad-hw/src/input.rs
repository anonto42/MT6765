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
