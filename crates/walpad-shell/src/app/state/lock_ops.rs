//! Screen-lock flow actions, sheet submission, and reply handling for `App`.

use super::lock_flow::{LockAct, LockFlow, Op, AUTO_LOCK_OPTIONS};
use super::model::friendly;
use super::{App, Cmd, Outcome};
use crate::core_client::ReplyError;
use crate::overlay::{Overlay, PasswordSheet, SheetPurpose};
use walpad_proto::{LockKind, Secret};
use walpad_ui::components::Tone;

impl App {
    fn lock_status(&self) -> walpad_proto::LockStatus {
        self.model
            .status
            .as_ref()
            .map(|s| s.lock.clone())
            .unwrap_or_default()
    }

    fn open_lock_sheet(&mut self, purpose: SheetPurpose, error: Option<String>) {
        let mut s = PasswordSheet::lock(purpose);
        s.error = error;
        self.model.overlay = Some(Overlay::Password(s));
    }

    pub(super) fn on_lock_tap(&mut self, a: LockAct, out: &mut Outcome) {
        let l = self.lock_status();
        out.dirty = true;
        match a {
            LockAct::SetPin | LockAct::SetPassword if !l.configured => {
                let kind = if a == LockAct::SetPin {
                    LockKind::Pin
                } else {
                    LockKind::Password
                };
                self.model.lock_flow = Some(LockFlow {
                    op: Op::Set,
                    kind,
                    current: None,
                    first: None,
                });
                self.open_lock_sheet(SheetPurpose::LockNew(kind), None);
            }
            LockAct::Change | LockAct::Remove if l.configured => {
                let kind = match l.kind {
                    Some(k @ (LockKind::Pin | LockKind::Password)) => k,
                    _ => {
                        self.toast("Use walpad-ctl lock-set over SSH for a pattern", Tone::Info);
                        return;
                    }
                };
                let (op, purpose) = if a == LockAct::Change {
                    (Op::Change, SheetPurpose::LockCurrent(kind))
                } else {
                    (Op::Remove, SheetPurpose::LockRemove(kind))
                };
                self.model.lock_flow = Some(LockFlow {
                    op,
                    kind,
                    current: None,
                    first: None,
                });
                self.open_lock_sheet(purpose, None);
            }
            LockAct::Now if l.configured => out.cmd(Cmd::LockNow),
            LockAct::CycleAuto if l.configured => {
                let i = AUTO_LOCK_OPTIONS
                    .iter()
                    .position(|(s, _)| *s == l.auto_lock_s)
                    .map_or(0, |i| (i + 1) % AUTO_LOCK_OPTIONS.len());
                out.cmd(Cmd::LockConfig(AUTO_LOCK_OPTIONS[i].0));
            }
            _ => {}
        }
    }

    /// The sheet's confirm button (or Done on the keyboard).
    pub(super) fn lock_submit(&mut self, out: &mut Outcome) {
        let Some(Overlay::Password(s)) = &mut self.model.overlay else {
            return;
        };
        if !s.valid() || s.connecting {
            if !s.valid() {
                s.error = Some(
                    match s.purpose {
                        SheetPurpose::LockNew(LockKind::Pin)
                        | SheetPurpose::LockRepeat(LockKind::Pin) => "The PIN has 4-12 digits",
                        SheetPurpose::LockNew(_) | SheetPurpose::LockRepeat(_) => {
                            "The password has 4-64 characters"
                        }
                        _ => "Type the current secret",
                    }
                    .into(),
                );
            }
            return;
        }
        let typed = Secret::new(s.secret.clone());
        let purpose = s.purpose;
        let Some(flow) = self.model.lock_flow.as_mut() else {
            return;
        };
        out.dirty = true;
        match purpose {
            SheetPurpose::LockRemove(_) => {
                s.connecting = true;
                s.error = None;
                out.cmd(Cmd::LockRemove(typed));
            }
            SheetPurpose::LockCurrent(k) => {
                flow.current = Some(typed);
                self.open_lock_sheet(SheetPurpose::LockNew(k), None);
            }
            SheetPurpose::LockNew(k) => {
                flow.first = Some(typed);
                self.open_lock_sheet(SheetPurpose::LockRepeat(k), None);
            }
            SheetPurpose::LockRepeat(k) => {
                if flow.first.as_ref() == Some(&typed) {
                    s.connecting = true;
                    s.error = None;
                    out.cmd(Cmd::LockSet {
                        kind: k,
                        secret: typed,
                        current: flow.current.clone(),
                    });
                } else {
                    s.secret.clear();
                    s.error = Some("The two entries differ".into());
                }
            }
            _ => {}
        }
    }

    /// The core answered a lock request.
    pub(super) fn on_lock_reply(
        &mut self,
        ok: bool,
        error: Option<&ReplyError>,
        out: &mut Outcome,
    ) {
        out.dirty = true;
        let flow = self.model.lock_flow.take();
        if ok {
            // lock_now / lock_config: the status event shows the result
            if let Some(f) = flow {
                self.close_overlay();
                self.toast(
                    match f.op {
                        Op::Set => "Screen lock set",
                        Op::Change => "Screen lock changed",
                        Op::Remove => "Screen lock removed",
                    },
                    Tone::Info,
                );
            }
            return;
        }
        let msg = error.map_or_else(|| "Could not change the lock".to_string(), friendly);
        let Some(f) = flow else {
            self.toast(msg, Tone::Danger);
            return;
        };
        match f.op {
            // the current secret was wrong: back to asking for it
            Op::Change if msg.contains("wrong") || msg.contains("wait") => {
                self.model.lock_flow = Some(LockFlow {
                    current: None,
                    first: None,
                    ..f.clone()
                });
                self.open_lock_sheet(SheetPurpose::LockCurrent(f.kind), Some(msg));
            }
            _ => {
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.connecting = false;
                    s.secret.clear();
                    s.error = Some(msg);
                }
                self.model.lock_flow = Some(f);
            }
        }
    }
}
