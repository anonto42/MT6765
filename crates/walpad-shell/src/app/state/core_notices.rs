//! Package, mobile, and Bluetooth notice handlers for `App`.

use crate::bluetooth::PairState;
use crate::overlay::{Overlay, SheetPurpose};
use crate::store;
use serde_json::Value;
use walpad_ui::components::Tone;

use super::cmd::{Cmd, Outcome};
use super::App;

impl App {
    /// Progress and result of an install or uninstall (change app-catalogue-manager).
    pub(super) fn on_pkg_notice(&mut self, v: &Value, out: &mut Outcome) {
        out.dirty = true;
        let id = v["id"].as_str().unwrap_or("").to_string();
        let op = v["op"].as_str().unwrap_or("install").to_string();
        match v["kind"].as_str() {
            Some("pkg_progress") => {
                self.model.store.busy = Some(store::Busy {
                    op,
                    id,
                    percent: v["percent"].as_u64().unwrap_or(0).min(100) as u8,
                    text: v["text"].as_str().unwrap_or("").to_string(),
                });
            }
            Some("pkg_done") => {
                let name = self.model.store.name_of(&id);
                self.model.store.busy = None;
                if v["ok"] == true {
                    self.model.store.error = None;
                    let verb = if op == "remove" {
                        "uninstalled"
                    } else {
                        "installed"
                    };
                    self.toast(format!("{name} {verb}"), Tone::Info);
                } else {
                    let why = v["error"].as_str().unwrap_or("unknown error");
                    self.model.store.error = Some(format!("{name}: {why}"));
                    self.toast(
                        format!(
                            "Could not {} {name}",
                            if op == "remove" {
                                "uninstall"
                            } else {
                                "install"
                            }
                        ),
                        Tone::Danger,
                    );
                }
                out.cmd(Cmd::PkgCatalogue);
                // the app list changed: re-read apps.json / apps.d
                out.refresh = true;
            }
            _ => {}
        }
    }

    /// Results of radio / PIN / data requests (owner request 2026-11-10).
    pub(super) fn on_mobile_notice(&mut self, v: &Value, _out: &mut Outcome) {
        _out.dirty = true;
        let err = v["error"].as_str().unwrap_or("unknown error").to_string();
        match v["kind"].as_str() {
            Some("mobile_pin") => {
                let sheet = matches!(&self.model.overlay,
                    Some(Overlay::Password(s)) if s.purpose == SheetPurpose::SimPin);
                if v["ok"] == true {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast("SIM unlocked", Tone::Info);
                } else if let (true, Some(Overlay::Password(s))) = (sheet, &mut self.model.overlay)
                {
                    s.connecting = false;
                    s.secret.clear();
                    s.error = Some(err);
                } else {
                    self.toast(format!("SIM PIN: {err}"), Tone::Danger);
                }
            }
            Some("mobile_radio") if v["ok"] != true => {
                self.toast(format!("Mobile radio: {err}"), Tone::Danger)
            }
            Some("mobile_data") => match v["state"].as_str() {
                Some("on") => self.toast("Mobile data on", Tone::Info),
                Some("failed") => {
                    let why = v["reason"].as_str().unwrap_or("unknown error");
                    self.toast(format!("Mobile data failed: {why}"), Tone::Danger)
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn bt_name(&self, address: &str) -> String {
        self.model
            .status
            .as_ref()
            .and_then(|s| s.bluetooth.devices.iter().find(|d| d.address == address))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| address.to_string())
    }

    /// Pairing and connect results (owner request 2026-11-10).
    pub(super) fn on_bt_notice(&mut self, v: &Value, _out: &mut Outcome) {
        _out.dirty = true;
        let addr = v["address"].as_str().unwrap_or("").to_string();
        let name = self.bt_name(&addr);
        let err = v["error"].as_str().unwrap_or("unknown error").to_string();
        match v["kind"].as_str() {
            Some("bluetooth_pair") => {
                let mine = matches!(&self.model.overlay,
                    Some(Overlay::BtPair { address, .. }) if *address == addr);
                let set = |s: &mut Self, state: PairState| {
                    s.model.overlay = Some(Overlay::BtPair {
                        address: addr.clone(),
                        name: name.clone(),
                        state,
                    })
                };
                let code = v["passkey"].as_str().unwrap_or("").to_string();
                match v["state"].as_str() {
                    Some("confirm") => set(self, PairState::Confirm(code)),
                    Some("passkey") => set(self, PairState::Display(code)),
                    Some("ok") => {
                        self.model.bt_busy = None;
                        if mine {
                            self.model.overlay = None;
                        }
                        self.toast(format!("Paired with {name}"), Tone::Info);
                    }
                    Some("failed") => {
                        self.model.bt_busy = None;
                        if mine {
                            set(self, PairState::Failed(err));
                        } else {
                            self.toast(format!("Pairing failed: {err}"), Tone::Danger);
                        }
                    }
                    _ => {}
                }
            }
            Some("bluetooth") => {
                if self
                    .model
                    .bt_busy
                    .as_deref()
                    .is_none_or(|b| b == addr || addr.is_empty())
                {
                    self.model.bt_busy = None;
                }
                let op = v["op"].as_str().unwrap_or("");
                if v["ok"] == true {
                    if op == "connect" {
                        self.toast(format!("Connected to {name}"), Tone::Info);
                    }
                } else {
                    let what = match op {
                        "connect" => format!("Could not connect to {name}"),
                        "disconnect" => format!("Could not disconnect {name}"),
                        "forget" => format!("Could not forget {name}"),
                        _ => "Bluetooth".to_string(),
                    };
                    self.toast(format!("{what}: {err}"), Tone::Danger);
                }
            }
            _ => {}
        }
    }
}
