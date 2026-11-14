use super::layout::{DockButton, AUTO_COLLAPSE_MS};
use crate::app::Cmd;
use walpad_hw::Root;

/// `(app id, wm_class)` of every app in apps.json that names a window class.
pub fn load_wm_classes(root: &Root) -> Vec<(String, String)> {
    walpad_proto::appdefs::entries(
        &root.p(walpad_proto::appdefs::APPS_JSON),
        &root.p(walpad_proto::appdefs::APPS_D),
    )
    .iter()
    .filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let class = e["wm_class"].as_str().filter(|c| !c.is_empty())?;
        Some((id, class.to_string()))
    })
    .collect()
}

/// The apps.json id for a window's WM_CLASS (instance, class), compared case-insensitively.
pub fn app_for_class(apps: &[(String, String)], wm_class: &[String]) -> Option<String> {
    apps.iter()
        .find(|(_, c)| wm_class.iter().any(|w| w.eq_ignore_ascii_case(c)))
        .map(|(id, _)| id.clone())
}

/// Should the dock (its handle) be up? `active` is `_NET_ACTIVE_WINDOW` (0 = none). One of the
/// dock's own windows being active (a tap on it) changes nothing.
pub fn wants_visible(active: u32, shell: u32, dock: &[u32], currently: bool) -> bool {
    if dock.contains(&active) {
        currently
    } else {
        active != 0 && active != shell
    }
}

/// What a tap does. Home is handled in X by the dock itself, so it yields no command.
pub fn command_for(
    b: DockButton,
    apps: &[(String, String)],
    active_class: &[String],
) -> Option<Cmd> {
    match b {
        DockButton::Home => None,
        DockButton::Back => Some(Cmd::Back),
        DockButton::Keyboard => Some(Cmd::KeyboardToggle),
        DockButton::CloseApp => app_for_class(apps, active_class).map(Cmd::CloseApp),
    }
}

/// Expanded / collapsed state of the strip with its auto-collapse timer (milliseconds on any
/// monotonic clock).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub expanded: bool,
    collapse_at: Option<u64>,
}

impl Fold {
    /// A tap on the handle: expand (with the timer) or collapse.
    pub fn toggle(&mut self, now: u64) {
        if self.expanded {
            self.collapse();
        } else {
            self.expanded = true;
            self.collapse_at = Some(now + AUTO_COLLAPSE_MS);
        }
    }

    /// A touch on the expanded strip restarts the timer.
    pub fn touched(&mut self, now: u64) {
        if self.expanded {
            self.collapse_at = Some(now + AUTO_COLLAPSE_MS);
        }
    }

    pub fn collapse(&mut self) {
        self.expanded = false;
        self.collapse_at = None;
    }

    /// True when the timer just folded the strip.
    pub fn tick(&mut self, now: u64) -> bool {
        if self.collapse_at.is_some_and(|t| now >= t) {
            self.collapse();
            return true;
        }
        false
    }

    pub fn deadline(&self) -> Option<u64> {
        self.collapse_at
    }
}
