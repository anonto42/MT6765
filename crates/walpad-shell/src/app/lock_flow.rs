//! Settings > Security: set, change and remove the screen lock, lock now, the auto-lock delay
//! (OpenSpec `lock-screen`). A small sequence of the shell's password sheets:
//!
//! * set:    New (type) -> Repeat (type again) -> `lock_set`
//! * change: Current -> New -> Repeat -> `lock_set` with the current secret (a wrong one comes
//!   back from the core and returns to the Current sheet)
//! * remove: Current -> `lock_remove`
//!
//! The secrets are only held in the sheets and in `LockFlow` until the request is sent; they are
//! `Secret`s (Debug prints `***`) and are never logged. Only the core verifies anything.
//! A pattern lock can be locked and unlocked on the lock screen but cannot be typed in these
//! sheets: Settings says so and points at `walpad-ctl lock-set` / `lock-remove` over SSH.

use super::Target;
use crate::settings_screen::Row;
use walpad_proto::{LockKind, Secret, Status};
use walpad_ui::components::Tone;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockAct {
    SetPin,
    SetPassword,
    Change,
    Remove,
    Now,
    /// Next entry of `AUTO_LOCK_OPTIONS`.
    CycleAuto,
}

/// Lock this long after the screen went off (seconds, label).
pub const AUTO_LOCK_OPTIONS: [(u32, &str); 6] = [
    (0, "immediately"),
    (30, "after 30 s"),
    (60, "after 1 min"),
    (300, "after 5 min"),
    (900, "after 15 min"),
    (1800, "after 30 min"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Set,
    Change,
    Remove,
}

/// The open sequence: what is being done and the secrets typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockFlow {
    pub op: Op,
    pub kind: LockKind,
    pub current: Option<Secret>,
    pub first: Option<Secret>,
}

fn kind_name(k: LockKind) -> &'static str {
    match k {
        LockKind::Pin => "PIN",
        LockKind::Password => "Password",
        LockKind::Pattern => "Pattern",
    }
}

fn auto_label(secs: u32) -> &'static str {
    AUTO_LOCK_OPTIONS
        .iter()
        .find(|(s, _)| *s == secs)
        .map_or("custom", |(_, l)| l)
}

/// The Security page's screen-lock rows for the current status.
pub fn lock_rows(st: &Option<Status>) -> Vec<Row> {
    let Some(s) = st else {
        return vec![Row::Info(
            "Screen lock".into(),
            "Unknown".into(),
            Tone::Warning,
        )];
    };
    let l = &s.lock;
    let mut rows = Vec::new();
    match (l.configured, l.kind) {
        (true, Some(k)) => {
            rows.push(Row::Info(
                "Screen lock".into(),
                kind_name(k).into(),
                Tone::Info,
            ));
            rows.push(Row::ActionText {
                label: "Lock now".into(),
                target: Target::Lock(LockAct::Now),
                danger: false,
            });
            rows.push(Row::ActionText {
                label: format!("Lock {}", auto_label(l.auto_lock_s)),
                target: Target::Lock(LockAct::CycleAuto),
                danger: false,
            });
            if k == LockKind::Pattern {
                rows.push(Row::Mono(
                    "Pattern lock: change it with walpad-ctl lock-set over SSH".into(),
                ));
            } else {
                rows.push(Row::ActionText {
                    label: format!("Change {}", kind_name(k)),
                    target: Target::Lock(LockAct::Change),
                    danger: false,
                });
                rows.push(Row::ActionText {
                    label: "Remove screen lock".into(),
                    target: Target::Lock(LockAct::Remove),
                    danger: true,
                });
            }
        }
        _ => {
            rows.push(Row::Info("Screen lock".into(), "Off".into(), Tone::Warning));
            rows.push(Row::Action {
                label: "Set a PIN",
                target: Target::Lock(LockAct::SetPin),
                danger: false,
            });
            rows.push(Row::Action {
                label: "Set a password",
                target: Target::Lock(LockAct::SetPassword),
                danger: false,
            });
        }
    }
    rows.push(Row::Mono(
        "The USB recovery shell (telnet 192.168.42.1) is NOT protected by this lock.".into(),
    ));
    rows
}
