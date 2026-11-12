//! Configuration the screens show or change: /etc/walpad/devd.json and keys.json (read through the
//! Root prefix; the core owns the writes).
use serde_json::Value;
use walpad_hw::Root;

#[derive(Debug, Clone, PartialEq)]
pub struct DevdSettings {
    pub auto_rotate: bool,
    pub rotate_flip: bool,
    pub dim_after_s: u32,
    pub dim_level: u32,
    pub off_after_s: u32,
    pub lid_off: bool,
    pub low_batt_mv: u32,
    pub critical_batt_mv: u32,
    pub touch_gestures: bool,
    pub multitouch: bool,
    pub net_ip_mode: String,
    pub net_ip: String,
    pub net_mask: String,
    pub net_gateway: String,
    pub net_mac_mode: String,
    pub net_mac: String,
    pub net_hostname: String,
    pub charge_mode: String,
    pub charge_limit_pct: u32,
    pub charge_current_ma: u32,
}

impl Default for DevdSettings {
    /// The contract defaults (docs/rust/CONTRACTS.md).
    fn default() -> Self {
        DevdSettings {
            auto_rotate: true,
            rotate_flip: false,
            dim_after_s: 90,
            dim_level: 20,
            off_after_s: 240,
            lid_off: true,
            low_batt_mv: 3550,
            critical_batt_mv: 3350,
            touch_gestures: true,
            multitouch: true,
            net_ip_mode: "dhcp".into(),
            net_ip: String::new(),
            net_mask: "255.255.255.0".into(),
            net_gateway: String::new(),
            net_mac_mode: "hardware".into(),
            net_mac: String::new(),
            net_hostname: String::new(),
            charge_mode: "smart".into(),
            charge_limit_pct: 85,
            charge_current_ma: 1500,
        }
    }
}

pub const NET_MASK_OPTIONS: [&str; 4] = [
    "255.255.255.0",
    "255.255.0.0",
    "255.255.255.128",
    "255.0.0.0",
];
pub const NET_MAC_MODES: [&str; 3] = ["hardware", "random", "custom"];
pub const CHARGE_MODES: [&str; 4] = ["smart", "bypass", "eco", "fast"];
pub const CHARGE_LIMIT_OPTIONS: [u32; 5] = [80, 85, 90, 95, 100];
pub const CHARGE_CURRENT_OPTIONS: [u32; 4] = [500, 1000, 1500, 2000];

pub fn read_devd(root: &Root) -> DevdSettings {
    let v: Value = std::fs::read_to_string(root.p("/etc/walpad/devd.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let tv: Value = std::fs::read_to_string(root.p("/etc/walpad/touch.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let d = DevdSettings::default();
    let u = |k: &str, def: u32| v[k].as_u64().map_or(def, |x| x as u32);
    let b = |k: &str, def: bool| v[k].as_bool().unwrap_or(def);
    let s = |k: &str, def: &str| v[k].as_str().unwrap_or(def).to_string();
    DevdSettings {
        auto_rotate: b("auto_rotate", d.auto_rotate),
        rotate_flip: b("rotate_flip", d.rotate_flip),
        dim_after_s: u("dim_after_s", d.dim_after_s),
        dim_level: u("dim_level", d.dim_level),
        off_after_s: u("off_after_s", d.off_after_s),
        lid_off: b("lid_off", d.lid_off),
        low_batt_mv: u("low_batt_mv", d.low_batt_mv),
        critical_batt_mv: u("critical_batt_mv", d.critical_batt_mv),
        touch_gestures: tv["gestures"]["enabled"]
            .as_bool()
            .unwrap_or(d.touch_gestures),
        multitouch: tv["multitouch"].as_bool().unwrap_or(d.multitouch),
        net_ip_mode: s("net_ip_mode", &d.net_ip_mode),
        net_ip: s("net_ip", &d.net_ip),
        net_mask: s("net_mask", &d.net_mask),
        net_gateway: s("net_gateway", &d.net_gateway),
        net_mac_mode: s("net_mac_mode", &d.net_mac_mode),
        net_mac: s("net_mac", &d.net_mac),
        net_hostname: s("net_hostname", &d.net_hostname),
        charge_mode: s("charge_mode", &d.charge_mode),
        charge_limit_pct: u("charge_limit_pct", d.charge_limit_pct),
        charge_current_ma: u("charge_current_ma", d.charge_current_ma),
    }
}

/// Hardware volume-button gesture slots (`(setting_key, label, default_action)`).
pub const KEY_SLOTS: [(&str, &str, &str); 11] = [
    ("key.vol_up.single", "Volume up · tap", "volume:+5"),
    ("key.vol_up.double", "Volume up · double tap", "key:Prior"),
    (
        "key.vol_up.triple",
        "Volume up · triple tap",
        "ctl:brightness +40",
    ),
    ("key.vol_up.quad", "Volume up · 4 taps", "none"),
    ("key.vol_up.long", "Volume up · hold", "keyboard"),
    ("key.vol_down.single", "Volume down · tap", "volume:-5"),
    (
        "key.vol_down.double",
        "Volume down · double tap",
        "key:Next",
    ),
    (
        "key.vol_down.triple",
        "Volume down · triple tap",
        "ctl:brightness -40",
    ),
    ("key.vol_down.quad", "Volume down · 4 taps", "none"),
    ("key.vol_down.long", "Volume down · hold", "home"),
    ("key.both.press", "Both buttons", "ctl:rotate toggle"),
];

/// Cycle to the next action in `walpad_proto::KEY_ACTIONS` given the current raw or pretty action.
pub fn next_key_action(cur: &str) -> &'static str {
    let acts = walpad_proto::KEY_ACTIONS;
    let idx = acts
        .iter()
        .position(|&a| a == cur || pretty_action(a) == cur)
        .unwrap_or(acts.len() - 1);
    acts[(idx + 1) % acts.len()]
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
            "back" => "Back".into(),
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
        // tests run in parallel: every call gets its own directory
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("walpad-cfg-{}-{n}", std::process::id()));
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
