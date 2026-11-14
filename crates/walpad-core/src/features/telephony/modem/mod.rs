//! Cellular modem (owner request 2026-11-10): SIM state, radio, signal, operator and mobile data.
//!
//! * The modem is booted by `/usr/local/sbin/walpad-modem start` (vendor daemons + gsm0710muxd),
//!   run once per boot from the core when the AT channel is not there yet.
//! * Status comes from AT commands on /dev/radio/pttycmd1, sent by one bounded worker thread
//!   (`at::Worker`, 5 s per command); the core loop only queues jobs and reads results.
//! * Mobile data is the script's job (`walpad-modem data on|off|status`, 40 s): it is a child
//!   process polled from the loop like walpad-hotspot. No AT job runs while the script runs (one
//!   opener per channel).
//! * Never logged: the SIM PIN (sent only over the channel), IMEI and ICCID (only `mobile_info`).

pub mod at;
mod commands;
mod jobs;
pub mod parse;
mod script;
#[cfg(test)]
mod tests;
mod types;

pub use types::{
    parse_conf, JobKind, MobileError, Timing, CONF, NOTI_TTY, RADIO_FILE, SCRIPT, START_MARK, TTY,
};

use crate::phone::{Gate, Phone};
use at::{AtCmd, Job, Recv, Worker};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;
use walpad_hw::Root;
use walpad_proto::{MobileData, MobileReg, MobileStatus, ModemState, SimState};

use types::{Run, Verb, OUT};

pub struct Modem {
    root: Root,
    script: PathBuf,
    tty: PathBuf,
    noti_tty: PathBuf,
    noti: Option<std::fs::File>,
    noti_buf: String,
    conf: PathBuf,
    radio_file: PathBuf,
    start_mark: PathBuf,
    out: PathBuf,
    st: MobileStatus,
    worker: Option<Worker<JobKind>>,
    inflight: bool,
    queue: VecDeque<Job<JobKind>>,
    run: Option<Run>,
    next_poll: Instant,
    next_check: Instant,
    radio_pref: bool,
    /// The persisted radio choice was applied after the modem became ready.
    pref_applied: bool,
    /// A data on/off waiting for the running AT job to finish.
    pending_data: Option<bool>,
    imei: Option<String>,
    iccid: Option<String>,
    notices: Vec<Value>,
    t: Timing,
    /// Voice calls and text messages share this module's AT worker (one opener per channel).
    phone: Phone,
}

impl Modem {
    pub fn new(root: &Root) -> Modem {
        let now = Instant::now();
        let mut m = Modem {
            root: root.clone(),
            script: root.p(SCRIPT),
            tty: root.p(TTY),
            noti_tty: root.p(NOTI_TTY),
            noti: None,
            noti_buf: String::new(),
            conf: root.p(CONF),
            radio_file: root.p(RADIO_FILE),
            start_mark: root.p(START_MARK),
            out: root.p(OUT),
            st: MobileStatus::default(),
            worker: None,
            inflight: false,
            queue: VecDeque::new(),
            run: None,
            next_poll: now,
            next_check: now,
            radio_pref: true,
            pref_applied: false,
            pending_data: None,
            imei: None,
            iccid: None,
            notices: Vec::new(),
            t: Timing::default(),
            phone: Phone::new(root),
        };
        m.radio_pref = std::fs::read_to_string(&m.radio_file)
            .map(|s| s.trim() != "0")
            .unwrap_or(true);
        m.st.apn = parse_conf(&std::fs::read_to_string(&m.conf).unwrap_or_default());
        m
    }

    /// Test hook: shorter timeouts and poll periods.
    pub fn set_timing(&mut self, t: Timing) {
        self.t = t;
    }

    pub fn status(&self) -> &MobileStatus {
        &self.st
    }

    /// Events for the `notice` topic (results of radio / PIN / data requests).
    pub fn take_notices(&mut self) -> Vec<Value> {
        let mut v = std::mem::take(&mut self.notices);
        v.extend(self.phone.take_notices());
        v
    }

    /// The phone (calls, messages, contacts) living on this modem's AT worker.
    pub fn phone(&self) -> &Phone {
        &self.phone
    }

    pub fn phone_mut(&mut self) -> &mut Phone {
        &mut self.phone
    }

