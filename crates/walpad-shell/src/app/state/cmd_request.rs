//! IPC wire encoding and reply-purpose mapping for `Cmd`.

use serde_json::{json, Value};

use super::cmd::{Cmd, Purpose};

impl Cmd {
    /// (command, args) for the core's IPC.
    pub fn request(&self) -> (&'static str, Value) {
        match self {
            Cmd::SetBrightness(v) => ("set_brightness", json!({"value": v})),
            Cmd::SetVolume(v) => ("volume_set", json!({"percent": v})),
            Cmd::Mute(on) => ("mute", json!({"on": on})),
            Cmd::TestTone => ("volume_test", Value::Null),
            Cmd::JackForce(route) => ("jack_force", json!({"route": route})),
            Cmd::RotateToggle => ("rotate", json!({"mode": "toggle"})),
            Cmd::KeyboardToggle => ("keyboard", json!({"mode": "toggle"})),
            Cmd::KeyboardShow => ("keyboard", json!({"mode": "show"})),
            Cmd::KeyboardHide => ("keyboard", json!({"mode": "hide"})),
            // shell-local: `Shell::send` applies these and never puts them on the socket
            Cmd::MousePadShow => ("mouse_pad", json!({"mode": "show"})),
            Cmd::MousePadHide => ("mouse_pad", json!({"mode": "hide"})),
            Cmd::ClipboardShow => ("clipboard", json!({"mode": "show"})),
            Cmd::ScreenOff => ("screen", json!({"mode": "off"})),
            Cmd::Back => ("back", Value::Null),
            Cmd::Launch(id) => ("launch", json!({"app": id})),
            Cmd::LaunchFile { app, path } => {
                ("launch", json!({"app": app, "args": {"path": path}}))
            }
            Cmd::MediaScan | Cmd::MediaThumbs(_) | Cmd::MediaInfo(_) | Cmd::MediaRemember(_) => {
                ("media", Value::Null)
            }
            Cmd::CloseApp(id) => ("close_app", json!({"app": id})),
            Cmd::NewInstance(id) => ("new_instance", json!({"app": id})),
            Cmd::FocusWindow(w) => ("focus_window", json!({"window": w})),
            Cmd::CloseWindow(w) => ("close_window", json!({"window": w})),
            Cmd::WindowRect { window, x, y, w, h } => (
                "window_rect",
                json!({"window": window, "x": x, "y": y, "w": w, "h": h}),
            ),
            Cmd::Snap { window, zone } => ("snap", json!({"window": window, "zone": zone})),
            Cmd::Layout { mode, ratio, swap } => {
                let mut a = serde_json::Map::new();
                if let Some(m) = mode {
                    a.insert("mode".into(), json!(m));
                }
                if let Some(r) = ratio {
                    a.insert("ratio".into(), json!(r));
                }
                if *swap {
                    a.insert("swap".into(), json!(true));
                }
                ("layout", Value::Object(a))
            }
            Cmd::SetPinned(ids) => ("set_setting", json!({"key": "pinned_apps", "value": ids})),
            Cmd::WifiScan => ("wifi_scan", Value::Null),
            Cmd::WifiListSaved => ("wifi_list_saved", Value::Null),
            Cmd::WifiConnect {
                ssid_hex,
                secret,
                saved_id,
            } => {
                let mut a = serde_json::Map::new();
                if let Some(h) = ssid_hex {
                    a.insert("ssid".into(), json!(h));
                }
                if let Some(s) = secret {
                    // the one place the secret is serialized: onto the control socket, never a log
                    a.insert(
                        "secret".into(),
                        serde_json::to_value(s).unwrap_or(Value::Null),
                    );
                }
                if let Some(id) = saved_id {
                    a.insert("saved_id".into(), json!(id));
                }
                ("wifi_connect", Value::Object(a))
            }
            Cmd::WifiDisconnect => ("wifi_disconnect", Value::Null),
            Cmd::WifiForget(id) => ("wifi_forget", json!({"saved_id": id})),
            Cmd::WifiEnable(on) => ("wifi_enable", json!({"on": on})),
            Cmd::Hotspot(on) => ("hotspot", json!({"on": on})),
            Cmd::HotspotConfig { ssid, secret } => {
                let mut a = serde_json::Map::new();
                if let Some(n) = ssid {
                    a.insert("ssid".into(), json!(n));
                }
                if let Some(s) = secret {
                    // serialized onto the control socket only, never a log
                    a.insert(
                        "secret".into(),
                        serde_json::to_value(s).unwrap_or(Value::Null),
                    );
                }
                ("hotspot_config", Value::Object(a))
            }
            Cmd::Vpn(on) => ("vpn", json!({"on": on})),
            Cmd::VpnKeep => ("vpn_confirm", Value::Null),
            Cmd::VpnConfig {
                profile,
                kill_switch,
                dns,
                proxy,
            } => {
                let mut a = serde_json::Map::new();
                if let Some(p) = profile {
                    a.insert("profile".into(), json!(p));
                }
                if let Some(k) = kill_switch {
                    a.insert("kill_switch".into(), json!(k));
                }
                if let Some(d) = dns {
                    a.insert("dns".into(), json!(d));
                }
                if let Some(p) = proxy {
                    a.insert("proxy".into(), json!(p));
                }
                ("vpn_config", Value::Object(a))
            }
            Cmd::VpnImport(path) => ("vpn_import", json!({"path": path})),
            Cmd::VpnFiles => ("vpn_files", Value::Null),
            Cmd::Gps(on) => ("gps", json!({"on": on})),
            // shell-local: `Shell::send` copies and never puts this on the socket
            Cmd::CopyText(_) => ("clipboard", json!({"mode": "copy"})),
            Cmd::MobileRadio(on) => ("mobile_radio", json!({"on": on})),
            Cmd::MobileData(on) => ("mobile_data", json!({"on": on})),
            Cmd::MobileApn(a) => ("mobile_apn", json!({"apn": a})),
            // serialized onto the control socket only, never a log
            Cmd::SimPin(pin) => (
                "sim_pin",
                json!({"pin": serde_json::to_value(pin).unwrap_or(Value::Null)}),
            ),
            Cmd::MobileInfo => ("mobile_info", Value::Null),
            Cmd::BtEnable(on) => ("bluetooth_enable", json!({"on": on})),
            Cmd::BtScan(on) => ("bluetooth_scan", json!({"on": on})),
            Cmd::BtPair(a) => ("bluetooth_pair", json!({"address": a})),
            Cmd::BtConnect(a) => ("bluetooth_connect", json!({"address": a})),
            Cmd::BtDisconnect(a) => ("bluetooth_disconnect", json!({"address": a})),
            Cmd::BtForget(a) => ("bluetooth_forget", json!({"address": a})),
            Cmd::BtConfirm { address, accept } => (
                "bluetooth_confirm",
                json!({"address": address, "accept": accept}),
            ),
            // serialized onto the control socket only, never a log
            Cmd::LockSet {
                kind,
                secret,
                current,
            } => {
                let mut a = json!({
                    "kind": kind,
                    "secret": serde_json::to_value(secret).unwrap_or(Value::Null),
                });
                if let Some(c) = current {
                    a["current"] = serde_json::to_value(c).unwrap_or(Value::Null);
                }
                ("lock_set", a)
            }
            Cmd::LockRemove(c) => (
                "lock_remove",
                json!({"current": serde_json::to_value(c).unwrap_or(Value::Null)}),
            ),
            Cmd::LockNow => ("lock_now", Value::Null),
            Cmd::LockConfig(s) => ("lock_config", json!({"auto_lock_s": s})),
            Cmd::PkgCatalogue => ("pkg_catalogue", Value::Null),
            Cmd::PkgInstall(id) => ("pkg_install", json!({"id": id})),
            Cmd::PkgRemove(id) => ("pkg_remove", json!({"id": id})),
            Cmd::DefaultApp { role, app } => ("default_app", json!({"role": role, "app": app})),
            // serialized onto the control socket only, never a log
            Cmd::PhoneDial(n) => (
                "phone_dial",
                json!({"number": serde_json::to_value(n).unwrap_or(Value::Null)}),
            ),
            Cmd::PhoneUssdCancel => ("phone_ussd_cancel", Value::Null),
            Cmd::PhoneSimContacts => ("phone_sim_contacts", Value::Null),
            Cmd::SmsSend { number, text } => (
                "sms_send",
                json!({"number": serde_json::to_value(number).unwrap_or(Value::Null),
                    "text": serde_json::to_value(text).unwrap_or(Value::Null)}),
            ),
            Cmd::SmsList => ("sms_list", Value::Null),
            Cmd::SmsDelete(id) => ("sms_delete", json!({"id": id})),
            Cmd::SmsRead(n) => (
                "sms_read",
                json!({"number": serde_json::to_value(n).unwrap_or(Value::Null)}),
            ),
            Cmd::ContactList => ("contact_list", Value::Null),
            Cmd::ContactSave { id, name, number } => {
                let mut a = json!({"name": name,
                    "numbers": [serde_json::to_value(number).unwrap_or(Value::Null)]});
                if let Some(i) = id {
                    a["id"] = json!(i);
                }
                ("contact_save", a)
            }
            Cmd::ContactDelete(id) => ("contact_delete", json!({"id": id})),
            Cmd::CallLog => ("call_log", Value::Null),
            Cmd::CallLogClear => ("call_log_clear", Value::Null),
            Cmd::Notifications => ("notifications", Value::Null),
            Cmd::NotificationMarkRead(id) => ("notification_mark_read", json!({"id": id})),
            Cmd::NotificationsMarkAllRead => ("notifications_mark_all_read", Value::Null),
            Cmd::NotificationsClearAll => ("notifications_clear_all", Value::Null),
            Cmd::Rotate(m) => ("rotate", json!({"mode": m})),
            Cmd::Stats(i, on) => ("stats_config", json!({ i.key(): on })),
            Cmd::Setting(k, v) => ("set_setting", json!({"key": k, "value": v})),
            Cmd::SetTime(dt) => ("set_time", json!({"datetime": dt})),
            Cmd::OpenMap(Some((lat, lon))) => ("open_map", json!({"lat": lat, "lon": lon})),
            Cmd::OpenMap(None) => ("open_map", json!({})),
            Cmd::RestartShell => ("restart_shell", Value::Null),
            Cmd::PowerMenuState(open) => ("power_menu", json!({"open": open})),
            Cmd::Power { poweroff } => (
                "power",
                json!({"action": if *poweroff { "poweroff" } else { "reboot" }, "confirmed": true}),
            ),
        }
    }

