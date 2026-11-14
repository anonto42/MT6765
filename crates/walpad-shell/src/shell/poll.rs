use super::Shell;
use crate::app::read_clock_24h;
use crate::core_client::CoreEvent;
use std::time::{Duration, Instant};
use walpad_proto::ShellInput;

impl Shell {
    pub(super) fn poll_core_events(&mut self, now: u64) {
        for ev in self.core.tick(Instant::now()) {
            match &ev {
                CoreEvent::Connected => {
                    self.clock_24h = read_clock_24h(&self.root);
                    // a (re)started core learns the keyboard state from us
                    self.report_keyboard();
                }
                CoreEvent::Status(s) => {
                    let was_off = self.screen_off;
                    self.screen_off = s.screen == walpad_proto::ScreenState::Off;
                    if was_off && !self.screen_off {
                        self.dirty = true;
                    }
                    self.on_lock_status(&s.lock);
                    self.on_phone_status(&s.phone);
                }
                // the core is gone: no call can be controlled, the screen must not stay up
                CoreEvent::Disconnected => {
                    self.screen_off = false;
                    self.call_status = Default::default();
                    self.sync_call();
                }
                CoreEvent::Reply {
                    id,
                    ok,
                    data,
                    error,
                } if self.lock_pending == Some(*id) => {
                    self.lock_pending = None;
                    self.on_unlock_reply(*ok, data, error.as_ref().map(|e| e.msg.as_str()));
                }
                CoreEvent::Input(ShellInput::KeyboardShow) => self.set_keyboard(true, false),
                CoreEvent::Input(ShellInput::KeyboardHide) => self.set_keyboard(false, false),
                // a terminal is useless without keys: XIM focus normally brings the keyboard up
                // (st reports its focus); without an XIM connection it comes up as a fallback.
                // Kali and Scanner are the same st+fish shell as "terminal" (owner report
                // 2026-11-13, "from the terminals both kali and the normal terminal" - this used
                // to only match "terminal" by name, so Kali and Scanner never got it at all).
                CoreEvent::Apps(v)
                    if v["kind"] == "started"
                        && matches!(
                            v["app"].as_str(),
                            Some("terminal") | Some("kali") | Some("scanner")
                        ) =>
                {
                    let a = self.focus.app_started(now, self.ime.is_some());
                    self.apply_focus(a);
                }
                _ => {}
            }
            if let CoreEvent::Reply { id, ok, data, .. } = &ev {
                if let Some(i) = self.nav_close.iter().position(|(r, _)| r == id) {
                    let (_, app) = self.nav_close.remove(i);
                    if let Some(n) = &mut self.nav {
                        n.close_result(&app, *ok && data["closed"] != false);
                    }
                }
            }
            let out = self.app.on_core(&ev, now);
            self.dirty |= out.dirty;
            self.need_refresh |= out.refresh;
            self.send(out.cmds);
        }
    }

    pub(super) fn next_wait_ms(&self, max_wait: Duration, now: u64) -> u64 {
        // sleep until the next thing that needs us: input, core traffic, a long-press, the minute
        let mut wait = max_wait.as_millis() as u64;
        if let Some(d) = self.app.next_deadline() {
            wait = wait.min(d.saturating_sub(now));
        }
        if let Some(d) = self.rec.next_deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        if let Some(d) = self.focus.next_deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        if let Some(d) = self.bar.deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        for d in [
            self.keeper.as_ref().and_then(|k| k.next_deadline()),
            self.edit.as_ref().and_then(|e| e.next_deadline()),
        ]
        .into_iter()
        .flatten()
        {
            wait = wait.min(d.saturating_duration_since(Instant::now()).as_millis() as u64);
        }
        if let Some(d) = self.dock.as_ref().and_then(|d| d.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        if let Some(d) = self.wmbar.as_ref().and_then(|b| b.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        if let Some(d) = self.nav.as_ref().and_then(|n| n.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        wait = wait.min(
            self.next_minute
                .saturating_duration_since(Instant::now())
                .as_millis() as u64,
        );
        if let Some(d) = self.lock.as_ref().and_then(|l| l.next_deadline(now)) {
            wait = wait.min(d);
        }
        if let Some(d) = self.call.as_ref().and_then(|c| c.next_deadline(now)) {
            wait = wait.min(d);
        }
        if self.call_wanted() && self.call.is_none() {
            wait = wait.min(500); // the call window is retried
        }
        if self.lock_wanted && self.lock.is_none() {
            wait = wait.min(500); // the lock window is retried
        }
        if !self.core.connected() {
            wait = wait.min(500); // reconnect attempts
        }
        if self.media.busy() {
            wait = wait.min(100); // thumbnails arrive from the background thread
        }
        wait.max(1)
    }

    pub(super) fn wait_for_events(&self, wait_ms: u64) {
        let mut fds = vec![&self.win.xc.wait_fd];
        fds.extend(self.core.fd());
        fds.extend(self.dock.as_ref().map(|d| d.wait_fd()));
        fds.extend(self.wmbar.as_ref().map(|b| b.wait_fd()));
        fds.extend(self.kbd.as_ref().map(|k| k.wait_fd()));
        fds.extend(self.pad.as_ref().map(|p| p.wait_fd()));
        fds.extend(self.nav.as_ref().map(|n| n.wait_fd()));
        fds.extend(self.ime.as_ref().map(|s| s.wait_fd()));
        fds.extend(self.keeper.as_ref().map(|k| k.wait_fd()));
        fds.extend(self.edit.as_ref().map(|e| e.wait_fd()));
        fds.extend(self.lock.as_ref().map(|l| l.wait_fd()));
        fds.extend(self.call.as_ref().map(|c| c.wait_fd()));
        walpad_x::wait_any(&fds, Some(wait_ms));
    }
}
