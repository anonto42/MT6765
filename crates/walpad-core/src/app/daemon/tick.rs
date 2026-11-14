//! Periodic subsystem ticking, notification polling, and status broadcasting for [`Daemon`].
use super::errors::{wall_ms, wifi_event_json};
use super::{Daemon, BATTERY_EVERY, STATUS_EVERY};
use crate::logic::{battery, notifications as notif_logic};
use serde_json::json;
use std::time::Instant;
use walpad_hw::{battery::read_battery, usb};
use walpad_proto::{encode_line, Event, ScreenState, Topic};

impl Daemon {
    /// Step periodic subsystems after I/O servicing and publish any resulting status/events.
    pub(super) fn tick_subsystems(&mut self, mut did: bool) {
        did |= self.app_tick();
        let wall = wall_ms();
        self.lock
            .screen(self.brain.screen.state() == ScreenState::Off, wall);
        did |= self.lock.tick(wall);
        for ev in self.wifi.tick(Instant::now()) {
            let line = encode_line(&Event::new("wifi", wifi_event_json(&ev)));
            for c in &mut self.clients {
                if c.hello && c.topics.contains(&Topic::Wifi) {
                    c.queue(line.clone());
                }
            }
            did = true;
        }
        if self.hotspot.tick(Instant::now()) {
            self.publish_hotspot();
            did = true;
        }
        // header stats (owner request 2026-11-11): sampled only while an item is on
        did |= self.stats.tick(Instant::now());
        if self.gnss.tick(Instant::now()) {
            self.publish_gps();
            did = true;
        }
        did |= self.bluetooth.tick(Instant::now());
        for n in self.bluetooth.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        if self.pkg.tick(Instant::now()) {
            // an app was installed or removed: the allowlist follows
            self.apps.reload(&self.root);
            did = true;
        }
        for n in self.pkg.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        if self.vpn.tick(Instant::now()) {
            self.publish_vpn();
            did = true;
        }
        did |= self.modem.tick(Instant::now());
        did |= self.volume.tick(Instant::now());
        did |= self.jack.tick(Instant::now());
        for n in self.modem.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        did |= self.tick_notifications();
        let now = Instant::now();
        if now >= self.next_soc {
            self.soc_tick(now);
            did = true;
        }
        // outside live device mode the battery guard only logs (the Python daemon owns shutdown)
        if self.live.as_ref().is_none_or(|l| !l.device) && now >= self.next_battery {
            self.next_battery = now + BATTERY_EVERY;
            let raw = read_battery(&self.root);
            for a in self.brain.guard.step(&raw, &self.brain.batt_cfg) {
                match a {
                    battery::Act::CriticalPoweroff { mv } => eprintln!(
                        "walpad-core: CRITICAL battery {mv} mV (not acting: device subsystem is not live)"
                    ),
                    battery::Act::LowChanged(l) => {
                        eprintln!("walpad-core: low battery flag -> {l}")
                    }
                }
            }
            did = true;
        }
        if did || now >= self.next_status {
            let s = self.status();
            if now >= self.next_status {
                self.next_status = now + STATUS_EVERY;
                self.write_status_json(&s);
            }
            self.push_status_if_changed();
        }
    }

    fn tick_notifications(&mut self) -> bool {
        // notification centre (owner request 2026-11-12, change device-notifications): watches
        // status transitions already tracked above, plus the new USB/OTG and removable-storage
        // sysfs reads (walpad_hw::usb). Rate-limited to ~2 s via `Notifications::is_due`.
        let notif_now = Instant::now();
        if !self.notifications.is_due(notif_now) {
            return false;
        }
        let raw = read_battery(&self.root);
        let charging = battery::effective_status(&raw)
            .as_deref()
            .is_some_and(|s| s.to_ascii_lowercase().starts_with("charg"));
        let usb_devices: Vec<(String, String)> = usb::attached_devices(&self.root)
            .into_iter()
            .map(|d| {
                let name = d.name.unwrap_or_else(|| d.id.clone());
                (d.id, name)
            })
            .collect();
        let removable_mounts = usb::removable_mounts(&self.root);
        let bt = self.bluetooth.status();
        let mut bt_connected: Vec<(String, String)> = bt
            .devices
            .iter()
            .filter(|d| d.connected)
            .map(|d| (d.address.clone(), d.name.clone()))
            .collect();
        bt_connected.sort();
        let mobile = self.modem.status();
        let hotspot = self.hotspot.status();
        let gps = self.gnss.status();
        let snap = notif_logic::Snapshot {
            charging,
            usb_devices,
            removable_mounts,
            bt_connected,
            mobile_data_on: mobile.data == walpad_proto::MobileData::On,
            hotspot_clients: hotspot.clients.unwrap_or(0),
            gps_fix: gps.state == walpad_proto::GpsState::Fix,
        };
        let fired = self.notifications.tick(notif_now, wall_ms(), snap);
        if fired.is_empty() {
            return false;
        }
        for n in fired {
            if let Ok(v) = serde_json::to_value(&n) {
                self.publish(
                    Topic::Notice,
                    "notice",
                    json!({"kind": "notification", "notification": v}),
                );
            }
        }
        true
    }
}
