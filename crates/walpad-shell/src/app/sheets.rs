//! Password/config sheet submission, Wi-Fi network selection, hotspot toggle, and on-screen keys.

use crate::network;
use crate::overlay::{self, Overlay, PasswordSheet, SheetPurpose};
use serde_json::json;
use walpad_proto::Secret;
use walpad_ui::components::{Key, KeyboardLayer, Tab};

use super::cmd::{Cmd, Outcome};
use super::App;

impl App {
    pub(super) fn on_available(&mut self, i: usize, out: &mut Outcome) {
        let Some(n) = self.model.net.results.get(i).cloned() else {
            return;
        };
        if network::is_connected(&self.model.status) == Some(n.ssid.as_str())
            || self.model.net.connecting.is_some()
        {
            return;
        }
        if let Some(saved) = self.model.net.saved.iter().find(|s| s.ssid == n.ssid) {
            // credentials are already stored: just select it
            self.model.net.connecting = Some(n.ssid_hex.clone());
            out.cmd(Cmd::WifiConnect {
                ssid_hex: None,
                secret: None,
                saved_id: Some(saved.id),
            });
        } else if n.secured {
            self.model.overlay = Some(Overlay::Password(PasswordSheet::new(n.ssid_hex, n.ssid)));
        } else {
            self.model.net.connecting = Some(n.ssid_hex.clone());
            out.cmd(Cmd::WifiConnect {
                ssid_hex: Some(n.ssid_hex),
                secret: None,
                saved_id: None,
            });
        }
    }

