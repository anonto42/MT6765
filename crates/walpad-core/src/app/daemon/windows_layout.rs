//! Window layout loading, periodic layout enforcement, and close-grace deadlines for [`Daemon`].
use super::errors::read_file;
use super::Daemon;
use crate::apps::terminate_pid;
use crate::layout::{Layout, LayoutOp, WinView};
use serde_json::Value;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::wm::LayoutMode;

/// How often the layout engine looks at the windows while a mode other than `single` is on.
pub(super) const LAYOUT_EVERY: Duration = Duration::from_millis(400);

/// A close request for one window: SIGTERM of its pid when it is still there at `deadline`.
pub(super) struct WinClose {
    pub(super) window: u32,
    pub(super) pid: Option<u32>,
    pub(super) deadline: Instant,
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