    pub fn purpose(&self) -> Purpose {
        match self {
            Cmd::WifiListSaved => Purpose::SavedList,
            Cmd::WifiScan => Purpose::Scan,
            Cmd::WifiConnect { .. } => Purpose::Connect,
            Cmd::WifiForget(_) => Purpose::Forget,
            Cmd::HotspotConfig { .. } => Purpose::HotspotConfig,
            Cmd::MobileApn(_) => Purpose::MobileApn,
            Cmd::SimPin(_) => Purpose::SimPin,
            Cmd::MobileInfo => Purpose::MobileInfo,
            Cmd::Vpn(_) | Cmd::VpnKeep => Purpose::Vpn,
            Cmd::Gps(_) => Purpose::Gps,
            Cmd::VpnConfig { dns, proxy, .. } if dns.is_some() || proxy.is_some() => {
                Purpose::VpnSheet
            }
            Cmd::VpnConfig { .. } => Purpose::Vpn,
            Cmd::VpnImport(_) => Purpose::VpnImport,
            Cmd::VpnFiles => Purpose::VpnFiles,
            Cmd::Notifications => Purpose::Notifications,
            Cmd::BtPair(_) => Purpose::BtPair,
            Cmd::BtConnect(_) | Cmd::BtDisconnect(_) | Cmd::BtForget(_) => Purpose::BtOp,
            Cmd::LockSet { .. } | Cmd::LockRemove(_) | Cmd::LockNow | Cmd::LockConfig(_) => {
                Purpose::Lock
            }
            Cmd::PkgCatalogue => Purpose::PkgCatalogue,
            Cmd::PkgInstall(_) | Cmd::PkgRemove(_) => Purpose::PkgAction,
            Cmd::DefaultApp { .. } => Purpose::DefaultApp,
            Cmd::SmsList => Purpose::PhoneSms,
            Cmd::ContactList => Purpose::PhoneContacts,
            Cmd::CallLog => Purpose::PhoneCalls,
            Cmd::PhoneDial(_)
            | Cmd::SmsSend { .. }
            | Cmd::SmsDelete(_)
            | Cmd::SmsRead(_)
            | Cmd::ContactSave { .. }
            | Cmd::ContactDelete(_)
            | Cmd::CallLogClear => Purpose::PhoneAction,
            Cmd::CloseApp(_) => Purpose::Close,
            Cmd::Setting(..) | Cmd::SetTime(_) | Cmd::SetPinned(_) | Cmd::Stats(..) => {
                Purpose::Setting
            }
            _ => Purpose::Other,
        }
    }
}
