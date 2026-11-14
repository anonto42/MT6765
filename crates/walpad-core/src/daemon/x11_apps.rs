//! App launching, closing, X11 window scanning, and child reaping for [`Daemon`].

use crate::apps::{terminate_pid, Exit, LaunchError};
use crate::xbridge::class_matches;
use serde_json::json;
use std::time::Instant;
use walpad_proto::{ErrorCode, Topic};

use super::{Daemon, APPS_X_EVERY};

/// Close app for an app we did not start: WM_DELETE_WINDOW sent, SIGTERM of its pid at `deadline`
/// if its window is still there.
pub(super) struct ExtClose {
    pub(super) app: String,
    pub(super) deadline: Instant,
}

impl Daemon {
    pub(super) fn launch(
        &mut self,
        id: &str,
        path: Option<&str>,
    ) -> Result<&'static str, (ErrorCode, String)> {
        let Some(def) = self.apps.def(id).cloned() else {
            return Err((ErrorCode::InvalidArgs, format!("unknown app {id:?}")));
        };
        if !self.apps.installed(&def, &self.root) {
            return Err((ErrorCode::Refused, format!("{id} is not installed")));
        }
        if self.pkg.busy_with(id) {
            return Err((
                ErrorCode::Busy,
                format!("{id} is being installed or removed"),
            ));
        }
        // a file to open: resolved against the allowed roots here, argv built by the core
        let file = match path {
            None => None,
            Some(p) => {
                if !def.open {
                    return Err((ErrorCode::InvalidArgs, format!("{id} does not open files")));
                }
                Some(
                    crate::apps::resolve_open_path(&self.root, p)
                        .map_err(|m| (ErrorCode::InvalidArgs, m))?,
                )
            }
        };
        if let Some(f) = &file {
            // a player that is already open switches to the new file: close it, start again when
            // it has gone (app_tick)
            if self.apps.running(id) && self.apps.owns(id) {
                self.apps.set_pending_open(id, f.clone());
                self.close_app(id);
                return Ok("replacing");
            }
        } else if def.single_instance || def.max_instances > 1 {
            // one window of the app is enough: bring it to the front (`new_instance` adds more).
            // By pid, the same lookup `windows_cmd` already uses - not `focus_class`'s plain
            // WM_CLASS search, which cannot tell two apps sharing one class apart (terminal and
            // Kali are both `st`) and, reported by the owner 2026-11-12, failed outright to
            // re-raise a window that had already lost focus once (going Home, then tapping the
            // app again did nothing).
            if let Some(wins) = self.x_try(|b| b.app_windows()) {
                let win = wins
                    .iter()
                    .find(|w| self.app_of_window(&w.win).0.as_deref() == Some(id))
                    .map(|w| w.win.id);
                if let Some(wid) = win {
                    if self.x_try(|b| b.focus_window(wid)).is_some() {
                        return Ok("focused");
                    }
                }
            }
            if self.apps.running(id) {
                return Ok("already_running");
            }
        }
        let pid = self
            .apps
            .spawn_open(id, &self.root, file.as_deref())
            .map_err(|e| match e {
                LaunchError::Closing => (
                    ErrorCode::Busy,
                    format!("{id} is still closing, try again in a moment"),
                ),
                LaunchError::Limit => (ErrorCode::Refused, format!("{id} is at its window limit")),
                e => (ErrorCode::Internal, format!("{e:?}")),
            })?;
        self.publish(
            Topic::Apps,
            "apps",
            json!({"kind": "started", "app": id, "pid": pid}),
        );
        Ok("launched")
    }

    /// Close app (M6.1): our own child gets SIGTERM (SIGKILL after the grace period); an app
    /// somebody else started gets WM_DELETE_WINDOW and, if its window survives the grace period,
    /// SIGTERM of its _NET_WM_PID. Returns how it was done, None if nothing is open.
    pub(super) fn close_app(&mut self, id: &str) -> Option<&'static str> {
        if self.apps.request_close(id, Instant::now()) {
            return Some("terminate");
        }
        if self.apps.owns(id) {
            return None; // already closing
        }
        let class = self.apps.def(id)?.wm_class.clone();
        if class.is_empty() {
            return None;
        }
        let wins: Vec<_> = self
            .x_try(|b| b.windows())?
            .into_iter()
            .filter(|w| class_matches(&w.class, &class))
            .collect();
        if wins.is_empty() {
            return None;
        }
        let mut deadline = Instant::now() + self.close_grace;
        for w in &wins {
            if w.deletable {
                self.x_try(|b| b.close_window(w.id));
            } else {
                // cannot be asked politely: go straight to the signal
                deadline = Instant::now();
            }
        }
        if !self.ext_close.iter().any(|c| c.app == id) {
            self.ext_close.push(ExtClose {
                app: id.to_string(),
                deadline,
            });
        }
        Some("window_close")
    }

    /// Housekeeping for apps: reap children and publish their exit, look for app windows on
    /// screen, finish pending external closes. Returns true when something changed.
    pub(super) fn app_tick(&mut self) -> bool {
        // reap finished helpers so they do not stay zombies
        self.helpers
            .borrow_mut()
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        let now = Instant::now();
        let mut did = false;
        let mut reopen_list: Vec<(String, String)> = Vec::new();
        for (id, exit) in self.apps.reap(now) {
            did = true;
            if let Some(p) = self.apps.take_pending_open(&id) {
                reopen_list.push((id.clone(), p));
            }
            let name = self
                .apps
                .def(&id)
                .map(|d| {
                    if d.name.is_empty() {
                        id.clone()
                    } else {
                        d.name.clone()
                    }
                })
                .unwrap_or_else(|| id.clone());
            // another window of the same app is still open: the app keeps running
            let others = self.apps.running(&id);
            match exit {
                Exit::Clean | Exit::Closed if others => self.publish(
                    Topic::Apps,
                    "apps",
                    json!({"kind": "instance_closed", "app": id}),
                ),
                Exit::Clean | Exit::Closed => self.publish(
                    Topic::Apps,
                    "apps",
                    json!({"kind": "exited", "app": id, "closed": exit == Exit::Closed}),
                ),
                Exit::Crashed { code, signal } => {
                    eprintln!("walpad-core: app {id} crashed (code {code:?}, signal {signal:?})");
                    self.publish(
                        Topic::Apps,
                        "apps",
                        json!({"kind": "crashed", "app": id, "code": code, "signal": signal}),
                    );
                    self.publish(
                        Topic::Notice,
                        "notice",
                        json!({"kind": "app_crashed", "app": id, "name": name}),
                    );
                    // back to the shell so the user is not left on a blank screen
                    if !others {
                        self.x_try(|b| b.focus_title("WalPad Home"));
                    }
                }
            }
        }
        for (id, path) in reopen_list {
            if let Err((_, m)) = self.launch(&id, Some(&path)) {
                eprintln!("walpad-core: reopen {id}: {m}");
            }
        }
        if now >= self.apps_x_at {
            self.apps_x_at = now + APPS_X_EVERY;
            did |= self.scan_app_windows();
        }
        did |= self.finish_ext_closes(now);
        did |= self.windows_tick(now);
        did
    }

    /// Which apps have a window on screen (also apps started by hand or by the Python launcher).
    fn scan_app_windows(&mut self) -> bool {
        if !self.apps.watches_windows() {
            return false;
        }
        let Some(wins) = self.x_try(|b| b.windows()) else {
            return false; // X is away: keep what we knew
        };
        let seen: std::collections::HashSet<String> = self
            .apps
            .defs()
            .iter()
            .filter(|d| self.apps.installed(d, &self.root))
            .filter(|d| wins.iter().any(|w| class_matches(&w.class, &d.wm_class)))
            .map(|d| d.id.clone())
            .collect();
        let changes = self.apps.set_ext(seen);
        let did = !changes.is_empty();
        for (id, up) in changes {
            let kind = if up { "started" } else { "exited" };
            self.publish(
                Topic::Apps,
                "apps",
                json!({"kind": kind, "app": id, "external": true}),
            );
        }
        did
    }

    fn finish_ext_closes(&mut self, now: Instant) -> bool {
        let due: Vec<String> = self
            .ext_close
            .iter()
            .filter(|c| now >= c.deadline)
            .map(|c| c.app.clone())
            .collect();
        if due.is_empty() {
            return false;
        }
        self.ext_close.retain(|c| now < c.deadline);
        for id in due {
            let class = self
                .apps
                .def(&id)
                .map(|d| d.wm_class.clone())
                .unwrap_or_default();
            let wins = self.x_try(|b| b.windows()).unwrap_or_default();
            for w in wins.iter().filter(|w| class_matches(&w.class, &class)) {
                match w.pid {
                    Some(pid) if terminate_pid(pid, false) => {
                        eprintln!(
                            "walpad-core: {id} ignored WM_DELETE_WINDOW, sent SIGTERM to {pid}"
                        )
                    }
                    _ => eprintln!("walpad-core: {id} did not close and has no usable pid"),
                }
            }
        }
        true
    }
}
