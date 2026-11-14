use super::Shell;
use crate::app::WallTime;
use crate::ime::ImEvent;
use crate::window::WinEvent;
use std::time::{Duration, Instant};

impl Shell {
    /// One loop iteration. Returns Err when the X connection is gone (the supervisor restarts us).
    pub fn step(&mut self, max_wait: Duration) -> Result<(), String> {
        let events = self.win.poll_events().map_err(|e| e.0)?;
        for ev in events {
            match ev {
                WinEvent::Input(i) => {
                    let now = self.now_ms();
                    let gs = self.rec.feed(i, now);
                    self.apply_gestures(gs);
                }
                WinEvent::Resize(w, h) => self.resize(w, h),
                WinEvent::Expose => self.dirty = true,
                WinEvent::Key(_) => {}
            }
        }
        let now = self.now_ms();
        let gs = self.rec.tick(now);
        self.apply_gestures(gs);

        self.poll_overlays(now);
        self.poll_core_events(now);

        self.dirty |= self.app.tick(now);
        for ev in self.media.poll() {
            self.dirty |= self.app.on_media(ev);
        }
        let queued = self.app.take_cmds();
        self.send(queued);
        if Instant::now() >= self.next_minute {
            let (wall, to_minute) = WallTime::now(self.clock_24h);
            self.next_minute = Instant::now() + Duration::from_millis(to_minute);
            if let Some(l) = self.lock.as_mut() {
                if let Err(e) = l.set_wall(wall.clone(), &mut self.fonts) {
                    eprintln!("walpad-shell: lock screen: {}", e.0);
                }
            }
            self.dirty |= self.app.set_wall(wall);
            // uptime and memory change: refresh the facts once a minute while Settings/Device is open
            self.need_refresh |= matches!(
                self.app.active,
                walpad_ui::components::Tab::Settings | walpad_ui::components::Tab::Device
            );
        }
        if self.need_refresh {
            self.reload();
        }
        if self.dirty && !self.screen_off {
            self.redraw()?;
        }

        let wait = self.next_wait_ms(max_wait, now);
        self.wait_for_events(wait);
        Ok(())
    }

    fn poll_overlays(&mut self, now: u64) {
        if self.lock_wanted && self.lock.is_none() {
            self.sync_lock();
        }
        if let Some(mut l) = self.lock.take() {
            match l.poll(now, &mut self.fonts) {
                Ok(evs) => {
                    self.lock = Some(l);
                    for e in evs {
                        self.on_lock_event(e);
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: lock screen error: {}", e.0);
                    self.lock_reopen_at = now + 1000;
                }
            }
        }
        if self.call_wanted() && self.call.is_none() {
            self.sync_call();
        }
        if let Some(mut c) = self.call.take() {
            match c.poll(now, &mut self.fonts) {
                Ok(acts) => {
                    self.call = Some(c);
                    for a in acts {
                        self.on_call_act(a);
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: call screen error: {}", e.0);
                    self.call_reopen_at = now + 1000;
                    if let Some(l) = self.lock.as_mut() {
                        let _ = l.set_paused(false);
                    }
                }
            }
        }
        if let Some(mut d) = self.dock.take() {
            match d.poll(&mut self.fonts) {
                Ok(cmds) => {
                    self.dock = Some(d);
                    self.send(cmds);
                }
                Err(e) => eprintln!("walpad-shell: system dock disabled: {}", e.0),
            }
        }
        if let Some(mut b) = self.wmbar.take() {
            match b.poll(&mut self.fonts) {
                Ok(cmds) => {
                    self.wmbar = Some(b);
                    self.send(cmds);
                }
                Err(e) => eprintln!("walpad-shell: window bar disabled: {}", e.0),
            }
        }
        // the edge zone only exists while the bottom edge is free of keyboard, pad and dock strip
        let blocked = self.keyboard_visible()
            || self.mouse_pad_visible()
            || self.dock.as_ref().is_some_and(|d| d.expanded());
        if let Some(mut n) = self.nav.take() {
            let res = n
                .set_blocked(blocked)
                .and_then(|()| n.poll(&mut self.fonts));
            match res {
                Ok(out) => {
                    self.nav = Some(n);
                    for t in out.notices {
                        self.app.notice(t);
                        self.dirty = true;
                    }
                    self.send(out.cmds);
                }
                Err(e) => eprintln!("walpad-shell: edge swipe / Recents disabled: {}", e.0),
            }
        }
        if let Some(mut k) = self.kbd.take() {
            match k.poll(&mut self.fonts) {
                Ok(hid) => {
                    self.kbd = Some(k);
                    if hid {
                        // minimise key (down until the next focus-in), or the user went Home
                        self.focus.manual(false);
                        self.bottom_changed();
                        self.report_keyboard();
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: system keyboard disabled: {}", e.0);
                    self.bottom_changed();
                }
            }
        }
        if let Some(mut p) = self.pad.take() {
            match p.poll(&mut self.fonts) {
                Ok(hid) => {
                    self.pad = Some(p);
                    if hid {
                        // Close button, or the user went Home
                        self.bottom_changed();
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: mouse pad disabled: {}", e.0);
                    self.bottom_changed();
                }
            }
        }
        if let Some(mut s) = self.ime.take() {
            match s.poll() {
                Ok(evs) => {
                    self.ime = Some(s);
                    let now = self.now_ms();
                    for ev in evs {
                        match ev {
                            ImEvent::Connected => self.focus.client_connected(),
                            ImEvent::FocusIn(ic) => self.focus.focus_in(ic, now),
                            ImEvent::FocusOut(ic) | ImEvent::Destroyed(ic) => {
                                self.focus.focus_out(ic, now);
                            }
                            ImEvent::ClientGone(w) => self.focus.client_gone(w, now),
                            ImEvent::ActiveWindow(fallback_app) => {
                                self.focus.active_window_changed(now, fallback_app);
                            }
                        }
                        self.apply_focus(None);
                    }
                }
                Err(e) => eprintln!("walpad-shell: input method (XIM) disabled: {}", e.0),
            }
        }
        let now = self.now_ms();
        let a = self.focus.tick(now);
        self.apply_focus(a);
        self.step_edit();
    }
}
