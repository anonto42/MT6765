//! Tap handlers for Device, Settings, and App Store screens.

use crate::device_screen;
use crate::overlay::{Overlay, PasswordSheet, SheetPurpose};
use crate::settings_screen::Page;
use crate::store::{self, StoreTarget, StoreView};
use serde_json::json;
use walpad_proto::StatsItems;
use walpad_ui::components::{Tab, Tone};

use super::cmd::{Cmd, Outcome};
use super::target::Target;
use super::App;

impl App {
    /// Handle Device, Settings, and Store targets. Returns `true` if `t` belonged to them.
    pub(super) fn on_device_tap(
        &mut self,
        t: Target,
        interactive: bool,
        out: &mut Outcome,
    ) -> bool {
        match t {
            Target::Rot(i) if interactive => {
                if let Some((_, mode)) = device_screen::ROTATION_ROWS.get(i) {
                    out.cmd(Cmd::Rotate(mode));
                }
            }
            Target::Dim(i) if interactive => {
                if let Some((v, _)) = device_screen::DIM_OPTIONS.get(i) {
                    out.cmd(Cmd::Setting("dim_after_s", json!(v)));
                }
            }
            Target::Off(i) if interactive => {
                if let Some((v, _)) = device_screen::OFF_OPTIONS.get(i) {
                    out.cmd(Cmd::Setting("off_after_s", json!(v)));
                }
            }
            Target::LidToggle if interactive => {
                out.cmd(Cmd::Setting("lid_off", json!(!self.model.devd.lid_off)))
            }
            Target::ThemeToggle if interactive => {
                let next = walpad_ui::tokens::theme().toggle();
                walpad_ui::tokens::set_theme(next);
                out.cmd(Cmd::Setting("theme", json!(next.as_str())));
            }
            Target::Wallpaper(i) if interactive => {
                if let Some(&kind) = walpad_ui::art::WallpaperKind::ALL.get(i) {
                    if kind == walpad_ui::art::WallpaperKind::Custom
                        && self.model.wallpaper_path.is_empty()
                    {
                        self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                            SheetPurpose::WallpaperPath,
                            &self.model.wallpaper_path,
                        )));
                    } else {
                        self.model.wallpaper = kind;
                        out.cmd(Cmd::Setting("wallpaper", json!(kind.as_str())));
                    }
                }
            }
            Target::WallpaperPathEdit if interactive => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::WallpaperPath,
                    &self.model.wallpaper_path,
                )));
            }
            Target::RotateFlipToggle if interactive => out.cmd(Cmd::Setting(
                "rotate_flip",
                json!(!self.model.devd.rotate_flip),
            )),
            Target::PowerDimCycle if interactive => {
                let opts = device_screen::DIM_OPTIONS;
                let idx =
                    device_screen::selected_option(&opts, self.model.devd.dim_after_s).unwrap_or(0);
                let next = opts[(idx + 1) % opts.len()].0;
                out.cmd(Cmd::Setting("dim_after_s", json!(next)));
            }
            Target::PowerDimLevelCycle if interactive => {
                let next = device_screen::next_u32(
                    &device_screen::DIM_LEVEL_OPTIONS,
                    self.model.devd.dim_level,
                );
                out.cmd(Cmd::Setting("dim_level", json!(next)));
            }
            Target::PowerOffCycle if interactive => {
                let opts = device_screen::OFF_OPTIONS;
                let idx =
                    device_screen::selected_option(&opts, self.model.devd.off_after_s).unwrap_or(0);
                let next = opts[(idx + 1) % opts.len()].0;
                out.cmd(Cmd::Setting("off_after_s", json!(next)));
            }
            Target::LowBattCycle if interactive => {
                let next = device_screen::next_u32(
                    &device_screen::LOW_BATT_OPTIONS,
                    self.model.devd.low_batt_mv,
                );
                out.cmd(Cmd::Setting("low_batt_mv", json!(next)));
            }
            Target::CritBattCycle if interactive => {
                let next = device_screen::next_u32(
                    &device_screen::CRIT_BATT_OPTIONS,
                    self.model.devd.critical_batt_mv,
                );
                out.cmd(Cmd::Setting("critical_batt_mv", json!(next)));
            }
            Target::ChargeModeCycle if interactive => {
                let modes = crate::config::CHARGE_MODES;
                let idx = modes
                    .iter()
                    .position(|&m| m == self.model.devd.charge_mode)
                    .unwrap_or(0);
                let next = modes[(idx + 1) % modes.len()];
                out.cmd(Cmd::Setting("charge_mode", json!(next)));
            }
            Target::ChargeLimitCycle if interactive => {
                let next = device_screen::next_u32(
                    &crate::config::CHARGE_LIMIT_OPTIONS,
                    self.model.devd.charge_limit_pct,
                );
                out.cmd(Cmd::Setting("charge_limit_pct", json!(next)));
            }
            Target::ChargeCurrentCycle if interactive => {
                let next = device_screen::next_u32(
                    &crate::config::CHARGE_CURRENT_OPTIONS,
                    self.model.devd.charge_current_ma,
                );
                out.cmd(Cmd::Setting("charge_current_ma", json!(next)));
            }
            Target::TouchGesturesToggle if interactive => out.cmd(Cmd::Setting(
                "touch_gestures",
                json!(!self.model.devd.touch_gestures),
            )),
            Target::MultitouchToggle if interactive => out.cmd(Cmd::Setting(
                "multitouch",
                json!(!self.model.devd.multitouch),
            )),
            Target::KeyGesture(i) if interactive => {
                if let Some((key, label, def_act)) = crate::config::KEY_SLOTS.get(i) {
                    let cur = self
                        .model
                        .sys
                        .gestures
                        .iter()
                        .find(|(l, _)| l == label)
                        .map(|(_, a)| a.as_str())
                        .unwrap_or(def_act);
                    let next = crate::config::next_key_action(cur);
                    out.cmd(Cmd::Setting(key, json!(next)));
                }
            }
            Target::Page(Page::Display) => self.go(Tab::Device, out),
            Target::Page(pg) => {
                self.model.settings_page = Some(pg);
                if pg == Page::Apps {
                    self.model.store.view = StoreView::Installed;
                    out.cmd(Cmd::PkgCatalogue);
                }
                self.scroll = 0;
                self.content_hits.clear();
                out.refresh = true;
            }
            Target::Back
                if self.model.settings_page == Some(Page::Apps)
                    && self.model.store.view != StoreView::Installed =>
            {
                self.model.store.view = StoreView::Installed;
                self.scroll = 0;
                self.content_hits.clear();
            }
            Target::Store(t) if interactive => self.on_store_tap(t, out),
            Target::Back if self.active == Tab::Gps => {
                self.model.settings_page = None;
                self.go(Tab::Settings, out);
            }
            Target::Back => {
                self.model.settings_page = match self.model.settings_page {
                    Some(Page::Logs) => Some(Page::Maintenance),
                    _ => None,
                };
                self.scroll = 0;
                self.content_hits.clear();
            }
            Target::Stat(i) if interactive => {
                let on = self
                    .model
                    .status
                    .as_ref()
                    .map_or_else(StatsItems::default, |s| s.stats.items);
                out.cmd(Cmd::Stats(i, !i.on(&on)))
            }
            Target::WindowsMode | Target::WindowsRatio | Target::WindowsSwap if interactive => {
                if let Some((p, c)) = crate::windows_ui::tap(&self.model.windows, t) {
                    self.model.windows = p;
                    out.cmd(c);
                }
            }
            Target::ClockToggle if interactive => {
                out.cmd(Cmd::Setting("clock_24h", json!(!self.model.clock_24h)))
            }
            Target::ClockSetTime if interactive => {
                let now = jiff::Zoned::now().strftime("%Y-%m-%d %H:%M").to_string();
                self.model.overlay = Some(Overlay::Password(PasswordSheet::net_config(
                    SheetPurpose::ClockTime,
                    &now,
                )));
            }
            Target::RestartShell if interactive => {
                out.cmd(Cmd::RestartShell);
                self.toast("Restarting the shell", Tone::Info);
            }
            Target::Reboot => {
                self.model.overlay = Some(Overlay::PowerConfirm {
                    poweroff: false,
                    armed_until: None,
                })
            }
            Target::Shutdown => {
                self.model.overlay = Some(Overlay::PowerConfirm {
                    poweroff: true,
                    armed_until: None,
                })
            }
            _ => return false,
        }
        true
    }

    pub(super) fn on_store_tap(&mut self, t: StoreTarget, out: &mut Outcome) {
        let st = &mut self.model.store;
        match t {
            StoreTarget::DismissError => st.error = None,
            StoreTarget::Browse => {
                st.view = StoreView::Browse;
                self.scroll = 0;
                self.content_hits.clear();
            }
            StoreTarget::Role(i) => {
                st.view = StoreView::Role(i);
                self.scroll = 0;
                self.content_hits.clear();
            }
            StoreTarget::PickDefault(i) => {
                if let (StoreView::Role(ri), Some(e)) = (st.view, st.entries.get(i)) {
                    if let Some(role) = walpad_proto::DEFAULT_ROLES.get(ri) {
                        out.cmd(Cmd::DefaultApp {
                            role: role.to_string(),
                            app: e.id.clone(),
                        });
                    }
                }
            }
            StoreTarget::Install(i) => {
                let Some(e) = st.entries.get(i).cloned() else {
                    return;
                };
                if st.busy.is_some() {
                    return;
                }
                if let Some(why) = st.space_problem(&e) {
                    // nothing is sent: explain it here, nothing changes
                    st.error = Some(format!("{}: {why}", e.name));
                    return;
                }
                st.error = None;
                st.busy = Some(store::Busy {
                    op: "install".into(),
                    id: e.id.clone(),
                    percent: 0,
                    text: "Starting the installation".into(),
                });
                out.cmd(Cmd::PkgInstall(e.id));
            }
            StoreTarget::Remove(i) => {
                let Some(e) = st.entries.get(i).cloned() else {
                    return;
                };
                if st.busy.is_some() || e.system {
                    return;
                }
                if st.armed != Some(i) {
                    st.armed = Some(i);
                    return;
                }
                st.armed = None;
                st.error = None;
                st.busy = Some(store::Busy {
                    op: "remove".into(),
                    id: e.id.clone(),
                    percent: 0,
                    text: "Starting the removal".into(),
                });
                out.cmd(Cmd::PkgRemove(e.id));
            }
        }
    }
}
