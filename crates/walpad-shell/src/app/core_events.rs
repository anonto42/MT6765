//! Core daemon event and Wi-Fi/VPN/Hotspot event handlers for `App`.

use crate::core_client::CoreEvent;
use crate::network::{bars_from_dbm, NetEntry};
use crate::overlay::Overlay;
use crate::volume_ui::{self, Osd};
use serde_json::Value;
use walpad_ui::components::{Tab, Tone};

use super::cmd::{Cmd, Outcome};
use super::model::Ms;
use super::App;

impl App {
    pub fn on_core(&mut self, ev: &CoreEvent, now: Ms) -> Outcome {
        self.now = now;
        let mut out = Outcome::default();
        match ev {
            CoreEvent::Connected => {
                self.model.connected = true;
                if self.active == Tab::Network {
                    out.cmd(Cmd::WifiListSaved);
                }
                if self.active == Tab::Phone {
                    self.open_phone(&mut out);
                }
                out.dirty = true;
            }
            CoreEvent::Disconnected => {
                self.model.connected = false;
                self.model.status = None;
                self.model.net.scanning = false;
                self.model.net.connecting = None;
                self.model.bt_busy = None;
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.connecting = false;
                }
                if matches!(self.model.overlay, Some(Overlay::BtPair { .. })) {
                    self.model.overlay = None;
                }
                self.pending.clear();
                if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
                    self.model.overlay = None;
                }
                self.rebuild_tiles();
                out.dirty = true;
            }
            CoreEvent::Status(s) => {
                // The GPS fields (age, signal levels) change every couple of seconds while it
                // runs: redraw only where they are visible (GPS screen, Quick Settings) or when
                // its state changed.
                let quiet = self.active != Tab::Gps
                    && self.model.overlay != Some(Overlay::QuickSettings)
                    && self.model.status.as_ref().is_some_and(|o| {
                        let mut a = o.clone();
                        a.gps = s.gps.clone();
                        o.gps.state == s.gps.state && a == **s
                    });
                // owner request 2026-11-12: a low-battery alert is a toast, not a persistent
                // full-width block - fire once on the transition into low, not every status poll
                let was_low = self.model.status.as_ref().is_some_and(|o| o.battery.low);
                let charging = s
                    .battery
                    .state
                    .as_deref()
                    .is_some_and(|st| st.to_ascii_lowercase().starts_with("charg"));
                if s.battery.low && !was_low && !charging {
                    self.toast("Battery low - connect the charger", Tone::Warning);
                }
                self.model.status = Some((**s).clone());
                self.model.connected = true;
                self.rebuild_tiles();
                out.dirty = !quiet;
            }
            CoreEvent::Wifi(v) => self.on_wifi_event(v, &mut out),
            CoreEvent::Notice(v)
                if v["kind"].as_str().is_some_and(|k| k.starts_with("mobile_")) =>
            {
                self.on_mobile_notice(v, &mut out)
            }
            CoreEvent::Notice(v)
                if v["kind"]
                    .as_str()
                    .is_some_and(|k| k.starts_with("bluetooth")) =>
            {
                self.on_bt_notice(v, &mut out)
            }
            CoreEvent::Notice(v) if v["kind"].as_str().is_some_and(|k| k.starts_with("pkg_")) => {
                self.on_pkg_notice(v, &mut out)
            }
            CoreEvent::Notice(v)
                if matches!(
                    v["kind"].as_str(),
                    Some(
                        "incoming_call"
                            | "call_ended"
                            | "sms_received"
                            | "sms_sent"
                            | "phone_error"
                    )
                ) =>
            {
                self.on_phone_notice(v, &mut out)
            }
            CoreEvent::Notice(v) => {
                if v["kind"] == "app_crashed" {
                    let name = v["name"].as_str().or(v["app"].as_str()).unwrap_or("An app");
                    self.toast(format!("{name} closed unexpectedly"), Tone::Danger);
                    out.dirty = true;
                }
            }
            CoreEvent::Input(k) => self.on_hw_key(*k, &mut out),
            // the volume pill for a hardware key (a slider drag needs no extra feedback)
            CoreEvent::Audio(v) if v["source"] == "key" => {
                self.model.osd = Some(Osd {
                    volume: v["volume"].as_u64().unwrap_or(0).min(100) as u8,
                    muted: v["muted"] == true,
                    available: v["available"] == true,
                    until: now + volume_ui::OSD_MS,
                });
                out.dirty = true;
            }
            CoreEvent::Audio(_) => {}
            // app lifecycle events drive the system keyboard (shell loop), nothing to draw here
            CoreEvent::Apps(_) => {}
            CoreEvent::Reply {
                id,
                ok,
                data,
                error,
            } => self.on_reply(*id, *ok, data, error.as_ref(), &mut out),
        }
        out
    }

    fn on_wifi_event(&mut self, v: &Value, out: &mut Outcome) {
        match v["kind"].as_str() {
            Some("scan") => {
                self.model.net.results = v["networks"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| {
                                Some(NetEntry {
                                    ssid_hex: n["ssid_hex"].as_str()?.to_string(),
                                    ssid: n["ssid"].as_str()?.to_string(),
                                    bars: bars_from_dbm(
                                        n["signal_dbm"].as_i64().unwrap_or(-90) as i32
                                    ),
                                    secured: n["security"].as_str() != Some("open"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.model.net.scanning = false;
                self.model.net.scanned = true;
                self.model.net.error = None;
                out.dirty = true;
            }
            Some("connect") => {
                let ssid = v["ssid"].as_str().unwrap_or("").to_string();
                let hex = v["ssid_hex"].as_str().unwrap_or("").to_string();
                self.model.net.connecting = None;
                if v["ok"] == true {
                    self.toast(format!("Connected to {ssid}"), Tone::Info);
                    if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.ssid_hex == hex)
                    {
                        self.model.overlay = None;
                    }
                    out.cmd(Cmd::WifiListSaved);
                } else {
                    let text = match v["error"].as_str() {
                        Some("wrong_password") => "Wrong password",
                        Some("timeout") => "Timed out after 25 s",
                        _ => "Could not connect",
                    };
                    match &mut self.model.overlay {
                        Some(Overlay::Password(s)) if s.ssid_hex == hex => {
                            s.connecting = false;
                            s.error = Some(text.into());
                        }
                        _ => self.toast(format!("{text}: {ssid}"), Tone::Danger),
                    }
                }
                out.dirty = true;
            }
            Some("disconnected") => {
                self.toast("Wi-Fi disconnected", Tone::Warning);
                out.dirty = true;
            }
            Some("vpn") => {
                match v["state"].as_str() {
                    Some("on") => self.toast("VPN connected", Tone::Info),
                    Some("failed") => {
                        let why = v["reason"].as_str().unwrap_or("unknown error");
                        self.toast(format!("VPN: {why}"), Tone::Danger)
                    }
                    _ => {}
                }
                out.dirty = true;
            }
            Some("hotspot") => {
                match v["state"].as_str() {
                    Some("on") if v["exclusive"] == true => {
                        self.toast("Hotspot on, Wi-Fi client paused", Tone::Warning)
                    }
                    Some("on") => self.toast("Hotspot on", Tone::Info),
                    Some("failed") => {
                        let why = v["reason"].as_str().unwrap_or("unknown error");
                        self.toast(format!("Hotspot failed: {why}"), Tone::Danger)
                    }
                    _ => {}
                }
                out.dirty = true;
            }
            _ => {}
        }
    }
}
