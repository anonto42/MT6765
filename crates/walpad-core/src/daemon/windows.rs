//! Window commands of the daemon (change multitasking-windows): the window list, extra instances
//! of an app (several terminals), focus/close by window id, move/resize/snap and the layout mode.
//! The geometry is `walpad_proto::wm`, the decisions are `crate::layout`, the X requests are
//! `XBridge`.
use super::*;
use crate::layout::{Layout, LayoutOp, WinView};
use crate::xbridge::{WinInfo, XWindow};
use walpad_proto::wm::{self, LayoutMode, SnapZone, WRect};

/// How often the layout engine looks at the windows while a mode other than `single` is on.
pub(super) const LAYOUT_EVERY: Duration = Duration::from_millis(400);
/// A window that ignores WM_DELETE_WINDOW gets SIGTERM after this long.
const WIN_CLOSE_GRACE: Duration = Duration::from_secs(3);

/// A close request for one window: SIGTERM of its pid when it is still there at `deadline`.
pub(super) struct WinClose {
    window: u32,
    pid: Option<u32>,
    deadline: Instant,
}

/// Mode, ratio and swap saved in /etc/walpad/shell.json (absent or bad values: single, 50, false).
pub(super) fn load_layout(root: &Root) -> Layout {
    let v: Value =
        serde_json::from_str(&read_file(root, "/etc/walpad/shell.json")).unwrap_or(Value::Null);
    Layout::new(
        v["windows_mode"]
            .as_str()
            .and_then(LayoutMode::parse)
            .unwrap_or_default(),
        v["split_ratio"].as_u64().unwrap_or(50).min(255) as u8,
        v["split_swap"].as_bool().unwrap_or(false),
    )
}

impl Daemon {
    fn layout_json(&self) -> Value {
        json!({"mode": self.layout.mode.as_str(), "ratio": self.layout.ratio, "swap": self.layout.swap})
    }

    /// The app (id, instance number) a window belongs to: our child by pid, else the first app
    /// whose wm_class matches.
    pub(super) fn app_of_window(&self, w: &XWindow) -> (Option<String>, Option<u32>) {
        if let Some((id, n)) = w.pid.and_then(|p| self.apps.instance_of_pid(p)) {
            return (Some(id), Some(n));
        }
        let id = self
            .apps
            .defs()
            .iter()
            .find(|d| !d.wm_class.is_empty() && class_matches(&w.class, &d.wm_class))
            .map(|d| d.id.clone());
        (id, None)
    }

    /// One app window by id (the shell's own windows are never offered).
    fn app_window(&mut self, id: u32) -> Result<WinInfo, (ErrorCode, String)> {
        let wins = self
            .x_try(|b| b.app_windows())
            .ok_or((ErrorCode::Internal, "the display is not reachable".into()))?;
        wins.into_iter()
            .find(|w| w.win.id == id)
            .ok_or((ErrorCode::InvalidArgs, format!("no app window {id}")))
    }

    pub(super) fn work_area(&mut self) -> WRect {
        let o = self.brain.rotation.orientation();
        self.x_try(|b| Ok(b.work_area(o)))
            .unwrap_or_else(|| wm::panel(o))
    }