    /// What the phone needs to know about the line.
    fn gate(&self) -> Gate {
        Gate {
            ready: self.st.modem == ModemState::Ready,
            radio_on: self.st.radio_on == Some(true),
            sim_ready: self.st.sim == SimState::Ready,
            registered: matches!(self.st.registration, MobileReg::Home | MobileReg::Roaming),
        }
    }

    /// Run a phone command against the current line state, then let the worker start on it.
    pub fn phone_op<R>(&mut self, now: Instant, f: impl FnOnce(&mut Phone, Instant) -> R) -> R {
        self.phone.set_gate(self.gate(), now);
        let r = f(&mut self.phone, now);
        self.dispatch(now);
        r
    }

    /// The phone block of the status.
    pub fn phone_status(&self, now: Instant) -> walpad_proto::PhoneStatus {
        self.phone.status(now)
    }

    fn notice(&mut self, v: Value) {
        self.notices.push(v);
    }

    /// At core start: pick up a running modem, or boot it once per boot.
    pub fn startup(&mut self, now: Instant) {
        if !self.script.exists() {
            self.st.modem = ModemState::Absent;
            return;
        }
        self.st.modem = ModemState::Present;
        if self.tty.exists() {
            self.become_ready(now);
        } else if !self.start_mark.exists() {
            if let Some(d) = self.start_mark.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&self.start_mark, b"1\n");
            match self.spawn(Verb::Start, now) {
                Ok(()) => {
                    eprintln!("walpad-core: starting the modem (walpad-modem start)");
                    self.st.modem = ModemState::Booting;
                }
                Err(e) => self.fail(e),
            }
        } else {
            self.st.reason = Some("the modem is not running".into());
        }
    }

    fn fail(&mut self, reason: String) {
        eprintln!("walpad-core: modem: {reason}");
        self.st.modem = ModemState::Failed;
        self.st.reason = Some(reason);
    }

    fn become_ready(&mut self, now: Instant) {
        if self.worker.is_none() {
            match Worker::spawn(self.tty.clone(), self.t.at_cmd) {
                Ok(w) => self.worker = Some(w),
                Err(e) => return self.fail(format!("cannot start the AT worker: {e}")),
            }
        }
        eprintln!("walpad-core: modem ready (AT channel present)");
        self.st.modem = ModemState::Ready;
        self.st.reason = None;
        self.pref_applied = false;
        self.queue.clear();
        self.queue.push_back(Job {
            kind: JobKind::Init,
            cmds: ["ATE0", "AT+CMEE=1", "ATI", "AT+CGSN"]
                .into_iter()
                .map(AtCmd::new)
                .collect(),
        });
        self.next_poll = now;
        // the data context may have outlived a core restart
        if self.run.is_none() {
            let _ = self.spawn(Verb::DataStatus, now);
        }
    }

    /// The AT channel went away: back to "not ready", look for it again later.
    fn lost(&mut self, why: &str, now: Instant) {
        eprintln!("walpad-core: modem: {why}");
        self.st.modem = ModemState::Present;
        self.st.reason = Some(why.into());
        self.clear_network();
        self.st.radio_on = None;
        self.st.sim = SimState::Unknown;
        self.st.data = MobileData::Off;
        self.st.ip = None;
        self.queue.clear();
        self.next_check = now + self.t.recheck;
    }

    fn clear_network(&mut self) {
        self.st.operator = None;
        self.st.tech = None;
        self.st.signal_bars = None;
        self.st.signal_dbm = None;
        self.st.registration = if self.st.radio_on == Some(false) {
            MobileReg::NotRegistered
        } else {
            MobileReg::Unknown
        };
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st.clone();
        while let Some(r) = self.worker.as_ref().map(|w| w.try_recv()) {
            match r {
                Recv::Done(d) => {
                    self.inflight = false;
                    self.handle(d, now);
                }
                Recv::Empty => break,
                Recv::Gone => {
                    self.worker = None;
                    self.inflight = false;
                    break;
                }
            }
        }
        self.tick_run(now);
        self.poll_noti();
        if matches!(self.st.modem, ModemState::Present | ModemState::Failed)
            && self.run.is_none()
            && now >= self.next_check
        {
            self.next_check = now + self.t.recheck;
            if self.tty.exists() {
                self.become_ready(now);
            }
        }
        self.phone.set_gate(self.gate(), now);
        self.phone.tick(now);
        self.dispatch(now);
        let phone_changed = self.phone.take_dirty();
        self.st != before || phone_changed
    }
}
