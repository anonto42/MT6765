//! Tap handlers for connectivity screens: VPN, GPS, Mobile, and Bluetooth.

use crate::bluetooth::{self, PairState};
use crate::gps_screen;
use crate::mobile;
use crate::overlay::{Overlay, PasswordSheet, SheetPurpose};
use crate::settings_screen::Page;
use crate::vpn_screen;
use walpad_ui::components::{Tab, Tone};

use super::cmd::{Cmd, Outcome};
use super::target::Target;
use super::App;

impl App {
    /// Handle connectivity targets. Returns `true` if `t` belonged to connectivity.
    pub(super) fn on_connectivity_tap(
        &mut self,
        t: Target,
        interactive: bool,
        out: &mut Outcome,
    ) -> bool {
        match t {
            Target::Page(Page::Vpn) => self.go(Tab::Vpn, out),
            Target::VpnSwitch if interactive && vpn_screen::switchable(&self.model.status) => {
                out.cmd(Cmd::Vpn(!vpn_screen::is_on(&self.model.status)))
            }
            Target::VpnKill if interactive && vpn_screen::editable(&self.model.status) => {
                let ks = self
                    .model
                    .status
                    .as_ref()
                    .is_some_and(|s| s.vpn.kill_switch);
                out.cmd(Cmd::VpnConfig {
                    profile: None,
                    kill_switch: Some(!ks),
                    dns: None,
                    proxy: None,
                })
            }
            Target::VpnKeep if interactive => out.cmd(Cmd::VpnKeep),
            Target::VpnProfile if interactive && vpn_screen::editable(&self.model.status) => {
                if let Some(s) = &self.model.status {
                    let names: Vec<&String> = s.vpn.profiles.iter().map(|p| &p.name).collect();
                    let cur = names
                        .iter()
                        .position(|n| Some(*n) == s.vpn.profile.as_ref());
                    if let Some(next) = names.get(cur.map_or(0, |i| (i + 1) % names.len().max(1))) {
                        out.cmd(Cmd::VpnConfig {
                            profile: Some((*next).clone()),
                            kill_switch: None,
                            dns: None,
                            proxy: None,
                        });
                    }
                }
            }
            Target::VpnImport(i) if interactive => {
                if let Some(f) = self.model.vpn_files.get(i) {
                    out.cmd(Cmd::VpnImport(f.path.clone()));
                }
            }
            Target::VpnDns | Target::VpnProxy if interactive => {
                let dns = matches!(t, Target::VpnDns);
                let cur = self.model.status.as_ref().map(|s| {
                    if dns {
                        s.vpn.dns.join(", ")
                    } else {
                        s.vpn.proxy.clone().unwrap_or_default()
                    }
                });
                self.model.overlay = Some(Overlay::Password(PasswordSheet::vpn(
                    if dns {
                        SheetPurpose::VpnDns
                    } else {
                        SheetPurpose::VpnProxy
                    },
                    &cur.unwrap_or_default(),
                )));
            }
            Target::WifiCard | Target::HotspotCard => self.go(Tab::Network, out),
            Target::MobileCard | Target::Page(Page::Mobile) => self.go(Tab::Mobile, out),
            Target::BtCard | Target::Page(Page::Bluetooth) => self.go(Tab::Bluetooth, out),
            Target::GpsCard | Target::Page(Page::Gps) => self.go(Tab::Gps, out),
            Target::GpsSwitch if interactive && gps_screen::switchable(&self.model.status) => {
                out.cmd(Cmd::Gps(!gps_screen::switch_on(&self.model.status)))
            }
            Target::GpsCopyCoords if interactive => {
                let text = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| gps_screen::coords_text(&s.gps));
                if let Some(t) = text {
                    out.cmd(Cmd::CopyText(t));
                    self.toast("Position copied", Tone::Info);
                }
            }
            Target::GpsCopyLink if interactive => {
                let text = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| gps_screen::map_link(&s.gps));
                if let Some(t) = text {
                    out.cmd(Cmd::CopyText(t));
                    self.toast("Map link copied: paste it in the browser", Tone::Info);
                }
            }
            Target::GpsOpenMap if interactive => {
                let coords = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| Some((s.gps.lat?, s.gps.lon?)));
                out.cmd(Cmd::OpenMap(coords));
                self.toast("Opening OpenStreetMap...", Tone::Info);
            }
            Target::BtSwitch if interactive && bluetooth::switchable(&self.model.status) => {
                out.cmd(Cmd::BtEnable(!bluetooth::switch_on(&self.model.status)))
            }
            Target::BtScan if interactive && bluetooth::powered(&self.model.status) => {
                out.cmd(Cmd::BtScan(!bluetooth::scanning(&self.model.status)))
            }
            Target::BtAction(i) if interactive && self.model.bt_busy.is_none() => {
                let d = bluetooth::my_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.connected));
                if let Some((address, connected)) = d {
                    self.model.bt_busy = Some(address.clone());
                    out.cmd(if connected {
                        Cmd::BtDisconnect(address)
                    } else {
                        Cmd::BtConnect(address)
                    });
                }
            }
            Target::BtForget(i) if interactive => {
                let d = bluetooth::my_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.name.clone()));
                if let Some((address, name)) = d {
                    self.model.overlay = Some(Overlay::BtForget { address, name });
                }
            }
            Target::BtAvail(i) if interactive && self.model.bt_busy.is_none() => {
                let d = bluetooth::available_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.name.clone()));
                if let Some((address, name)) = d {
                    self.model.bt_busy = Some(address.clone());
                    self.model.overlay = Some(Overlay::BtPair {
                        address: address.clone(),
                        name,
                        state: PairState::Waiting,
                    });
                    out.cmd(Cmd::BtPair(address));
                }
            }
            Target::BtPairAccept | Target::BtPairReject => {
                let accept = t == Target::BtPairAccept;
                if let Some(Overlay::BtPair { address, name, .. }) = self.model.overlay.take() {
                    out.cmd(Cmd::BtConfirm {
                        address: address.clone(),
                        accept,
                    });
                    if accept {
                        self.model.overlay = Some(Overlay::BtPair {
                            address,
                            name,
                            state: PairState::Waiting,
                        });
                    } else {
                        self.model.bt_busy = None;
                    }
                }
            }
            Target::MobileRadio if interactive && mobile::radio_switchable(&self.model.status) => {
                out.cmd(Cmd::MobileRadio(!mobile::radio_on(&self.model.status)))
            }
            Target::MobileDataSwitch
                if interactive && mobile::data_switchable(&self.model.status) =>
            {
                out.cmd(Cmd::MobileData(!mobile::data_on(&self.model.status)))
            }
            Target::MobileApn if interactive => {
                let apn = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| s.mobile.apn.clone())
                    .unwrap_or_default();
                self.model.overlay = Some(Overlay::Password(PasswordSheet::mobile(
                    SheetPurpose::MobileApn,
                    &apn,
                )));
            }
            Target::SimPinEntry if interactive && mobile::pin_needed(&self.model.status) => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::mobile(
                    SheetPurpose::SimPin,
                    "",
                )));
            }
            Target::ImeiShow if interactive => {
                self.model.imei_shown = !self.model.imei_shown;
                if self.model.imei_shown && self.model.mobile_info.is_none() {
                    out.cmd(Cmd::MobileInfo);
                }
            }
            _ => return false,
        }
        true
    }
}