    /// Hotspot switch: off when it runs or starts; on needs a password (opens the sheet first).
    pub(super) fn hotspot_switch(&mut self, out: &mut Outcome) {
        let Some(has_secret) = self.model.status.as_ref().map(|s| s.hotspot.has_secret) else {
            return;
        };
        if network::hotspot_on(&self.model.status) {
            out.cmd(Cmd::Hotspot(false));
        } else if !has_secret {
            if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
                self.go(Tab::Network, out);
            }
            let mut s = PasswordSheet::hotspot(SheetPurpose::HotspotPassword, "");
            s.error = Some("Set a password first".into());
            self.model.overlay = Some(Overlay::Password(s));
        } else {
            out.cmd(Cmd::Hotspot(true));
        }
    }

    pub(super) fn try_connect(&mut self, out: &mut Outcome) {
        if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_lock()) {
            return self.lock_submit(out);
        }
        if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_phone()) {
            return self.phone_submit(out);
        }
        if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_net_config()) {
            if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                if !s.valid() {
                    s.error = Some("Invalid value".into());
                    return;
                }
                let val = s.secret.clone();
                match s.purpose {
                    SheetPurpose::WifiManualSsid => {
                        let hex: String = val.bytes().map(|b| format!("{b:02x}")).collect();
                        self.model.overlay = Some(Overlay::Password(PasswordSheet::new(hex, val)));
                    }
                    SheetPurpose::NetIp => {
                        let mode = if val.is_empty() { "dhcp" } else { "manual" };
                        out.cmd(Cmd::Setting("net_ip_mode", json!(mode)));
                        out.cmd(Cmd::Setting("net_ip", json!(val)));
                        self.close_overlay();
                    }
                    SheetPurpose::NetGateway => {
                        out.cmd(Cmd::Setting("net_gateway", json!(val)));
                        self.close_overlay();
                    }
                    SheetPurpose::NetMac => {
                        let up = overlay::normalize_mac(&val);
                        let mode = if up.is_empty() { "hardware" } else { "custom" };
                        out.cmd(Cmd::Setting("net_mac_mode", json!(mode)));
                        out.cmd(Cmd::Setting("net_mac", json!(up)));
                        self.close_overlay();
                    }
                    SheetPurpose::NetHostname => {
                        out.cmd(Cmd::Setting("net_hostname", json!(val)));
                        self.close_overlay();
                    }
                    SheetPurpose::WallpaperPath => {
                        self.model.wallpaper_path = val.clone();
                        let kind = if val.is_empty() {
                            walpad_ui::art::WallpaperKind::Default
                        } else {
                            walpad_ui::art::WallpaperKind::Custom
                        };
                        self.model.wallpaper = kind;
                        out.cmd(Cmd::Setting("wallpaper_path", json!(val)));
                        out.cmd(Cmd::Setting("wallpaper", json!(kind.as_str())));
                        self.close_overlay();
                    }
                    SheetPurpose::ClockTime => {
                        out.cmd(Cmd::SetTime(val));
                        self.close_overlay();
                    }
                    _ => {}
                }
            }
            return;
        }
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            if s.is_vpn() {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    out.cmd(if s.purpose == SheetPurpose::VpnDns {
                        Cmd::VpnConfig {
                            profile: None,
                            kill_switch: None,
                            dns: Some(overlay::dns_list(&s.secret)),
                            proxy: None,
                        }
                    } else {
                        Cmd::VpnConfig {
                            profile: None,
                            kill_switch: None,
                            dns: None,
                            proxy: Some(s.secret.clone()),
                        }
                    });
                } else if !s.valid() {
                    s.error = Some(
                        if s.purpose == SheetPurpose::VpnDns {
                            "Up to 3 IP addresses, e.g. 1.1.1.1"
                        } else {
                            "Use http://host:port, no user name or password"
                        }
                        .into(),
                    );
                }
                return;
            }
            if s.is_mobile() {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    out.cmd(if s.purpose == SheetPurpose::SimPin {
                        Cmd::SimPin(Secret::new(s.secret.clone()))
                    } else {
                        Cmd::MobileApn(s.secret.clone())
                    });
                } else if !s.valid() {
                    s.error = Some(
                        if s.purpose == SheetPurpose::SimPin {
                            "The PIN has 4-8 digits"
                        } else {
                            "Use letters, digits, . - _ (up to 62)"
                        }
                        .into(),
                    );
                }
                return;
            }
            if s.purpose != SheetPurpose::WifiJoin {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    let text = s.secret.clone();
                    out.cmd(if s.purpose == SheetPurpose::HotspotName {
                        Cmd::HotspotConfig {
                            ssid: Some(text),
                            secret: None,
                        }
                    } else {
                        Cmd::HotspotConfig {
                            ssid: None,
                            secret: Some(Secret::new(text)),
                        }
                    });
                }
                return;
            }
            if s.valid() && !s.connecting {
                s.connecting = true;
                s.error = None;
                self.model.net.connecting = Some(s.ssid_hex.clone());
                out.cmd(Cmd::WifiConnect {
                    ssid_hex: Some(s.ssid_hex.clone()),
                    secret: Some(Secret::new(s.secret.clone())),
                    saved_id: None,
                });
            }
        }
    }

    pub(super) fn on_key(&mut self, idx: usize, out: &mut Outcome) {
        let frame = self.frame;
        let Some(Overlay::Password(s)) = &mut self.model.overlay else {
            return;
        };
        let Some(key) = overlay::key_at(&frame, s.layer, idx) else {
            return;
        };
        match key {
            Key::Char(c) => {
                if s.secret.len() + c.len_utf8() <= s.max_len() {
                    s.secret.push(c);
                    s.error = None;
                }
                if s.shift {
                    s.shift = false;
                    s.layer = KeyboardLayer::Lower;
                }
            }
            Key::Space => {
                if s.secret.len() < s.max_len() {
                    s.secret.push(' ');
                    s.error = None;
                }
            }
            Key::Backspace => {
                s.secret.pop();
                s.error = None;
            }
            Key::Shift => {
                s.shift = s.layer != KeyboardLayer::Upper;
                s.layer = if s.shift {
                    KeyboardLayer::Upper
                } else {
                    KeyboardLayer::Lower
                };
            }
            Key::Layer => {
                s.shift = false;
                s.layer = if s.layer == KeyboardLayer::Symbols {
                    KeyboardLayer::Lower
                } else {
                    KeyboardLayer::Symbols
                };
            }
            Key::Enter => self.try_connect(out),
        }
    }
}