    /// `windows`: the app windows on screen with their app, instance, title and geometry.
    pub(super) fn windows_cmd(&mut self) -> CmdResult {
        let Some((wins, active)) = self.x_try(|b| Ok((b.app_windows()?, b.active_window()?)))
        else {
            return Ok(json!({"windows": [], "layout": self.layout_json()}));
        };
        let n = wins.len();
        let mut list: Vec<Value> = wins
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let (app, instance) = self.app_of_window(&w.win);
                json!({
                    "window": w.win.id, "app": app, "instance": instance, "title": w.title,
                    "x": w.rect.x, "y": w.rect.y, "w": w.rect.w, "h": w.rect.h,
                    "focused": w.win.id == active, "maximized": w.maximized,
                    // 0 = front
                    "z": n - 1 - i,
                })
            })
            .collect();
        // stable order for tabs: by window id (creation order)
        list.sort_by_key(|v| v["window"].as_u64().unwrap_or(0));
        Ok(json!({"windows": list, "layout": self.layout_json()}))
    }

    /// `new_instance`: one more copy of an app that allows several.
    pub(super) fn new_instance(&mut self, id: &str) -> CmdResult {
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
        if self.apps.max_instances(id) < 2 {
            return Err((
                ErrorCode::Refused,
                format!("{id} does not run several windows"),
            ));
        }
        let (pid, n) = self.apps.spawn_new(id, &self.root).map_err(|e| match e {
            LaunchError::Limit => (
                ErrorCode::Refused,
                format!("{id} already runs {} windows", self.apps.max_instances(id)),
            ),
            LaunchError::Closing => (
                ErrorCode::Busy,
                format!("{id} is still closing, try again in a moment"),
            ),
            e => (ErrorCode::Internal, format!("{e:?}")),
        })?;
        self.publish(
            Topic::Apps,
            "apps",
            json!({"kind": "started", "app": id, "pid": pid, "instance": n}),
        );
        Ok(json!({"app": id, "instance": n, "pid": pid}))
    }

    pub(super) fn focus_window(&mut self, id: u32) -> CmdResult {
        self.app_window(id)?;
        self.x_try(|b| b.focus_window(id))
            .ok_or((ErrorCode::Internal, "the display is not reachable".into()))?;
        Ok(json!({"window": id}))
    }

    /// `close_window`: WM_DELETE_WINDOW now, SIGTERM of the window's pid if it is still there
    /// after the grace period (or at once when it cannot be asked politely).
    pub(super) fn close_window_cmd(&mut self, id: u32) -> CmdResult {
        let w = self.app_window(id)?;
        let mut deadline = Instant::now() + WIN_CLOSE_GRACE;
        if w.win.deletable {
            self.x_try(|b| b.close_window(id));
        } else {
            deadline = Instant::now();
        }
        self.win_closes.push(WinClose {
            window: id,
            pid: w.win.pid,
            deadline,
        });
        Ok(json!({"window": id}))
    }

    /// Free mode only: tiles and maximised windows are the layout engine's.
    fn require_free(&self) -> Result<(), (ErrorCode, String)> {
        if self.layout.mode == LayoutMode::Free {
            Ok(())
        } else {
            Err((
                ErrorCode::Refused,
                "windows can be placed by hand in free mode only (layout mode free)".into(),
            ))
        }
    }

    fn place_window(&mut self, id: u32, want: WRect) -> CmdResult {
        let w = self.app_window(id)?;
        let area = self.work_area();
        let r = wm::clamp_rect(want, area);
        self.x_try(|b| {
            if w.maximized {
                b.set_maximized(id, false)?;
            }
            b.move_resize(id, r)
        })
        .ok_or((ErrorCode::Internal, "the display is not reachable".into()))?;
        self.layout.placed(id, r);
        Ok(json!({"window": id, "x": r.x, "y": r.y, "w": r.w, "h": r.h}))
    }

    pub(super) fn window_rect_cmd(&mut self, id: u32, r: WRect) -> CmdResult {
        self.require_free()?;
        self.place_window(id, r)
    }

    pub(super) fn snap_cmd(&mut self, id: u32, zone: SnapZone) -> CmdResult {
        self.require_free()?;
        let area = self.work_area();
        self.place_window(id, wm::snap_rect(area, zone))
    }

    /// `layout`: read, or change mode / ratio / swap (saved in shell.json).
    pub(super) fn layout_cmd(
        &mut self,
        mode: Option<LayoutMode>,
        ratio: Option<u8>,
        swap: bool,
    ) -> CmdResult {
        let mut changed = false;
        if let Some(m) = mode {
            if m != self.layout.mode {
                self.layout.mode = m;
                self.set_setting("windows_mode", json!(m.as_str()))?;
                changed = true;
            }
        }
        if let Some(r) = ratio {
            if r != self.layout.ratio {
                self.layout.ratio = r;
                self.set_setting("split_ratio", json!(r))?;
                changed = true;
            }
        }
        if swap {
            self.layout.swap = !self.layout.swap;
            self.set_setting("split_swap", json!(self.layout.swap))?;
            changed = true;
        }
        if changed {
            self.layout.invalidate();
            self.layout_at = Instant::now();
            self.windows_tick(Instant::now());
            let ev = self.layout_json();
            self.publish(
                Topic::Apps,
                "apps",
                json!({"kind": "layout", "mode": ev["mode"], "ratio": ev["ratio"], "swap": ev["swap"]}),
            );
        }
        Ok(self.layout_json())
    }

    /// Layout engine and window close deadlines. Returns true when something was done.
    pub(super) fn windows_tick(&mut self, now: Instant) -> bool {
        let mut did = self.finish_win_closes(now);
        if self.layout.busy() && now >= self.layout_at {
            self.layout_at = now + LAYOUT_EVERY;
            did |= self.apply_layout();
        }
        did
    }

    fn apply_layout(&mut self) -> bool {
        let o = self.brain.rotation.orientation();
        let Some((area, wins)) = self.x_try(|b| Ok((b.work_area(o), b.app_windows()?))) else {
            // X is away: everything is sent again once it is back
            self.layout.invalidate();
            return false;
        };
        let views: Vec<WinView> = wins
            .iter()
            .map(|w| WinView {
                id: w.win.id,
                rect: w.rect,
                maximized: w.maximized,
            })
            .collect();
        let ops = self.layout.plan(&views, area);
        if ops.is_empty() {
            return false;
        }
        let done = self.x_try(|b| {
            for op in &ops {
                match *op {
                    LayoutOp::Maximize(w) => b.set_maximized(w, true)?,
                    LayoutOp::Unmaximize(w) => b.set_maximized(w, false)?,
                    LayoutOp::Place(w, r) => b.move_resize(w, r)?,
                }
            }
            Ok(())
        });
        if done.is_none() {
            self.layout.invalidate();
        }
        true
    }

    fn finish_win_closes(&mut self, now: Instant) -> bool {
        if self.win_closes.is_empty() {
            return false;
        }
        let due: Vec<WinClose> = {
            let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.win_closes)
                .into_iter()
                .partition(|c| now >= c.deadline);
            self.win_closes = keep;
            due
        };
        if due.is_empty() {
            return false;
        }
        let alive: Vec<u32> = self
            .x_try(|b| b.app_windows())
            .unwrap_or_default()
            .iter()
            .map(|w| w.win.id)
            .collect();
        for c in due {
            if alive.contains(&c.window) {
                match c.pid {
                    Some(pid) if terminate_pid(pid, false) => {
                        eprintln!(
                            "walpad-core: window {} ignored close, SIGTERM to {pid}",
                            c.window
                        )
                    }
                    _ => eprintln!("walpad-core: window {} did not close", c.window),
                }
            }
        }
        true
    }
}
