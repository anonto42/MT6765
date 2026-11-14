//! Parsing from [`RawRequest`] and semantic validation of [`Command`].
use super::names::{COMMAND_NAMES, DEFAULT_ROLES, NO_ARG_COMMANDS};
use super::settings::{valid_datetime, validate_setting};
use super::validators::*;
use super::{Command, RawRequest};
use crate::ErrorCode;
use serde_json::Value;

impl Command {
    /// Build from an envelope, distinguishing unknown commands from bad arguments.
    pub fn from_raw(raw: &RawRequest) -> Result<Command, (ErrorCode, String)> {
        if !COMMAND_NAMES.contains(&raw.cmd.as_str()) {
            return Err((
                ErrorCode::UnknownCommand,
                format!("unknown command {:?}", raw.cmd),
            ));
        }
        let mut obj = serde_json::Map::new();
        obj.insert("cmd".into(), Value::String(raw.cmd.clone()));
        // commands that take no arguments ignore a stray `args` (unknown fields are ignored)
        if NO_ARG_COMMANDS.contains(&raw.cmd.as_str()) {
            // nothing to insert
        } else if !raw.args.is_null() {
            obj.insert("args".into(), raw.args.clone());
        } else if matches!(raw.cmd.as_str(), "set_brightness" | "layout" | "jack_force") {
            obj.insert("args".into(), Value::Object(Default::default()));
        }
        let cmd: Command = serde_json::from_value(Value::Object(obj))
            .map_err(|e| (ErrorCode::InvalidArgs, format!("bad args: {e}")))?;
        cmd.validate().map_err(|m| (ErrorCode::InvalidArgs, m))?;
        Ok(cmd)
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Command::Hello { client, .. } if client.is_empty() || client.len() > 64 => {
                Err("client must be 1-64 chars".into())
            }
            Command::SetBrightness(a) => match (a.value, a.delta) {
                (Some(v), None) if (0..=255).contains(&v) => Ok(()),
                (None, Some(d)) if (-255..=255).contains(&d) => Ok(()),
                (Some(_), None) => Err("value must be 0-255".into()),
                (None, Some(_)) => Err("delta must be -255..255".into()),
                _ => Err("give exactly one of value or delta".into()),
            },
            Command::WifiConnect {
                ssid,
                secret,
                saved_id,
            } => {
                match (ssid, saved_id) {
                    (Some(h), None) => {
                        if h.is_empty()
                            || h.len() > 64
                            || h.len() % 2 != 0
                            || !h.bytes().all(|b| b.is_ascii_hexdigit())
                        {
                            return Err("ssid must be 1-32 bytes as hex".into());
                        }
                    }
                    (None, Some(_)) => {}
                    _ => return Err("give exactly one of ssid or saved_id".into()),
                }
                if let Some(s) = secret {
                    let n = s.expose().len();
                    if !(8..=63).contains(&n) && n != 64 {
                        return Err("secret length is not valid".into());
                    }
                }
                Ok(())
            }
            Command::HotspotConfig { ssid, secret } => {
                if ssid.is_none() && secret.is_none() {
                    return Err("give ssid and/or secret".into());
                }
                if ssid.as_deref().is_some_and(|n| !valid_hotspot_ssid(n)) {
                    return Err("ssid must be 1-32 bytes without control characters".into());
                }
                if secret
                    .as_ref()
                    .is_some_and(|s| !valid_hotspot_secret(s.expose()))
                {
                    return Err("secret must be 8-63 printable ASCII characters".into());
                }
                Ok(())
            }
            Command::BluetoothPair { address }
            | Command::BluetoothConnect { address }
            | Command::BluetoothDisconnect { address }
            | Command::BluetoothForget { address }
            | Command::BluetoothConfirm { address, .. }
                if !valid_bt_address(address) =>
            {
                Err("address must be like AA:BB:CC:DD:EE:FF".into())
            }
            Command::VolumeSet { percent } if !(0..=100).contains(percent) => {
                Err("percent must be 0-100".into())
            }
            Command::VolumeDelta { delta } if !(-100..=100).contains(delta) => {
                Err("delta must be -100..100".into())
            }
            Command::VpnConfig {
                profile,
                kill_switch,
                dns,
                proxy,
            } => {
                if profile.is_none() && kill_switch.is_none() && dns.is_none() && proxy.is_none() {
                    return Err("give profile, kill_switch, dns and/or proxy".into());
                }
                if profile.as_deref().is_some_and(|n| !valid_vpn_name(n)) {
                    return Err("profile must be 1-24 characters of A-Z a-z 0-9 . - _".into());
                }
                if let Some(d) = dns {
                    if d.len() > 3 || d.iter().any(|s| !valid_dns_server(s)) {
                        return Err("dns must be up to 3 IPv4 or IPv6 addresses".into());
                    }
                }
                if proxy
                    .as_deref()
                    .is_some_and(|p| !p.is_empty() && !valid_proxy(p))
                {
                    return Err("proxy must look like http://host:port".into());
                }
                Ok(())
            }
            Command::VpnImport { path, name } => {
                if !valid_vpn_import_path(path) {
                    return Err("path must be a .conf or .ovpn file in a user folder".into());
                }
                if name.as_deref().is_some_and(|n| !valid_vpn_name(n)) {
                    return Err("name must be 1-24 characters of A-Z a-z 0-9 . - _".into());
                }
                Ok(())
            }
            Command::VpnRemove { name } if !valid_vpn_name(name) => {
                Err("name must be 1-24 characters of A-Z a-z 0-9 . - _".into())
            }
            Command::MobileApn { apn } if !valid_apn(apn) => {
                Err("apn must be 1-62 characters of A-Z a-z 0-9 . - _".into())
            }
            Command::SimPin { pin } if !valid_sim_pin(pin.expose()) => {
                Err("pin must be 4-8 digits".into())
            }
            Command::Launch { app, args } => {
                if !valid_app_id(app) {
                    Err("app must be an id like \"browser\"".into())
                } else if let Some(a) = args {
                    if valid_open_path(&a.path) {
                        Ok(())
                    } else {
                        Err("path must be an absolute file path in /root, /mnt, /home or /tmp/Downloads".into())
                    }
                } else {
                    Ok(())
                }
            }
            Command::CloseApp { app } => {
                if valid_app_id(app) {
                    Ok(())
                } else {
                    Err("app must be an id like \"browser\"".into())
                }
            }
            Command::NewInstance { app } if !valid_app_id(app) => {
                Err("app must be an id like \"terminal\"".into())
            }
            Command::FocusWindow { window }
            | Command::CloseWindow { window }
            | Command::WindowRect { window, .. }
            | Command::Snap { window, .. }
                if *window == 0 =>
            {
                Err("window must be a window id from `windows`".into())
            }
            Command::WindowRect { w, h, x, y, .. }
                if !(1..=8192).contains(w)
                    || !(1..=8192).contains(h)
                    || !(-8192..=8192).contains(x)
                    || !(-8192..=8192).contains(y) =>
            {
                Err("x, y, w, h must be within 8192".into())
            }
            Command::Layout { ratio: Some(r), .. } if !crate::wm::valid_ratio(*r) => {
                Err("ratio must be 33, 50 or 67".into())
            }
            Command::PkgInstall { id } | Command::PkgRemove { id } => {
                if valid_app_id(id) && !id.starts_with(['-', '_']) {
                    Ok(())
                } else {
                    Err("id must be a catalogue id like \"mousepad\"".into())
                }
            }
            Command::DefaultApp { role, app } => {
                if !DEFAULT_ROLES.contains(&role.as_str()) {
                    Err("role is not one of the default app roles".into())
                } else if !valid_app_id(app) || app.starts_with(['-', '_']) {
                    Err("app must be a catalogue id like \"evince\"".into())
                } else {
                    Ok(())
                }
            }
            Command::PhoneDial { number } if !valid_dial_number(number.expose()) => {
                Err("number must be 1-32 characters of 0-9 + * # ,".into())
            }
            Command::PhoneUssd { code } if !valid_dial_number(code.expose()) => {
                Err("code must be 1-32 characters of 0-9 + * # ,".into())
            }
            Command::PhoneDtmf { digit } if !valid_dtmf(digit) => {
                Err("digit must be one of 0-9 * #".into())
            }
            Command::SmsSend { number, text } => {
                if !valid_sms_number(number.expose()) {
                    Err("number must be 3-20 digits, optionally starting with +".into())
                } else if !valid_sms_text(text.expose()) {
                    Err("text must be 1-1000 characters without control characters".into())
                } else {
                    Ok(())
                }
            }
            Command::SmsRead { number } if !valid_sms_number(number.expose()) => {
                Err("number must be 3-20 digits, optionally starting with +".into())
            }
            Command::ContactSave { id, name, numbers } => {
                if id.as_deref().is_some_and(|i| !valid_store_id(i)) {
                    Err("id is not valid".into())
                } else if !valid_contact_name(name) {
                    Err("name must be 1-64 characters without control characters".into())
                } else if numbers.is_empty()
                    || numbers.len() > 5
                    || numbers.iter().any(|n| !valid_sms_number(n.expose()))
                {
                    Err("give 1-5 numbers of 3-20 digits, optionally starting with +".into())
                } else {
                    Ok(())
                }
            }
            Command::ContactDelete { id } if !valid_store_id(id) => Err("id is not valid".into()),
            Command::ContactImport { path } if !valid_vcard_path(path) => {
                Err("path must be a .vcf file in a user folder".into())
            }
            Command::SetSetting { key, value } => validate_setting(key, value),
            Command::SetTime { datetime } if !valid_datetime(datetime) => {
                Err("datetime must be YYYY-MM-DD HH:MM or YYYY-MM-DD HH:MM:SS".into())
            }
            Command::OpenMap { lat, lon }
                if lat.is_some() != lon.is_some()
                    || lat.is_some_and(|v| !v.is_finite() || !(-90.0..=90.0).contains(&v))
                    || lon.is_some_and(|v| !v.is_finite() || !(-180.0..=180.0).contains(&v)) =>
            {
                Err("lat must be -90..90 and lon -180..180".into())
            }
            Command::StatsConfig {
                cpu: None,
                ram: None,
                rom: None,
                net: None,
            } => Err("give at least one of cpu, ram, rom, net".into()),
            Command::LockSet {
                kind,
                secret,
                current,
            } => {
                if !valid_lock_secret(*kind, secret.expose()) {
                    return Err(lock_secret_rule(*kind).into());
                }
                if current.as_ref().is_some_and(|c| !valid_attempt(c.expose())) {
                    return Err("current secret is not valid".into());
                }
                Ok(())
            }
            Command::LockRemove { current: s } | Command::Unlock { secret: s }
                if !valid_attempt(s.expose()) =>
            {
                Err("secret must be 1-128 characters".into())
            }
            Command::LockConfig { auto_lock_s } if *auto_lock_s > 3600 => {
                Err("auto_lock_s must be 0-3600".into())
            }
            _ => Ok(()),
        }
    }
}
