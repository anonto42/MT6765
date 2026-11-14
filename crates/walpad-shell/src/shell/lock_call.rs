use super::Shell;
use crate::call_screen::CallAct;
use crate::call_window::CallWindow;
use crate::lock_window::{LockEvent, LockWindow};
use walpad_proto::LockKind;

impl Shell {
    /// Bring the lock window in line with `lock_wanted`.
    pub(super) fn sync_lock(&mut self) {
        let now = self.now_ms();
        if self.lock.is_none() && self.lock_wanted && now >= self.lock_reopen_at {
            self.lock_reopen_at = now + 1000;
            let wall = self.app.model.wall.clone();
            match LockWindow::open(&self.sock, self.win.size, wall) {
                Ok(l) => self.lock = Some(l),
                Err(e) => eprintln!("walpad-shell: lock screen unavailable: {}", e.0),
            }
        }
        let Some(mut w) = self.lock.take() else {
            return;
        };
        let (kind, retry) = (self.lock_kind, self.lock_retry_in_s);
        let r = if !self.lock_wanted {
            self.lock_pending = None;
            w.hide()
        } else if !w.visible() {
            // the strips of the shell go away with the app they belong to
            self.set_keyboard(false, true);
            self.set_pad(false);
            w.show(kind, retry, now, &mut self.fonts)
        } else {
            w.sync_retry(retry, &mut self.fonts)
        };
        match r {
            Ok(()) => self.lock = Some(w),
            Err(e) => {
                eprintln!("walpad-shell: lock screen error: {}", e.0);
                self.lock_reopen_at = now + 1000;
            }
        }
    }

    pub(super) fn on_lock_status(&mut self, l: &walpad_proto::LockStatus) {
        self.lock_wanted = l.locked;
        self.lock_kind = l.kind.unwrap_or(LockKind::Pin);
        self.lock_retry_in_s = l.retry_in_s;
        self.sync_lock();
    }

    pub(super) fn on_lock_event(&mut self, e: LockEvent) {
        let LockEvent::Submit(secret) = e;
        // the secret goes onto the control socket and nowhere else (never a log, never argv)
        match self
            .core
            .send("unlock", serde_json::json!({ "secret": secret }))
        {
            Some(id) => self.lock_pending = Some(id),
            None => {
                if let Some(l) = self.lock.as_mut() {
                    let _ = l.failed("The system service is not responding", &mut self.fonts);
                }
            }
        }
    }

    /// The core's answer to `unlock`.
    pub(super) fn on_unlock_reply(
        &mut self,
        ok: bool,
        data: &serde_json::Value,
        msg: Option<&str>,
    ) {
        let Some(mut l) = self.lock.take() else {
            return;
        };
        let r = if ok && data["unlocked"] == true {
            // the status event follows; go back to the app at once
            self.lock_wanted = false;
            self.lock_retry_in_s = None;
            l.hide()
        } else if ok {
            l.rejected(
                data["attempts_left"].as_u64().map(|n| n as u32),
                data["retry_in_s"].as_u64().map(|n| n as u32),
                &mut self.fonts,
            )
        } else {
            l.failed(msg.unwrap_or("Could not check the secret"), &mut self.fonts)
        };
        match r {
            Ok(()) => self.lock = Some(l),
            Err(e) => eprintln!("walpad-shell: lock screen error: {}", e.0),
        }
    }

    /// A call is on (ringing, dialling, running, or just ended): the call screen is wanted.
    pub(super) fn call_wanted(&self) -> bool {
        self.call_status.call != walpad_proto::CallState::Idle
    }

    /// Bring the call window in line with the last phone status, and pause / resume the lock
    /// screen under it.
    pub(super) fn sync_call(&mut self) {
        let now = self.now_ms();
        let wanted = self.call_wanted();
        if self.call.is_none() && wanted && now >= self.call_reopen_at {
            self.call_reopen_at = now + 1000;
            match CallWindow::open(&self.sock, self.win.size) {
                Ok(c) => self.call = Some(c),
                Err(e) => eprintln!("walpad-shell: call screen unavailable: {}", e.0),
            }
        }
        let was_visible = self.call.as_ref().is_some_and(|c| c.visible());
        if let Some(mut c) = self.call.take() {
            let r = if wanted {
                if !was_visible {
                    // the strips of the shell go away: the call screen is the only thing on top
                    self.set_keyboard(false, true);
                    self.set_pad(false);
                }
                // the lock screen steps aside before the call screen grabs the pointer
                if let Some(l) = self.lock.as_mut() {
                    if let Err(e) = l.set_paused(true) {
                        eprintln!("walpad-shell: lock screen: {}", e.0);
                    }
                }
                c.update(&self.call_status, now, &mut self.fonts)
            } else {
                let r = c.update(&self.call_status, now, &mut self.fonts);
                if let Some(l) = self.lock.as_mut() {
                    if let Err(e) = l.set_paused(false) {
                        eprintln!("walpad-shell: lock screen: {}", e.0);
                    }
                }
                r
            };
            match r {
                Ok(()) => self.call = Some(c),
                Err(e) => {
                    eprintln!("walpad-shell: call screen error: {}", e.0);
                    self.call_reopen_at = now + 1000;
                    if let Some(l) = self.lock.as_mut() {
                        let _ = l.set_paused(false);
                    }
                }
            }
        }
    }

    pub(super) fn on_phone_status(&mut self, st: &walpad_proto::PhoneStatus) {
        self.call_status = st.clone();
        self.sync_call();
    }

    /// What the owner tapped on the call screen goes to the core (never logged: no number in it).
    pub(super) fn on_call_act(&mut self, a: CallAct) {
        let (name, args) = match a {
            CallAct::Answer => ("phone_answer", serde_json::Value::Null),
            CallAct::Hangup => ("phone_hangup", serde_json::Value::Null),
            CallAct::Mute(on) => ("phone_mute", serde_json::json!({ "on": on })),
            CallAct::Speaker(on) => ("phone_speaker", serde_json::json!({ "on": on })),
            CallAct::Dtmf(c) => ("phone_dtmf", serde_json::json!({ "digit": c.to_string() })),
        };
        let _ = self.core.send(name, args);
    }
}
