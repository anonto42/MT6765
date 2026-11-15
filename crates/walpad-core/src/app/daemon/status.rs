//! Status snapshot assembly, event publishing, and Wi-Fi switch persistence for `Daemon`.

use crate::logic::battery;
use serde_json::{json, Value};
use std::time::Instant;
use walpad_hw::{battery::read_battery, gsensor::read_gsensor, input, sysfs};
use walpad_proto::{encode_line, Event, Hardware, LegacyStatus, Status, Topic};

use super::errors::{internal, parse_wifi_state, wall_ms, write_atomic, CmdResult};
use super::{Daemon, WIFI_ENABLED_FILE};

impl Daemon {
    /// Send an event to every subscriber of `topic`.
    pub(super) fn publish(&mut self, topic: Topic, event: &str, data: Value) {
        let line = encode_line(&Event::new(event, data));
        for c in &mut self.clients {
            if c.hello && c.topics.contains(&topic) {
                c.queue(line.clone());
            }
        }
    }

    /// `wifi` event `vpn` with the current VPN status (state changes, failures, reverts).
    pub(super) fn publish_vpn(&mut self) {
        let data = match serde_json::to_value(self.vpn.status()) {
            Ok(Value::Object(mut m)) => {
                // `kind` names the event; the tunnel type moves to `tunnel`
                let t = m.remove("kind").unwrap_or(Value::Null);
                m.insert("tunnel".into(), t);
                m.insert("kind".into(), json!("vpn"));
                Value::Object(m)
            }
            _ => return,
        };
        self.publish(Topic::Wifi, "wifi", data);
    }

    pub fn status(&self) -> Status {
        let raw = read_battery(&self.root);
        let mut wifi =
            parse_wifi_state(sysfs::read_trim(&self.root, "/run/walpad/wifi-state").as_deref());
        wifi.enabled = self.wifi_enabled();
        // a stale supplicant reading must not show a connection while the client is off
        let client_off =
            !wifi.enabled || matches!(wifi.state.as_deref(), Some("disabled" | "hotspot"));
        if !client_off {
            let live = self.wifi.live();
            wifi.ssid = live.ssid.clone();
            wifi.signal_dbm = live.signal_dbm;
            if wifi.ip.is_none() {
                wifi.ip = live.ip.clone();
            }
        }
        let is_touch = |n: &str| {
            self.profile
                .input
                .touch_name_patterns
                .iter()
                .any(|p| n.to_ascii_lowercase().contains(&p.to_ascii_lowercase()))
                || input::is_touch(n)
        };
        let devs = input::discover(&self.root);
        let bt = self.bluetooth.status();
        let ok = |b: bool| if b { "ok" } else { "unavailable" }.to_string();
        Status {
            battery: battery::model(&raw, self.brain.guard.low(), Some(&self.soc_out)),
            screen: self.brain.screen.state(),
            brightness: self.brain.screen.brightness(),
            orientation: self.brain.rotation.orientation(),
            rotation_locked: self.brain.rotation.locked(),
            lid_closed: self.brain.screen.lid_closed(),
            wifi,
            hotspot: self.hotspot.status().clone(),
            vpn: self.vpn.status().clone(),
            mobile: self.modem.status().clone(),
            phone: self.modem.phone_status(Instant::now()),
            gps: self.gnss.status().clone(),
            stats: self.stats.status().clone(),
            bluetooth: bt.clone(),
            audio: self.volume.status(),
            jack: self.jack.status(),
            lock: self.lock.status(wall_ms()),
            apps: self.apps.status(&self.root),
            hardware: Hardware {
                touch: ok(input::find(&devs, is_touch).is_some()),

                gsensor: ok(read_gsensor(&self.root).is_some()),
                audio: "unverified".into(),
                bluetooth: if bt.controller == walpad_proto::BluetoothController::Present {
                    "ok"
                } else {
                    "unavailable"
                }
                .into(),
                sd_card: "unavailable".into(),
            },
            notifications: self.notifications.status(),
        }
    }

    /// The Wi-Fi switch: anything but "0" in the state file (or no file) is on.
    fn wifi_enabled(&self) -> bool {
        sysfs::read_trim(&self.root, WIFI_ENABLED_FILE).as_deref() != Some("0")
    }

    /// `wifi_enable`: persist the switch; walpad-wifi's loop stops or restarts the supplicant
    /// within about a second. Off also drops the current association at once.
    pub(super) fn wifi_enable(&mut self, on: bool) -> CmdResult {
        let p = self.root.p(WIFI_ENABLED_FILE);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(internal)?;
        }
        write_atomic(&p, if on { b"1\n" } else { b"0\n" }).map_err(internal)?;
        if !on && self.wifi.connected() {
            let _ = self.wifi.disconnect();
        }
        eprintln!(
            "walpad-core: Wi-Fi switch {}",
            if on { "on" } else { "off" }
        );
        Ok(json!({
            "enabled": on,
            // the client stays paused while an exclusive hotspot runs
            "held_by_hotspot": on && self.hotspot.status().exclusive,
        }))
    }

    /// `audio` event on the status topic: `{volume, muted, available, source: "key"|"ipc"}`. The
    /// shell draws its volume pill for `key` (a slider drag needs no extra feedback).
    pub(super) fn publish_audio(&mut self, source: &str, a: walpad_proto::AudioStatus) {
        self.publish(
            Topic::Status,
            "audio",
            json!({"volume": a.volume, "muted": a.muted, "available": a.available, "source": source}),
        );
    }

    /// `wifi` event `hotspot` with the current hotspot status (state changes, failures).
    pub(super) fn publish_hotspot(&mut self) {
        let data = match serde_json::to_value(self.hotspot.status()) {
            Ok(Value::Object(mut m)) => {
                m.insert("kind".into(), json!("hotspot"));
                Value::Object(m)
            }
            _ => return,
        };
        self.publish(Topic::Wifi, "wifi", data);
    }

    /// `gps` event on the status topic with the current GPS status (owner request 2026-11-11).
    /// The position is private: it goes only to subscribed clients, never to a log.
    pub(super) fn publish_gps(&mut self) {
        if let Ok(v) = serde_json::to_value(self.gnss.status()) {
            self.publish(Topic::Status, "gps", v);
        }
    }

    pub(super) fn write_status_json(&self, s: &Status) {
        let wifi_raw =
            sysfs::read_trim(&self.root, "/run/walpad/wifi-state").unwrap_or_else(|| "off".into());
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let up = sysfs::uptime_s(&self.root).unwrap_or(0.0);
        let legacy = LegacyStatus::from_status(s, time, up, &wifi_raw);
        let dst = self.root.p("/run/walpad/status.json");
        if let Ok(js) = serde_json::to_vec(&legacy) {
            let _ = write_atomic(&dst, &js);
        }
    }

    pub(super) fn push_status_if_changed(&mut self) {
        let s = self.status();
        if self.last_pushed.as_ref() != Some(&s) {
            let ev = encode_line(&Event::new(
                "status",
                serde_json::to_value(&s).unwrap_or_default(),
            ));
            for c in &mut self.clients {
                if c.hello && c.topics.contains(&Topic::Status) {
                    c.queue(ev.clone());
                }
            }
            self.last_pushed = Some(s);
        }
    }
}
