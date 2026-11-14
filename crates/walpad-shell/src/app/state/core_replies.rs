//! Core IPC reply dispatcher for `App`.

use crate::core_client::ReplyError;
use crate::mobile::MobileInfo;
use crate::network::SavedEntry;
use crate::overlay::{Overlay, SheetPurpose};
use serde_json::Value;
use walpad_ui::components::Tone;

use super::cmd::{Cmd, Outcome, Purpose};
use super::model::friendly;
use super::App;

impl App {
    fn fail_sheet_or_toast(&mut self, sheet: bool, msg: String) {
        match &mut self.model.overlay {
            Some(Overlay::Password(s)) if sheet => {
                s.connecting = false;
                s.error = Some(msg);
            }
            _ => self.toast(msg, Tone::Danger),
        }
    }

    pub(super) fn on_reply(
        &mut self,
        id: u64,
        ok: bool,
        data: &Value,
        error: Option<&ReplyError>,
        out: &mut Outcome,
    ) {
        let purpose = self.pending.remove(&id).unwrap_or(Purpose::Other);
        out.dirty = true;
        match (purpose, ok) {
            (Purpose::SavedList, true) => {
                self.model.net.saved = data
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| {
                                Some(SavedEntry {
                                    id: s["id"].as_u64()? as u32,
                                    ssid: s["ssid"].as_str()?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (Purpose::Scan, false) => {
                let e = error.cloned().unwrap_or(ReplyError {
                    code: "error".into(),
                    msg: "scan failed".into(),
                });
                if e.code != "busy" {
                    self.model.net.scanning = false;
                    self.model.net.error = Some(friendly(&e));
                }
            }
            (Purpose::Connect, false) => {
                self.model.net.connecting = None;
                let msg = error
                    .map(friendly)
                    .unwrap_or_else(|| "Could not connect".into());
                let is_pwd = matches!(self.model.overlay, Some(Overlay::Password(_)));
                self.fail_sheet_or_toast(is_pwd, msg);
            }
            (Purpose::Forget, true) => out.cmd(Cmd::WifiListSaved),
            (Purpose::BtPair | Purpose::BtOp, false) => {
                self.model.bt_busy = None;
                if purpose == Purpose::BtPair
                    && matches!(self.model.overlay, Some(Overlay::BtPair { .. }))
                {
                    self.model.overlay = None;
                }
                if let Some(e) = error {
                    self.toast(friendly(e), Tone::Danger);
                }
            }
            (Purpose::HotspotConfig, ok) => {
                let sheet = matches!(&self.model.overlay,
                    Some(Overlay::Password(s)) if s.purpose != SheetPurpose::WifiJoin);
                if ok {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast(
                        if data["restarting"] == true {
                            "Saved, restarting the hotspot"
                        } else {
                            "Hotspot settings saved"
                        },
                        Tone::Info,
                    );
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    self.fail_sheet_or_toast(sheet, msg);
                }
            }
            (Purpose::Lock, ok) => self.on_lock_reply(ok, error, out),
            (Purpose::MobileInfo, true) => {
                self.model.mobile_info = Some(MobileInfo {
                    imei: data["imei"].as_str().map(String::from),
                    iccid: data["iccid"].as_str().map(String::from),
                });
            }
            (Purpose::MobileApn | Purpose::SimPin, ok) => {
                let sheet =
                    matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_mobile());
                if ok && purpose == Purpose::MobileApn {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast(
                        if data["reconnect"] == true {
                            "APN saved: turn mobile data off and on to use it"
                        } else {
                            "APN saved"
                        },
                        Tone::Info,
                    );
                } else if !ok {
                    // PIN accepted for checking: the sheet waits for the `mobile_pin` notice
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    self.fail_sheet_or_toast(sheet, msg);
                }
            }
            (Purpose::PhoneSms | Purpose::PhoneContacts | Purpose::PhoneCalls, true) => {
                self.on_phone_data(purpose, data)
            }
            (Purpose::PhoneAction, ok) => self.on_phone_action_reply(ok, data, error, out),
            (Purpose::PkgCatalogue, true) => self.model.store.apply_catalogue(data),
            (Purpose::PkgCatalogue, false) => {
                self.model.store.error =
                    Some(error.map_or_else(|| "Cannot read the catalogue".into(), friendly));
            }
            (Purpose::PkgAction, false) => {
                // refused (no space, built in, busy ...): nothing changed, say why
                self.model.store.busy = None;
                self.model.store.error =
                    Some(error.map_or_else(|| "The request failed".into(), friendly));
            }
            (Purpose::PkgAction, true) => {}
            (Purpose::DefaultApp, true) => out.cmd(Cmd::PkgCatalogue),
            (Purpose::DefaultApp, false) => {
                self.model.store.error =
                    Some(error.map_or_else(|| "Could not change the default app".into(), friendly));
            }
            (Purpose::VpnFiles, true) => {
                self.model.vpn_files = data["files"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|f| {
                                Some(crate::vpn_screen::VpnFile {
                                    name: f["name"].as_str()?.to_string(),
                                    path: f["path"].as_str()?.to_string(),
                                })
                            })
                            .take(crate::vpn_screen::MAX_FILES)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (Purpose::VpnImport, ok) => {
                if ok {
                    self.toast("VPN profile imported", Tone::Info);
                    out.cmd(Cmd::VpnFiles);
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Import failed".into());
                    self.toast(msg, Tone::Danger);
                }
            }
            (Purpose::Gps, false) => {
                let msg = error
                    .map(friendly)
                    .unwrap_or_else(|| "GPS request failed".into());
                self.toast(msg, Tone::Danger);
            }
            (Purpose::Vpn, false) => {
                let msg = error
                    .map(friendly)
                    .unwrap_or_else(|| "VPN request failed".into());
                self.toast(msg, Tone::Danger);
            }
            (Purpose::VpnSheet, ok) => {
                let sheet = matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_vpn());
                if ok {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast("Saved", Tone::Info);
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    self.fail_sheet_or_toast(sheet, msg);
                }
            }
            (Purpose::Setting, ok) => {
                // the core wrote (or refused) the file: show what is really configured
                out.refresh = true;
                if let (false, Some(e)) = (ok, error) {
                    self.toast(friendly(e), Tone::Danger);
                }
            }
            (Purpose::Close, true) => {
                if data["closed"] == false {
                    self.toast("Could not close the app", Tone::Warning);
                }
            }
            (Purpose::Notifications, true) => {
                self.model.notifications = data["notifications"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (_, false) => {
                if let Some(e) = error {
                    if e.code != "busy" {
                        self.toast(friendly(e), Tone::Danger);
                    }
                }
            }
            _ => {}
        }
    }
}
