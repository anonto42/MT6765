//! Tap handlers for Wi-Fi, Hotspot, and manual network configuration.

use crate::network;
use crate::overlay::{Overlay, PasswordSheet, SheetPurpose};
use serde_json::json;

use super::cmd::{Cmd, Outcome};
use super::target::Target;
use super::App;

impl App {
    /// Handle Wi-Fi, hotspot, and network configuration targets. Returns `true` if handled.
    pub(super) fn on_network_tap(
        &mut self,
        t: Target,
        interactive: bool,
        out: &mut Outcome,
    ) -> bool {
        match t {
            Target::WifiSwitch if interactive && self.model.status.is_some() => {
                out.cmd(Cmd::WifiEnable(!network::wifi_enabled(&self.model.status)))
            }
            Target::HotspotSwitch if interactive && self.model.status.is_some() => {
                self.hotspot_switch(out)
            }
            Target::HotspotName if interactive => {
                let name = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| s.hotspot.ssid.clone())
                    .unwrap_or_else(|| "WalPad".into());
                self.model.overlay = Some(Overlay::Password(PasswordSheet::hotspot(
                    SheetPurpose::HotspotName,
                    &name,
                )));
            }
            Target::HotspotPassword if interactive => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::hotspot(
                    SheetPurpose::HotspotPassword,
                    "",
                )));
            }
            Target::Scan
                if interactive
                    && !self.model.net.scanning
                    && !network::client_off(&self.model.status) =>
            {
                self.model.net.scanning = true;
                self.model.net.error = None;
                out.cmd(Cmd::WifiScan);
            }
            Target::Disconnect if interactive => out.cmd(Cmd::WifiDisconnect),
            Target::Avail(i) if interactive => self.on_available(i, out),
            Target::Saved(i) if interactive => {
                if let Some(s) = self.model.net.saved.get(i).cloned() {
                    let already =
                        network::is_connected(&self.model.status) == Some(s.ssid.as_str());
                    if !already && self.model.net.connecting.is_none() {
                        out.cmd(Cmd::WifiConnect {
                            ssid_hex: None,
                            secret: None,
                            saved_id: Some(s.id),
                        });
                    }
                }
            }
            Target::Forget(i) => {
                if let Some(s) = self.model.net.saved.get(i) {
                    self.model.overlay = Some(Overlay::Forget {
                        id: s.id,
                        ssid: s.ssid.clone(),
                    });
                }
            }
            Target::WifiManualSsid if interactive => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::WifiManualSsid,
                    "",
                )));
            }
            Target::NetMacMode if interactive => {
                let modes = crate::config::NET_MAC_MODES;
                let idx = modes
                    .iter()
                    .position(|&m| m == self.model.devd.net_mac_mode)
                    .unwrap_or(0);
                let next = modes[(idx + 1) % modes.len()];
                out.cmd(Cmd::Setting("net_mac_mode", json!(next)));
            }
            Target::NetMacEdit if interactive => {
                let cur = if !self.model.devd.net_mac.is_empty() {
                    self.model.devd.net_mac.clone()
                } else {
                    self.model.sys.mac.clone().unwrap_or_default()
                };
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::NetMac,
                    &cur,
                )));
            }
            Target::NetIpMode if interactive => {
                let next = if self.model.devd.net_ip_mode == "manual" {
                    "dhcp"
                } else {
                    "manual"
                };
                out.cmd(Cmd::Setting("net_ip_mode", json!(next)));
            }
            Target::NetIpEdit if interactive => {
                let cur = if !self.model.devd.net_ip.is_empty() {
                    self.model.devd.net_ip.clone()
                } else {
                    self.model
                        .status
                        .as_ref()
                        .and_then(|s| s.wifi.ip.clone())
                        .unwrap_or_default()
                };
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::NetIp,
                    &cur,
                )));
            }
            Target::NetMaskCycle if interactive => {
                let opts = crate::config::NET_MASK_OPTIONS;
                let idx = opts
                    .iter()
                    .position(|&m| m == self.model.devd.net_mask)
                    .unwrap_or(0);
                let next = opts[(idx + 1) % opts.len()];
                out.cmd(Cmd::Setting("net_mask", json!(next)));
            }
            Target::NetGatewayEdit if interactive => {
                let cur = if !self.model.devd.net_gateway.is_empty() {
                    self.model.devd.net_gateway.clone()
                } else {
                    self.model.sys.gateway.clone().unwrap_or_default()
                };
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::NetGateway,
                    &cur,
                )));
            }
            Target::NetHostnameEdit if interactive => {
                let cur = if !self.model.devd.net_hostname.is_empty() {
                    self.model.devd.net_hostname.clone()
                } else {
                    self.model.sys.hostname.clone().unwrap_or_default()
                };
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::NetHostname,
                    &cur,
                )));
            }
            _ => return false,
        }
        true
    }
}
