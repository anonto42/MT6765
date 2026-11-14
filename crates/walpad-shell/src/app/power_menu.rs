//! Power menu lifecycle, hardware key routing, and overlay dismissal for `App`.

use crate::overlay::{self, Overlay};
use walpad_proto::ShellInput;

use super::cmd::{Cmd, Outcome};
use super::model::{POWER_ARM_MS, POWER_MENU_MS};
use super::App;

impl App {
    /// Open the power menu with Cancel focused and tell the core to route the hardware keys here.
    pub(super) fn open_power_menu(&mut self, out: &mut Outcome) {
        if !self.model.connected {
            return;
        }
        self.close_overlay();
        self.model.overlay = Some(Overlay::PowerMenu {
            focus: overlay::POWER_CANCEL,
            armed: None,
            until: self.now + POWER_MENU_MS,
        });
        out.cmd(Cmd::PowerMenuState(true));
        out.dirty = true;
    }

    /// Close the menu from the clock (no `Outcome` to put the command in).
    pub(super) fn close_power_menu(&mut self) {
        if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
            self.model.overlay = None;
            self.queued.push(Cmd::PowerMenuState(false));
        }
    }

    /// Hardware keys from the core: only the power menu uses them.
    pub(super) fn on_hw_key(&mut self, k: ShellInput, out: &mut Outcome) {
        if k == ShellInput::PowerMenuOpen {
            if !matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
                self.open_power_menu(out);
            }
            return;
        }
        let Some(Overlay::PowerMenu { focus, .. }) = &mut self.model.overlay else {
            return;
        };
        let n = overlay::POWER_ITEMS.len();
        match k {
            ShellInput::VolumeDown => *focus = (*focus + 1) % n,
            ShellInput::VolumeUp => *focus = (*focus + n - 1) % n,
            ShellInput::PowerShort => {
                let i = *focus;
                self.power_item(i, out);
            }
            // the system keyboard is the shell loop's (keyboard.rs)
            ShellInput::PowerMenuOpen | ShellInput::KeyboardShow | ShellInput::KeyboardHide => {
                return
            }
        }
        out.dirty = true;
    }

    /// Activate a power menu item: Screen off and Cancel act at once, Restart and Shut down need
    /// a second tap within 3 s.
    pub(super) fn power_item(&mut self, i: usize, out: &mut Outcome) {
        let Some(Overlay::PowerMenu { focus, armed, .. }) = &mut self.model.overlay else {
            return;
        };
        let Some((_, two_step)) = overlay::POWER_ITEMS.get(i).copied() else {
            return;
        };
        *focus = i;
        out.dirty = true;
        if two_step && !armed.is_some_and(|(a, t)| a == i && self.now < t) {
            *armed = Some((i, self.now + POWER_ARM_MS));
            return;
        }
        self.model.overlay = None;
        out.cmd(Cmd::PowerMenuState(false));
        match i {
            0 if self.model.connected => out.cmd(Cmd::ScreenOff),
            1 | 2 if self.model.connected => out.cmd(Cmd::Power { poweroff: i == 2 }),
            _ => {}
        }
    }

    pub(super) fn close_overlay(&mut self) {
        if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
            self.close_power_menu();
            return;
        }
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            s.secret.clear();
        }
        self.model.net.connecting = None;
        self.model.lock_flow = None;
        self.model.phone.draft = None;
        self.model.overlay = None;
    }
}
