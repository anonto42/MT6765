//! Top-level tap dispatcher for `App`: navigation, panels, sliders, audio, and overlays.

use crate::chrome::NAV_APPS;
use crate::overlay::Overlay;
use crate::panels;
use crate::settings_screen::Page;
use crate::store::{StoreTarget, StoreView};
use crate::volume_ui;
use walpad_ui::components::{Tab, Tone};

use super::cmd::{Cmd, Outcome};
use super::model::{native_brightness, percent, POWER_ARM_MS};
use super::target::Target;
use super::App;

impl App {
    pub(super) fn on_tap(&mut self, t: Target, x: i32, was_slider: bool, out: &mut Outcome) {
        let interactive = self.model.connected;
        if !matches!(t, Target::Store(StoreTarget::Remove(_))) {
            self.model.store.armed = None;
        }
        if self.on_connectivity_tap(t, interactive, out)
            || self.on_network_tap(t, interactive, out)
            || self.on_device_tap(t, interactive, out)
        {
            return;
        }
        match t {
            Target::Nav(NAV_APPS) => self.model.overlay = Some(Overlay::AppsPanel),
            Target::Nav(i) => self.go(Tab::ALL[i], out),
            Target::QuickSettings => self.model.overlay = Some(Overlay::QuickSettings),
            Target::Dismiss => {
                if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
                    self.model.overlay = None;
                }
            }
            Target::PanelBody => {}
            Target::PanelActive(i) => {
                self.model.overlay = None;
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            // the close badge: the panel stays open (the app leaves "Active apps" with the next
            // status), so several apps can be closed in a row
            Target::PanelClose(i) if interactive => {
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    out.cmd(Cmd::CloseApp(id));
                }
            }
            Target::PanelTool(i) => {
                if let Some(tool) = panels::TOOLS.get(i) {
                    self.model.overlay = None;
                    match tool.action {
                        panels::ToolAction::MousePad => out.cmd(Cmd::MousePadShow),
                        panels::ToolAction::Clipboard => out.cmd(Cmd::ClipboardShow),
                        panels::ToolAction::Media => self.go(Tab::Media, out),
                        panels::ToolAction::Phone => self.go(Tab::Phone, out),
                        panels::ToolAction::AppStore => {
                            self.go(Tab::Settings, out);
                            self.model.settings_page = Some(Page::Apps);
                            self.model.store.view = StoreView::Browse;
                            out.cmd(Cmd::PkgCatalogue);
                        }
                    }
                }
            }
            Target::Media(t) => self.on_media_tap(t, out),
            Target::NotificationsCard => self.go(Tab::Notifications, out),
            Target::NotificationRow(i) if interactive => {
                if let Some(it) = self.model.notifications.get(i) {
                    if !it.read {
                        out.cmd(Cmd::NotificationMarkRead(it.id));
                    }
                }
                if let Some(it) = self.model.notifications.get_mut(i) {
                    it.read = true;
                }
            }
            Target::NotificationsClearAll if interactive => {
                self.model.notifications.clear();
                out.cmd(Cmd::NotificationsClearAll);
            }
            Target::BatteryCard => self.go(Tab::Device, out),
            Target::Tile(i) => {
                if let Some(id) = self.model.tiles.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            Target::AppTile(i) => {
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            Target::Slider if interactive => {
                let v = if was_slider {
                    self.model.slider_value
                } else {
                    self.model.slider_target = Target::Slider;
                    self.slider_value_at(x)
                };
                out.cmd(Cmd::SetBrightness(native_brightness(v)));
            }
            Target::VolumeSlider if volume_ui::usable(&self.model) => {
                let v = if was_slider {
                    self.model.slider_value
                } else {
                    self.model.slider_target = Target::VolumeSlider;
                    self.slider_value_at(x)
                };
                out.cmd(Cmd::SetVolume(percent(v)));
            }
            Target::MuteToggle if volume_ui::usable(&self.model) => {
                out.cmd(Cmd::Mute(!volume_ui::muted(&self.model)))
            }
            Target::TestTone if interactive => out.cmd(Cmd::TestTone),
            Target::JackHeadphones if interactive => {
                let forced = self.model.status.as_ref().and_then(|s| s.jack.forced);
                let next = if forced.is_some() {
                    None
                } else {
                    Some(walpad_proto::JackKind::Headphones)
                };
                out.cmd(Cmd::JackForce(next));
            }
            Target::JackHeadsetMic if interactive => {
                let forced = self.model.status.as_ref().and_then(|s| s.jack.forced);
                let next = if forced == Some(walpad_proto::JackKind::HeadsetWithMic) {
                    Some(walpad_proto::JackKind::Headphones)
                } else {
                    Some(walpad_proto::JackKind::HeadsetWithMic)
                };
                out.cmd(Cmd::JackForce(next));
            }
            Target::Rotation if interactive => out.cmd(Cmd::RotateToggle),
            Target::ScreenOff if interactive => {
                if self
                    .model
                    .screen_off_armed_until
                    .is_some_and(|u| self.now < u)
                {
                    self.model.screen_off_armed_until = None;
                    self.close_overlay();
                    out.cmd(Cmd::ScreenOff);
                } else {
                    self.model.screen_off_armed_until = Some(self.now + POWER_ARM_MS);
                    self.toast("Tap again to turn screen off", Tone::Warning);
                }
            }
            Target::Phone(t) => self.on_phone_tap(t, out),
            Target::PowerMenu => self.open_power_menu(out),
            Target::PowerItem(i) => self.power_item(i, out),
            Target::Lock(a) if interactive => self.on_lock_tap(a, out),
            Target::SheetCancel | Target::DialogCancel => self.close_overlay(),
            Target::DialogConfirm => match self.model.overlay.take() {
                Some(Overlay::Forget { id, .. }) => out.cmd(Cmd::WifiForget(id)),
                Some(Overlay::BtForget { address, .. }) => out.cmd(Cmd::BtForget(address)),
                // two taps: the first arms the button for 3 s ("Tap again to shut down")
                Some(Overlay::PowerConfirm {
                    poweroff,
                    armed_until,
                }) => {
                    if armed_until.is_some_and(|t| self.now < t) {
                        if self.model.connected {
                            out.cmd(Cmd::Power { poweroff });
                        }
                    } else {
                        self.model.overlay = Some(Overlay::PowerConfirm {
                            poweroff,
                            armed_until: Some(self.now + POWER_ARM_MS),
                        });
                    }
                }
                other => self.model.overlay = other,
            },
            Target::Eye => {
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.shown = !s.shown;
                }
            }
            Target::Key(i) => self.on_key(i, out),
            Target::SheetConnect => self.try_connect(out),
            Target::CloseApp => {
                if let Some(Overlay::App(s)) = self.model.overlay.take() {
                    out.cmd(Cmd::CloseApp(s.id));
                }
            }
            Target::PinToggle => {
                if let Some(Overlay::App(s)) = self.model.overlay.take() {
                    if s.pinned {
                        self.model.pinned.retain(|p| p != &s.id);
                    } else if !self.model.pinned.contains(&s.id) {
                        self.model.pinned.push(s.id.clone());
                    }
                    self.rebuild_tiles();
                    out.cmd(Cmd::SetPinned(self.model.pinned.clone()));
                }
            }
            _ => {}
        }
    }
}
