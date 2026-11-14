//! IPC command dispatcher for connectivity, phone, notifications, and packages.

use crate::pkg::Op;
use crate::wpa::parse::{hex, ssid_display, unhex};
use serde_json::{json, Value};
use std::time::Instant;
use walpad_proto::{Command, ErrorCode};

use super::errors::{
    bluetooth_err, gnss_err, hotspot_err, mobile_err, pkg_err, vpn_err, wifi_err, CmdResult,
};
use super::Daemon;

impl Daemon {
    pub(super) fn execute_net_phone(&mut self, cmd: Command) -> CmdResult {
        match cmd {
            Command::WifiScan => self
                .wifi
                .scan(Instant::now())
                .map(|_| json!({"accepted": true}))
                .map_err(wifi_err),
            Command::WifiListSaved => self
                .wifi
                .list_saved()
                .map(|v| {
                    Value::Array(
                        v.iter()
                            .map(|s| json!({"id": s.id, "ssid": ssid_display(&s.ssid), "ssid_hex": hex(&s.ssid)}))
                            .collect(),
                    )
                })
                .map_err(wifi_err),
            Command::WifiConnect {
                ssid,
                secret,
                saved_id,
            } => {
                let bytes = match ssid.as_deref() {
                    Some(h) => {
                        unhex(h).ok_or((ErrorCode::InvalidArgs, "ssid must be hex".to_string()))?
                    }
                    None => Vec::new(),
                };
                self.wifi
                    .connect(bytes, secret.as_ref(), saved_id, Instant::now())
                    .map(|_| json!({"accepted": true}))
                    .map_err(wifi_err)
            }
            Command::WifiDisconnect => self.wifi.disconnect().map(|_| json!({})).map_err(wifi_err),
            Command::WifiForget { saved_id } => {
                self.wifi.forget(saved_id).map(|_| json!({})).map_err(wifi_err)
            }
            Command::WifiEnable { on } => self.wifi_enable(on),
            Command::Hotspot { on } => {
                let st = self.hotspot.set_on(on, Instant::now()).map_err(hotspot_err)?;
                self.publish_hotspot();
                Ok(json!({"state": st}))
            }
            Command::HotspotConfig { ssid, secret } => {
                let restarting = self
                    .hotspot
                    .configure(ssid.as_deref(), secret.as_ref(), Instant::now())
                    .map_err(hotspot_err)?;
                let h = self.hotspot.status();
                let reply =
                    json!({"ssid": h.ssid, "has_secret": h.has_secret, "restarting": restarting});
                if restarting {
                    self.publish_hotspot();
                }
                Ok(reply)
            }
            Command::Gps { on } => {
                let st = self.gnss.set_on(on, Instant::now()).map_err(gnss_err)?;
                self.publish_gps();
                Ok(json!({"enabled": on, "state": st}))
            }
            Command::Vpn { on } => {
                let st = self.vpn.set_on(on, Instant::now()).map_err(vpn_err)?;
                self.publish_vpn();
                Ok(json!({"state": st}))
            }
            Command::VpnConfig {
                profile,
                kill_switch,
                dns,
                proxy,
            } => {
                self.vpn
                    .configure(
                        profile.as_deref(),
                        kill_switch,
                        dns.as_deref(),
                        proxy.as_deref(),
                        Instant::now(),
                    )
                    .map_err(vpn_err)?;
                let px = self.vpn.proxy().map(str::to_string);
                self.apps.set_proxy(px.as_deref());
                let v = self.vpn.status();
                Ok(
                    json!({"profile": v.profile, "kill_switch": v.kill_switch, "dns": v.dns, "proxy": v.proxy}),
                )
            }
            Command::VpnConfirm => {
                self.vpn.confirm(Instant::now()).map_err(vpn_err)?;
                Ok(json!({}))
            }
            Command::VpnImport { path, name } => {
                let p = self.vpn.import(&path, name.as_deref()).map_err(vpn_err)?;
                eprintln!("walpad-core: VPN profile imported");
                Ok(json!({"name": p.name, "kind": p.kind}))
            }
            Command::VpnRemove { name } => {
                self.vpn.remove(&name).map_err(vpn_err)?;
                Ok(json!({}))
            }
            Command::VpnFiles => {
                let files: Vec<Value> = self
                    .vpn
                    .candidates()
                    .into_iter()
                    .map(|(name, path, kind)| json!({"name": name, "path": path, "kind": kind}))
                    .collect();
                Ok(json!({"files": files}))
            }
            Command::MobileRadio { on } => self
                .modem
                .set_radio(on, Instant::now())
                .map_err(mobile_err),
            Command::MobileData { on } => {
                self.modem.set_data(on, Instant::now()).map_err(mobile_err)
            }
            Command::MobileApn { apn } => self.modem.set_apn(&apn).map_err(mobile_err),
            Command::SimPin { pin } => self.modem.sim_pin(&pin, Instant::now()).map_err(mobile_err),
            Command::MobileInfo => Ok(self.modem.info()),
            Command::BluetoothEnable { on } => self.bluetooth.set_enabled(on).map_err(bluetooth_err),
            Command::BluetoothScan { on } => self.bluetooth.set_scan(on).map_err(bluetooth_err),
            Command::BluetoothPair { address } => self.bluetooth.pair(&address).map_err(bluetooth_err),
            Command::BluetoothConnect { address } => {
                self.bluetooth.connect(&address).map_err(bluetooth_err)
            }
            Command::BluetoothDisconnect { address } => {
                self.bluetooth.disconnect(&address).map_err(bluetooth_err)
            }
            Command::BluetoothForget { address } => {
                self.bluetooth.forget(&address).map_err(bluetooth_err)
            }
            Command::BluetoothConfirm { address, accept } => {
                self.bluetooth.confirm(&address, accept).map_err(bluetooth_err)
            }
            // phone (owner request 2026-11-11): numbers and texts never reach a log
            Command::PhoneDial { number } => self
                .modem
                .phone_op(Instant::now(), |p, n| p.dial(number.expose(), n))
                .map_err(mobile_err),
            Command::PhoneUssd { code } => self
                .modem
                .phone_op(Instant::now(), |p, n| p.ussd(code.expose(), n))
                .map_err(mobile_err),
            Command::PhoneUssdCancel => self
                .modem
                .phone_op(Instant::now(), |p, _| p.ussd_cancel())
                .map_err(mobile_err),
            Command::PhoneSimContacts => self
                .modem
                .phone_op(Instant::now(), |p, _| p.sim_contacts())
                .map_err(mobile_err),
            Command::PhoneAnswer => self
                .modem
                .phone_op(Instant::now(), |p, n| p.answer(n))
                .map_err(mobile_err),
            Command::PhoneHangup => self
                .modem
                .phone_op(Instant::now(), |p, n| p.hangup(n))
                .map_err(mobile_err),
            Command::PhoneDtmf { digit } => {
                let c = digit.chars().next().unwrap_or(' ');
                self.modem
                    .phone_op(Instant::now(), |p, n| p.dtmf(c, n))
                    .map_err(mobile_err)
            }
            Command::PhoneMute { on } => self
                .modem
                .phone_op(Instant::now(), |p, n| p.mute(on, n))
                .map_err(mobile_err),
            Command::PhoneSpeaker { on } => self
                .modem
                .phone_op(Instant::now(), |p, n| p.speaker(on, n))
                .map_err(mobile_err),
            Command::SmsSend { number, text } => self
                .modem
                .phone_op(Instant::now(), |p, _| p.sms_send(number.expose(), text.expose()))
                .map_err(mobile_err),
            Command::SmsList => Ok(self.modem.phone().sms_list()),
            Command::SmsDelete { id } => self.modem.phone_mut().sms_delete(id).map_err(mobile_err),
            Command::SmsRead { number } => self
                .modem
                .phone_mut()
                .sms_read(number.expose())
                .map_err(mobile_err),
            Command::ContactList => Ok(self.modem.phone().contact_list()),
            Command::ContactSave { id, name, numbers } => {
                let nums = numbers.iter().map(|n| n.expose().to_string()).collect();
                self.modem
                    .phone_mut()
                    .contact_save(id.as_deref(), &name, nums)
                    .map_err(mobile_err)
            }
            Command::ContactDelete { id } => self
                .modem
                .phone_mut()
                .contact_delete(&id)
                .map_err(mobile_err),
            Command::ContactImport { path } => {
                let p = self.root.p(&path);
                self.modem.phone_mut().contact_import(&p).map_err(mobile_err)
            }
            Command::CallLog => Ok(self.modem.phone().call_log()),
            Command::CallLogClear => self.modem.phone_mut().call_log_clear().map_err(mobile_err),
            Command::Notifications => Ok(json!({"notifications": self.notifications.list()})),
            Command::NotificationMarkRead { id } => {
                let marked = self.notifications.mark_read(id);
                Ok(json!({"id": id, "marked": marked}))
            }
            Command::NotificationsMarkAllRead => {
                let marked = self.notifications.mark_all_read();
                Ok(json!({"marked": marked}))
            }
            Command::NotificationClear { id } => {
                let cleared = self.notifications.clear(id);
                Ok(json!({"id": id, "cleared": cleared}))
            }
            Command::NotificationsClearAll => {
                self.notifications.clear_all();
                Ok(json!({"cleared": true}))
            }
            Command::PkgCatalogue => Ok(self.pkg.catalogue_json()),
            Command::PkgInstall { id } => {
                self.pkg
                    .start(Op::Install, &id, Instant::now())
                    .map_err(pkg_err)?;
                Ok(json!({"accepted": true, "id": id}))
            }
            Command::PkgRemove { id } => {
                self.pkg
                    .start(Op::Remove, &id, Instant::now())
                    .map_err(pkg_err)?;
                // an app that runs is closed first: its window goes, then the package
                let closed = self.close_app(&id);
                Ok(json!({"accepted": true, "id": id, "closed": closed.is_some()}))
            }
            Command::DefaultApp { role, app } => {
                self.pkg.set_default(&role, &app).map_err(pkg_err)?;
                Ok(json!({"role": role, "app": app}))
            }
            _ => Ok(json!({})),
        }
    }
}
