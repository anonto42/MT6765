//! Core IPC command dispatcher for `Daemon` (device, windows, apps, lock, system).

use crate::brain::Brain;
use crate::lock::Unlock;
use crate::logic::screen_timeout as st;
use serde_json::json;
use std::time::Instant;
use walpad_proto::{Command, ErrorCode, PowerAction, ScreenMode, ScreenState};

use super::errors::{lock_err, wall_ms, CmdResult};
use super::{Daemon, SHELL_MENU_MAX_MS};

impl Daemon {
    pub fn execute(&mut self, cmd: Command) -> CmdResult {
        let now = self.now_ms();
        match cmd {
            Command::GetStatus => Ok(serde_json::to_value(self.status()).unwrap_or_default()),
            Command::Screen { mode } => {
                let a = self.brain.screen.screen(mode, now);
                let mut fx = Vec::new();
                Brain::screen_effects(a, &mut fx);
                self.apply(fx, true);
                // switching the screen off by hand locks at once (like the short power press)
                if mode != ScreenMode::On && self.brain.screen.state() == ScreenState::Off {
                    self.lock.lock_now();
                }
                Ok(
                    json!({"screen": self.brain.screen.state(), "brightness": self.brain.screen.brightness()}),
                )
            }
            Command::SetBrightness(args) => {
                let req = match (args.value, args.delta) {
                    (Some(v), _) => st::Brightness::Absolute(v),
                    (_, Some(d)) => st::Brightness::Delta(d),
                    _ => return Err((ErrorCode::InvalidArgs, "value or delta required".into())),
                };
                let a = self.brain.screen.brightness_cmd(req);
                let mut fx = Vec::new();
                Brain::screen_effects(a, &mut fx);
                self.apply(fx, true);
                Ok(json!({"brightness": self.brain.screen.brightness()}))
            }
            Command::VolumeSet { percent } => {
                let a = self.volume.set_volume(percent, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::VolumeDelta { delta } => {
                let a = self.volume.delta(delta, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::Mute { on } => {
                let a = self.volume.set_mute(on, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::VolumeTest => match self.volume.test_tone(Instant::now()) {
                Ok(true) => Ok(json!({"playing": true})),
                Ok(false) => Err((ErrorCode::Busy, "the test tone is still playing".into())),
                Err(m) => Err((ErrorCode::Internal, m)),
            },
            Command::JackForce { route } => {
                let st = self.jack.set_force(route);
                Ok(json!(st))
            }
            Command::Rotate { mode } => {
                let a = self.brain.rotation.command(mode);
                let mut fx = Vec::new();
                Brain::rotation_effects(a, &mut fx);
                self.apply(fx, true);
                Ok(
                    json!({"orientation": self.brain.rotation.orientation(), "rotation_locked": self.brain.rotation.locked()}),
                )
            }
            Command::Keyboard { mode } => Ok(json!({"keyboard": self.keyboard(mode)})),
            Command::Home => {
                let ok = self
                    .x_try(|b| b.focus_title("WalPad Home"))
                    .unwrap_or(false);
                Ok(json!({"focused": ok}))
            }
            Command::Back => {
                let sent = self.x_try(|b| b.back());
                Ok(json!({"sent": sent}))
            }
            Command::Launch { app, args } => self
                .launch(&app, args.as_ref().map(|a| a.path.as_str()))
                .map(|how| json!({"result": how})),
            Command::CloseApp { app } => {
                if self.apps.def(&app).is_none() {
                    return Err((ErrorCode::InvalidArgs, format!("unknown app {app:?}")));
                }
                let how = self.close_app(&app);
                Ok(json!({"closed": how.is_some(), "how": how}))
            }
            Command::NewInstance { app } => self.new_instance(&app),
            Command::Windows => self.windows_cmd(),
            Command::FocusWindow { window } => self.focus_window(window),
            Command::CloseWindow { window } => self.close_window_cmd(window),
            Command::WindowRect { window, x, y, w, h } => {
                self.window_rect_cmd(window, walpad_proto::wm::WRect::new(x, y, w, h))
            }
            Command::Snap { window, zone } => self.snap_cmd(window, zone),
            Command::Layout { mode, ratio, swap } => self.layout_cmd(mode, ratio, swap),
            Command::RestartShell => Ok(json!({"terminated": self.terminate("walpad-shell")})),
            Command::Screenshot => self.screenshot(),
            Command::SetSetting { key, value } => self.set_setting(&key, value),
            Command::SetTime { datetime } => self.set_time(&datetime),
            Command::OpenMap { lat, lon } => self.open_map(lat, lon),
            Command::Power {
                confirmed: false, ..
            } => Err((ErrorCode::Refused, "power requires confirmed: true".into())),
            Command::Power { action, .. } => {
                self.halt(match action {
                    PowerAction::Poweroff => "poweroff",
                    PowerAction::Reboot => "reboot",
                });
                Ok(json!({"action": action}))
            }
            Command::StatsConfig { cpu, ram, rom, net } => {
                let it = self
                    .stats
                    .configure(cpu, ram, rom, net, Instant::now())
                    .map_err(|m| (ErrorCode::Internal, m))?;
                Ok(json!({"items": it}))
            }
            Command::PowerMenu { open } => {
                self.shell_menu_until = open.then(|| now + SHELL_MENU_MAX_MS);
                Ok(json!({"open": open}))
            }
            Command::LockSet {
                kind,
                secret,
                current,
            } => self
                .lock
                .set(
                    kind,
                    secret.expose(),
                    current.as_ref().map(|c| c.expose()),
                    wall_ms(),
                )
                .map(|_| {
                    eprintln!("walpad-core: screen lock set ({kind:?})");
                    json!({"kind": kind})
                })
                .map_err(lock_err),
            Command::LockRemove { current } => self
                .lock
                .remove(current.expose(), wall_ms())
                .map(|_| {
                    eprintln!("walpad-core: screen lock removed");
                    json!({})
                })
                .map_err(lock_err),
            Command::Unlock { secret } => Ok(match self.lock.unlock(secret.expose(), wall_ms()) {
                Unlock::Unlocked => json!({"unlocked": true}),
                Unlock::Wrong { attempts_left } => {
                    json!({"unlocked": false, "attempts_left": attempts_left})
                }
                Unlock::Wait { retry_in_s } => {
                    json!({"unlocked": false, "retry_in_s": retry_in_s})
                }
            }),
            Command::LockNow => {
                Ok(json!({"locked": self.lock.lock_now(), "configured": self.lock.configured()}))
            }
            Command::LockConfig { auto_lock_s } => self
                .lock
                .set_auto_lock(auto_lock_s)
                .map(|_| json!({"auto_lock_s": auto_lock_s}))
                .map_err(lock_err),
            Command::Hello { .. } | Command::Subscribe { .. } => Ok(json!({})),
            other => self.execute_net_phone(other),
        }
    }
}
