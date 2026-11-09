//! Configuration the screens show or change: /etc/walpad/devd.json and keys.json (read through the
//! Root prefix; the core owns the writes).
use serde_json::Value;
use walpad_hw::Root;

#[derive(Debug, Clone, PartialEq)]
pub struct DevdSettings {
    pub auto_rotate: bool,
    pub rotate_flip: bool,
    pub dim_after_s: u32,
    pub off_after_s: u32,
    pub lid_off: bool,
    pub low_batt_mv: u32,
    pub critical_batt_mv: u32,
}

impl Default for DevdSettings {
    /// The contract defaults (docs/rust/CONTRACTS.md).
    fn default() -> Self {
        DevdSettings {
            auto_rotate: true,
            rotate_flip: false,
            dim_after_s: 90,
            off_after_s: 240,
            lid_off: true,
            low_batt_mv: 3550,
            critical_batt_mv: 3350,
        }
    }
}

pub fn read_devd(root: &Root) -> DevdSettings {
    let v: Value = std::fs::read_to_string(root.p("/etc/walpad/devd.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let d = DevdSettings::default();
    let u = |k: &str, def: u32| v[k].as_u64().map_or(def, |x| x as u32);
    let b = |k: &str, def: bool| v[k].as_bool().unwrap_or(def);
    DevdSettings {
        auto_rotate: b("auto_rotate", d.auto_rotate),
        rotate_flip: b("rotate_flip", d.rotate_flip),
        dim_after_s: u("dim_after_s", d.dim_after_s),
        off_after_s: u("off_after_s", d.off_after_s),
        lid_off: b("lid_off", d.lid_off),
        low_batt_mv: u("low_batt_mv", d.low_batt_mv),
        critical_batt_mv: u("critical_batt_mv", d.critical_batt_mv),
    }
}

/// "key:Up" -> "Key Up", "ctl:brightness +40" -> "Brightness +40", ...
pub fn pretty_action(a: &str) -> String {
    let a = a.trim();
    match a.split_once(':') {
        _ if a.is_empty() || a == "none" => "None".into(),
        Some(("key", k)) => format!("Key {k}"),
        Some(("ctl", c)) => {
            let mut ch = c.chars();
            ch.next()
                .map(|f| f.to_uppercase().collect::<String>() + ch.as_str())
                .unwrap_or_default()
        }
        Some(("launch", id)) => format!("Open {id}"),
        Some(("run", c)) => format!("Run {c} (old)"),
        Some(("volume", v)) => format!("Volume {v}"),
        _ => match a {
            "home" => "Home".into(),
            "keyboard" => "Toggle keyboard".into(),
            "powermenu" => "Power menu".into(),
            "screenshot" => "Screenshot".into(),
            other => other.to_string(),
        },
    }
}

/// Volume-button gestures from keys.json as (label, action) rows (read-only in v1).
pub fn read_gestures(root: &Root) -> Vec<(String, String)> {
    let v: Value = std::fs::read_to_string(root.p("/etc/walpad/keys.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let mut out = Vec::new();
    for (key, name) in [("vol_up", "Volume up"), ("vol_down", "Volume down")] {
        for (g, gl) in [
            ("single", "tap"),
            ("double", "double tap"),
            ("triple", "triple tap"),
            ("quad", "4 taps"),
            ("long", "hold"),
        ] {
            if let Some(a) = v[key][g].as_str() {
                out.push((format!("{name} · {gl}"), pretty_action(a)));
            }
        }
    }
    if let Some(a) = v["both"]["press"].as_str() {
        out.push(("Both buttons".into(), pretty_action(a)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with(files: &[(&str, &str)]) -> Root {
        let d =
            std::env::temp_dir().join(format!("walpad-cfg-{}-{}", std::process::id(), files.len()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
        for (n, c) in files {
            std::fs::write(d.join("etc/walpad").join(n), c).unwrap();
        }
        Root::new(d)
    }

    #[test]
    fn devd_defaults_and_overrides() {
        assert_eq!(read_devd(&root_with(&[])), DevdSettings::default());
        let r = root_with(&[(
            "devd.json",
            r#"{"dim_after_s":30,"off_after_s":0,"lid_off":false,"low_batt_mv":3600,"junk":1}"#,
        )]);
        let d = read_devd(&r);
        assert_eq!(
            (
                d.dim_after_s,
                d.off_after_s,
                d.lid_off,
                d.low_batt_mv,
                d.critical_batt_mv
            ),
            (30, 0, false, 3600, 3350)
        );
        assert_eq!(
            read_devd(&root_with(&[("devd.json", "not json")])),
            DevdSettings::default()
        );
    }

    #[test]
    fn actions_are_readable() {
        for (a, want) in [
            ("key:Up", "Key Up"),
            ("ctl:brightness +40", "Brightness +40"),
            ("launch:browser", "Open browser"),
            ("run:firefox-esr", "Run firefox-esr (old)"),
            ("home", "Home"),
            ("keyboard", "Toggle keyboard"),
            ("none", "None"),
            ("", "None"),
            ("volume:+5", "Volume +5"),
        ] {
            assert_eq!(pretty_action(a), want);
        }
    }

    #[test]
    fn gestures_from_keys_json() {
        let r = root_with(&[(
            "keys.json",
            r#"{"vol_up":{"single":"key:Up","long":"keyboard"},"vol_down":{"triple":"ctl:brightness -40"},"both":{"press":"ctl:rotate toggle"}}"#,
        )]);
        let g = read_gestures(&r);
        assert_eq!(g[0], ("Volume up · tap".to_string(), "Key Up".to_string()));
        assert!(g.contains(&(
            "Volume down · triple tap".to_string(),
            "Brightness -40".to_string()
        )));
        assert_eq!(
            g.last().unwrap(),
            &("Both buttons".to_string(), "Rotate toggle".to_string())
        );
        assert!(read_gestures(&root_with(&[])).is_empty());
    }
}
