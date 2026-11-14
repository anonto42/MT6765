//! X11 bridge lifecycle and system keyboard toggling for [`Daemon`].
use super::{Daemon, X_PING_EVERY, X_RETRY};
use crate::xbridge::{XBridge, XError};
use std::time::Instant;
use walpad_proto::{KeyboardMode, ShellInput};

impl Daemon {
    fn x_connect(&mut self) {
        if self.xb.is_some() || Instant::now() < self.x_retry {
            return;
        }
        match XBridge::connect(&self.root.p("/tmp/.X11-unix/X0")) {
            Ok(mut b) => {
                // X (re)started: restore struts and keyboard placement for the current orientation
                let o = self.brain.orientation();
                if let Err(e) = b.set_struts(o).and_then(|_| b.dock_keyboard(o).map(|_| ())) {
                    eprintln!("walpad-core: X resync failed: {}", e.0);
                    self.x_retry = Instant::now() + X_RETRY;
                } else {
                    eprintln!("walpad-core: connected to X");
                    self.xb = Some(b);
                }
            }
            Err(_) => self.x_retry = Instant::now() + X_RETRY,
        }
    }

    /// Run an X operation; on failure drop the bridge (it reconnects with backoff).
    pub(super) fn x_try<T>(
        &mut self,
        f: impl FnOnce(&mut XBridge) -> Result<T, XError>,
    ) -> Option<T> {
        self.x_connect();
        let b = self.xb.as_mut()?;
        match f(b) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("walpad-core: X error: {}", e.0);
                if let Some(mut b) = self.xb.take() {
                    b.release_if_down();
                }
                self.x_retry = Instant::now() + X_RETRY;
                None
            }
        }
    }

    pub(super) fn x_maintain(&mut self) {
        if self.xb.is_none() {
            self.x_connect();
        } else if Instant::now() >= self.x_ping {
            self.x_ping = Instant::now() + X_PING_EVERY;
            self.x_try(|b| b.ping());
        }
    }

    /// The on-screen keyboard is the shell's system keyboard (owner request 2026-11-10: replaces
    /// matchbox-keyboard): ask the shell over the `input` topic. The shell reports its own changes
    /// (dock button, minimise key, Home) with `keyboard show|hide`, so `toggle` resolves against
    /// its real state. "unavailable" when no shell listens.
    pub(super) fn keyboard(&mut self, mode: KeyboardMode) -> &'static str {
        if !self.shell_listening() {
            return "unavailable";
        }
        let show = match mode {
            KeyboardMode::Show => true,
            KeyboardMode::Hide => false,
            KeyboardMode::Toggle => !self.keyboard_shown,
        };
        self.keyboard_shown = show;
        self.publish_input(if show {
            ShellInput::KeyboardShow
        } else {
            ShellInput::KeyboardHide
        });
        if show {
            "shown"
        } else {
            "hidden"
        }
    }
}
