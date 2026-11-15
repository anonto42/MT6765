//! Validation for `set_setting` keys/values and `set_time` datetime strings.
use super::names::{ALLOWED_SETTINGS, KEY_ACTIONS};
use super::validators::{valid_app_id, valid_bt_address, valid_dns_server, valid_hostname};
use serde_json::Value;

pub(super) fn validate_setting(key: &str, v: &Value) -> Result<(), String> {
    if !ALLOWED_SETTINGS.contains(&key) {
        return Err(format!("setting {key:?} is not allowed"));
    }
    match key {
        "pinned_apps" => match v.as_array() {
            Some(a) if a.len() <= 8 && a.iter().all(|x| x.as_str().is_some_and(valid_app_id)) => {
                Ok(())
            }
            _ => Err("value must be a list of at most 8 app ids".into()),
        },
        "auto_rotate" | "rotate_flip" | "lid_off" | "clock_24h" | "touch_gestures"
        | "multitouch" => v
            .is_boolean()
            .then_some(())
            .ok_or_else(|| "value must be a boolean".to_string()),
        "dim_level" => match v.as_u64() {
            Some(n) if (1..=255).contains(&n) => Ok(()),
            _ => Err("value must be an integer 1-255".into()),
        },
        "low_batt_mv" => match v.as_u64() {
            Some(n) if (3300..=4000).contains(&n) => Ok(()),
            _ => Err("value must be an integer 3300-4000".into()),
        },
        "critical_batt_mv" => match v.as_u64() {
            Some(n) if (3100..=3800).contains(&n) => Ok(()),
            _ => Err("value must be an integer 3100-3800".into()),
        },
        "net_ip_mode" => match v.as_str() {
            Some("dhcp" | "manual") => Ok(()),
            _ => Err("value must be \"dhcp\" or \"manual\"".into()),
        },
        "net_ip" | "net_gateway" => match v.as_str() {
            Some(s) if s.is_empty() || valid_dns_server(s) => Ok(()),
            _ => Err("value must be an IPv4/IPv6 address or empty".into()),
        },
        "net_mask" => match v.as_str() {
            Some("255.255.255.0" | "255.255.0.0" | "255.255.255.128" | "255.0.0.0") => Ok(()),
            _ => Err("value must be an allowed subnet mask".into()),
        },
        "net_mac_mode" => match v.as_str() {
            Some("hardware" | "random" | "custom") => Ok(()),
            _ => Err("value must be \"hardware\", \"random\" or \"custom\"".into()),
        },
        "net_mac" => match v.as_str() {
            Some(s) if s.is_empty() || valid_bt_address(s) => Ok(()),
            _ => Err("value must be AA:BB:CC:DD:EE:FF or empty".into()),
        },
        "net_hostname" => match v.as_str() {
            Some(s) if s.is_empty() || valid_hostname(s) => Ok(()),
            _ => Err("value must be a hostname of 1-63 letters, digits or -".into()),
        },
        "theme" => match v.as_str() {
            Some("dark" | "light") => Ok(()),
            _ => Err("value must be \"dark\" or \"light\"".into()),
        },
        "wallpaper" => match v.as_str() {
            Some("default" | "live_coffee" | "live_aurora" | "live_stars" | "custom") => Ok(()),
            _ => Err(
                "value must be \"default\", \"live_coffee\", \"live_aurora\", \"live_stars\", or \"custom\""
                    .into(),
            ),
        },
        "wallpaper_path" => match v.as_str() {
            Some("") => Ok(()),
            Some(s) if s.len() <= 512 && s.starts_with('/') && !s.contains("..") => Ok(()),
            _ => Err("value must be empty or an absolute path without '..'".into()),
        },
        "charge_mode" => match v.as_str() {
            Some("smart" | "bypass" | "eco" | "fast") => Ok(()),
            _ => Err("value must be \"smart\", \"bypass\", \"eco\" or \"fast\"".into()),
        },
        "charge_limit_pct" => match v.as_u64() {
            Some(n) if (50..=100).contains(&n) => Ok(()),
            _ => Err("value must be an integer 50-100".into()),
        },
        "charge_current_ma" => match v.as_u64() {
            Some(500 | 1000 | 1500 | 2000) => Ok(()),
            _ => Err("value must be 500, 1000, 1500 or 2000".into()),
        },
        k if k.starts_with("key.") => match v.as_str() {
            Some(s) if KEY_ACTIONS.contains(&s) => Ok(()),
            _ => Err("value must be an allowed key action".into()),
        },
        _ => match v.as_u64() {
            Some(n) if n <= 3600 => Ok(()),
            _ => Err("value must be an integer 0-3600".into()),
        },
    }
}

/// Validate a manual local date & time string (`YYYY-MM-DD HH:MM` or `YYYY-MM-DD HH:MM:SS`).
pub fn valid_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 16 && b.len() != 19 {
        return false;
    }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return false;
    }
    if b.len() == 19 && b[16] != b':' {
        return false;
    }
    let parse_u = |from: usize, to: usize| -> Option<u32> {
        let slice = s.get(from..to)?;
        if !slice.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        slice.parse().ok()
    };
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm)) = (
        parse_u(0, 4),
        parse_u(5, 7),
        parse_u(8, 10),
        parse_u(11, 13),
        parse_u(14, 16),
    ) else {
        return false;
    };
    let ss = if b.len() == 19 {
        match parse_u(17, 19) {
            Some(v) => v,
            None => return false,
        }
    } else {
        0
    };
    if !(2000..=2099).contains(&y) || !(1..=12).contains(&m) || hh > 23 || mm > 59 || ss > 59 {
        return false;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let max_d = match m {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=max_d).contains(&d)
}
