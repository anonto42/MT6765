//! Periodic timers, call-audio process lifecycle, and AT job scheduling/handling for [`super::Phone`].
use super::ops::{ok_final, reply, AudioWant, Op};
use super::{parse, Phone, PhoneKind, PATH};
use crate::modem::at::{Final, Job, Outcome};
use crate::modem::JobKind;
use serde_json::json;
use std::process::{Command, Stdio};
use std::time::Instant;
use walpad_proto::{CallState, UssdStatus};

impl Phone {
    /// Timers: the `ended` banner and the audio script.
    pub fn tick(&mut self, now: Instant) {
        if self.ended.as_ref().is_some_and(|e| now >= e.until) {
            self.ended = None;
            self.dirty = true;
        }
        if let Some((c, deadline)) = &mut self.audio_run {
            let done = matches!(c.try_wait(), Ok(Some(_)) | Err(_));
            if done {
                self.audio_run = None;
            } else if now >= *deadline {
                let _ = c.kill();
                let _ = c.wait();
                self.audio_run = None;
                eprintln!("walpad-core: walpad-callaudio timed out");
            }
        }
        if self.audio_run.is_none() {
            if let Some(w) = self.audio_want.take() {
                self.run_audio(w, now);
            }
        }
    }

    fn run_audio(&mut self, w: AudioWant, now: Instant) {
        if !self.script.exists() {
            return;
        }
        let args: Vec<&str> = match w {
            AudioWant::Start(true) => vec!["start", "speaker"],
            AudioWant::Start(false) => vec!["start", "earpiece"],
            AudioWant::Stop => vec!["stop"],
        };
        let mut cmd = Command::new(&self.script);
        cmd.args(args)
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if self.root_base != std::path::Path::new("/") {
            cmd.env("WALPAD_CALLAUDIO_ROOT", &self.root_base);
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        match cmd.spawn() {
            Ok(c) => self.audio_run = Some((c, now + self.t.audio)),
            Err(e) => eprintln!("walpad-core: walpad-callaudio: {e}"),
        }
    }

    // ---------------------------------------------------------------- jobs
    fn start(&mut self, op: Op) -> Job<JobKind> {
        let job = Job {
            kind: JobKind::Phone(op.kind()),
            cmds: op.cmds(),
        };
        self.inflight = Some(op);
        job
    }

    /// The next AT job, if one is due: queued commands first, then the call list, then messages.
    pub fn next_job(&mut self, now: Instant) -> Option<Job<JobKind>> {
        if !self.gate.ready || self.inflight.is_some() {
            return None;
        }
        if !self.setup_done && self.gate.sim_ready && now >= self.next_setup {
            self.next_setup = now + self.t.setup_retry;
            return Some(self.start(Op::Setup));
        }
        if let Some(op) = self.queue.pop_front() {
            return Some(self.start(op));
        }
        // messages first when due (so a short call-list period cannot starve them), except while
        // a call is being set up or rings: the call list matters more then
        let signalling = self
            .call
            .as_ref()
            .is_some_and(|c| c.state != CallState::Active);
        let sms_due = self.setup_done && self.gate.sim_ready && now >= self.next_sms;
        if sms_due && !signalling {
            self.next_sms = now + self.t.sms_poll;
            return Some(self.start(Op::SmsPoll));
        }
        if self.gate.radio_on && now >= self.next_clcc {
            let p = if self.call.is_some() {
                self.t.clcc_call
            } else {
                self.t.clcc_idle
            };
            self.next_clcc = now + p;
            return Some(self.start(Op::Clcc));
        }
        if sms_due {
            self.next_sms = now + self.t.sms_poll;
            return Some(self.start(Op::SmsPoll));
        }
        None
    }

    /// The results of the job `next_job` handed out.
    pub fn handle(&mut self, kind: PhoneKind, results: &[Outcome], now: Instant) {
        let Some(op) = self.inflight.take() else {
            return;
        };
        if op.kind() != kind {
            return;
        }
        let mut got_cusd = false;
        for r in results.iter().flatten() {
            if let Some(c) = parse::cusd(&r.lines) {
                got_cusd = true;
                self.on_cusd(c);
            }
        }
        let first = results.first();
        match op {
            Op::Setup => {
                if ok_final(first) {
                    self.setup_done = true;
                    self.smsc = results
                        .last()
                        .and_then(|o| o.as_ref().ok())
                        .and_then(|r| parse::csca(&r.lines));
                    self.next_sms = now;
                    eprintln!("walpad-core: phone ready (PDU mode)");
                }
            }
            // (a timeout or an error keeps the state: the next poll decides)
            Op::Clcc => {
                if let Some(Ok(r)) = first {
                    self.apply_clcc(r, now)
                }
            }
            Op::Dial(_) => self.dial_result(first, now),
            Op::Ussd(_) => {
                if !got_cusd && !ok_final(first) {
                    self.ussd = Some(UssdStatus {
                        state: "failed".into(),
                        text: Some("USSD request failed".into()),
                        interactive: false,
                    });
                    self.dirty = true;
                }
            }
            Op::UssdCancel => {}
            Op::SimContactsRange => {
                let (lo, hi) = results
                    .last()
                    .and_then(|o| o.as_ref().ok())
                    .and_then(|r| parse::cpbr_range(&r.lines))
                    .unwrap_or((1, 50));
                let mut start = lo;
                while start <= hi {
                    let end = (start + 49).min(hi);
                    self.queue.push_back(Op::SimContactsRead(start, end));
                    start = end + 1;
                }
            }
            Op::SimContactsRead(..) => {
                if let Some(r) = reply(first).filter(|r| r.fin == Final::Ok) {
                    let cards: Vec<(String, Vec<String>)> = parse::cpbr(&r.lines)
                        .into_iter()
                        .map(|c| (c.name, vec![c.number]))
                        .collect();
                    if !cards.is_empty() {
                        let added = self.contacts.import(cards);
                        if added > 0 {
                            self.dirty = true;
                            self.notice(json!({"kind": "sim_contacts", "added": added}));
                        }
                    }
                }
            }
            Op::Answer => {
                if !ok_final(first) {
                    self.notice(json!({"kind": "phone_error", "what": "answer"}));
                }
                self.next_clcc = now;
            }
            Op::Hangup { ath } => {
                if !ok_final(first) && !ath && self.call.is_some() {
                    self.queue.push_front(Op::Hangup { ath: true });
                }
                self.next_clcc = now;
            }
            Op::Dtmf(_) => {
                if !ok_final(first) {
                    self.notice(json!({"kind": "phone_error", "what": "dtmf"}));
                }
            }
            Op::Mute(on) => {
                if ok_final(first) {
                    self.muted = on;
                    self.dirty = true;
                } else {
                    self.notice(json!({"kind": "phone_error", "what": "mute"}));
                }
            }
            Op::SmsSend { id, parts } => self.sms_result(id, parts.len(), results),
            Op::SmsPoll => {
                if let Some(r) = reply(first).filter(|r| r.fin == Final::Ok) {
                    self.import_sms(r, now);
                }
            }
            Op::SmsDelete(_) => {}
        }
    }
}
