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
pub mod parse;

use at::{AtCmd, AtError, Done, Final, Job, Outcome, Recv, Worker};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{
    valid_apn, valid_sim_pin, MobileData, MobileReg, MobileStatus, ModemState, Secret, SimState,
};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-modem";
pub const TTY: &str = "/dev/radio/pttycmd1";
pub const CONF: &str = "/etc/walpad/mobile.conf";
/// The owner's radio switch: `0` = off (airplane), anything else or missing = on.
pub const RADIO_FILE: &str = "/var/lib/walpad/mobile-radio";
/// /run is a tmpfs: present = `walpad-modem start` was already tried this boot.
pub const START_MARK: &str = "/run/walpad/modem-start-attempted";
const OUT: &str = "/run/walpad/modem-out";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Deadline of one AT command.
    pub at_cmd: Duration,
    /// Poll period while the radio is on / off.
    pub poll_on: Duration,
    pub poll_off: Duration,
    pub start: Duration,
    pub data: Duration,
    pub status: Duration,
    /// Look for the AT channel again this often while the modem is not ready.
    pub recheck: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            at_cmd: Duration::from_secs(5),
            poll_on: Duration::from_secs(10),
            poll_off: Duration::from_secs(30),
            start: Duration::from_secs(120),
            data: Duration::from_secs(40),
            status: Duration::from_secs(10),
            recheck: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// ATE0, AT+CMEE=1, ATI, AT+CGSN.
    Init,
    /// AT+CFUN?, AT+CPIN?, [AT+CCID], [CSQ, CREG?, CGREG?, CEREG?, COPS=3,0, COPS?].
    Poll {
        ccid: bool,
        full: bool,
    },
    Radio(bool),
    Pin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Start,
    DataOn,
    DataOff,
    DataStatus,
}

impl Verb {
    fn args(self) -> &'static [&'static str] {
        match self {
            Verb::Start => &["start"],
            Verb::DataOn => &["data", "on"],
            Verb::DataOff => &["data", "off"],
            Verb::DataStatus => &["data", "status"],
        }
    }
}

struct Run {
    verb: Verb,
    child: Child,
    deadline: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MobileError {
    Busy(String),
    Refused(String),
    Invalid(String),
    Failed(String),
}

pub struct Modem {
    root: Root,
    script: PathBuf,
    tty: PathBuf,
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
}

/// One `key=value` line of the script's output.
fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// The APN in mobile.conf (`apn=<value>`), if valid.
pub fn parse_conf(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("apn="))
        .map(str::trim)
        .find(|a| valid_apn(a))
        .map(String::from)
}

fn ok_final(o: Option<&Outcome>) -> bool {
    matches!(o, Some(Ok(r)) if r.fin == Final::Ok)
}

impl Modem {
    pub fn new(root: &Root) -> Modem {
        let now = Instant::now();
        let mut m = Modem {
            root: root.clone(),
            script: root.p(SCRIPT),
            tty: root.p(TTY),
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
        std::mem::take(&mut self.notices)
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

    // ------------------------------------------------------------------ script
    fn spawn(&mut self, verb: Verb, now: Instant) -> Result<(), String> {
        if let Some(d) = self.out.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let out = std::fs::File::create(&self.out).map_err(|e| format!("output file: {e}"))?;
        let err = out.try_clone().map_err(|e| format!("output file: {e}"))?;
        let mut cmd = Command::new(&self.script);
        cmd.args(verb.args())
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        if self.root.base() != std::path::Path::new("/") {
            cmd.env("WALPAD_MODEM_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-modem is not installed".to_string(),
            _ => format!("cannot run walpad-modem: {e}"),
        })?;
        let t = match verb {
            Verb::Start => self.t.start,
            Verb::DataOn | Verb::DataOff => self.t.data,
            Verb::DataStatus => self.t.status,
        };
        self.run = Some(Run {
            verb,
            child,
            deadline: now + t,
        });
        Ok(())
    }

    fn kill(mut r: Run) {
        if let Some(pid) = rustix::process::Pid::from_raw(r.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    }

    fn reason_of(out: &str, code: Option<i32>) -> String {
        if let Some(r) = field(out, "reason") {
            return parse::clean(r, 120);
        }
        // `walpad-modem start` speaks in "[modem] ..." lines: the last one says how it ended
        if let Some(l) = out.lines().rev().find_map(|l| l.strip_prefix("[modem] ")) {
            return parse::clean(l, 120);
        }
        match code {
            Some(c) => format!("walpad-modem failed (exit {c})"),
            None => "walpad-modem was killed".into(),
        }
    }

    fn finish(&mut self, verb: Verb, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let state = field(&out, "state");
        match verb {
            Verb::Start => {
                if self.tty.exists() {
                    self.become_ready(now);
                } else {
                    let why = Self::reason_of(&out, code);
                    self.fail(format!("the modem did not start: {why}"));
                    self.next_check = now + self.t.recheck;
                }
            }
            Verb::DataOn | Verb::DataOff | Verb::DataStatus => {
                let ip = field(&out, "ip").map(|s| parse::clean(s, 45));
                let apn = field(&out, "apn")
                    .filter(|a| valid_apn(a))
                    .map(String::from);
                match (verb, code, state) {
                    (Verb::DataStatus, c, _) if c != Some(0) => return, // older script
                    (_, Some(0), Some("on")) => {
                        self.st.data = MobileData::On;
                        self.st.data_reason = None;
                        self.st.ip = ip;
                        self.st.apn_in_use = apn;
                    }
                    (_, Some(0), Some("off") | None) if verb != Verb::DataOn => {
                        self.st.data = MobileData::Off;
                        self.st.data_reason = None;
                        self.st.ip = None;
                    }
                    _ => {
                        self.st.data = MobileData::Failed;
                        self.st.data_reason = Some(Self::reason_of(&out, code));
                        self.st.ip = ip;
                        if apn.is_some() {
                            self.st.apn_in_use = apn;
                        }
                    }
                }
                if verb != Verb::DataStatus {
                    eprintln!(
                        "walpad-core: mobile data {} -> {:?}{}",
                        if verb == Verb::DataOn { "on" } else { "off" },
                        self.st.data,
                        self.st
                            .data_reason
                            .as_deref()
                            .map(|r| format!(" ({r})"))
                            .unwrap_or_default()
                    );
                    let n = json!({"kind": "mobile_data", "state": self.st.data,
                        "reason": self.st.data_reason, "ip": self.st.ip});
                    self.notice(n);
                }
                self.next_poll = now;
            }
        }
    }

    fn tick_run(&mut self, now: Instant) {
        let Some(mut r) = self.run.take() else {
            return;
        };
        match r.child.try_wait() {
            Ok(Some(es)) => self.finish(r.verb, es.code(), now),
            Ok(None) if now >= r.deadline => {
                let verb = r.verb;
                Self::kill(r);
                match verb {
                    Verb::Start => {
                        if self.tty.exists() {
                            self.become_ready(now)
                        } else {
                            self.fail("walpad-modem start timed out".into());
                            self.next_check = now + self.t.recheck;
                        }
                    }
                    Verb::DataOn | Verb::DataOff => {
                        self.st.data = MobileData::Failed;
                        self.st.data_reason = Some("walpad-modem data timed out".into());
                        let n = json!({"kind": "mobile_data", "state": self.st.data,
                            "reason": self.st.data_reason, "ip": null});
                        self.notice(n);
                    }
                    Verb::DataStatus => {}
                }
            }
            Ok(None) => self.run = Some(r),
            Err(_) => Self::kill(r),
        }
    }

    // ------------------------------------------------------------------ AT jobs
    fn poll_job(&self) -> Job<JobKind> {
        let ccid = self.st.sim == SimState::Ready && self.iccid.is_none();
        let full = self.st.radio_on == Some(true) && self.st.sim == SimState::Ready;
        let mut cmds = vec![AtCmd::new("AT+CFUN?"), AtCmd::new("AT+CPIN?")];
        if ccid {
            cmds.push(AtCmd::new("AT+CCID"));
        }
        if full {
            for c in [
                "AT+CSQ",
                "AT+CREG?",
                "AT+CGREG?",
                "AT+CEREG?",
                "AT+COPS=3,0",
                "AT+COPS?",
            ] {
                cmds.push(AtCmd::new(c));
            }
        }
        Job {
            kind: JobKind::Poll { ccid, full },
            cmds,
        }
    }

    fn dispatch(&mut self, now: Instant) {
        if self.run.is_some() || self.inflight || self.st.modem != ModemState::Ready {
            return;
        }
        if let Some(on) = self.pending_data.take() {
            if let Err(e) = self.spawn(if on { Verb::DataOn } else { Verb::DataOff }, now) {
                self.st.data = MobileData::Failed;
                self.st.data_reason = Some(e);
            }
            return;
        }
        let job = match self.queue.pop_front() {
            Some(j) => j,
            None if now >= self.next_poll => {
                // pushed back again when the result arrives; this guards against a stuck worker
                self.next_poll = now + self.t.poll_on.max(self.t.at_cmd * 10);
                self.poll_job()
            }
            None => return,
        };
        match &self.worker {
            Some(w) if w.send(job) => self.inflight = true,
            _ => {
                self.worker = None;
                self.fail("the AT worker stopped".into());
            }
        }
    }

    fn handle(&mut self, d: Done<JobKind>, now: Instant) {
        if let Some(Err(AtError::Open(e))) = d.results.first() {
            let why = format!("AT channel unavailable ({e})");
            if let JobKind::Radio(_) | JobKind::Pin = d.kind {
                self.notice(json!({"kind": job_notice(d.kind), "ok": false, "error": "the modem is not available"}));
            }
            return self.lost(&why, now);
        }
        let timed_out = d.results.contains(&Err(AtError::Timeout));
        match d.kind {
            JobKind::Init => {
                let r = |i: usize| d.results.get(i).and_then(|o| o.as_ref().ok());
                if let Some(f) = r(2).and_then(parse::ati) {
                    self.st.firmware = Some(f);
                }
                if let Some(i) = r(3).and_then(parse::imei) {
                    eprintln!("walpad-core: modem IMEI {}", parse::mask(&i));
                    self.imei = Some(i);
                }
            }
            JobKind::Poll { ccid, full } => self.handle_poll(&d.results, ccid, full, now),
            JobKind::Radio(on) => {
                let ok = ok_final(d.results.first());
                if ok {
                    self.st.radio_on = Some(on);
                    if !on {
                        self.clear_network();
                        if matches!(self.st.data, MobileData::On | MobileData::Connecting) {
                            self.pending_data = Some(false);
                        }
                    }
                }
                eprintln!(
                    "walpad-core: mobile radio {} {}",
                    if on { "on" } else { "off" },
                    if ok { "done" } else { "failed" }
                );
                let err = (!ok).then(|| at_error_text(d.results.first()));
                self.notice(json!({"kind": "mobile_radio", "on": on, "ok": ok, "error": err}));
                self.next_poll = now;
            }
            JobKind::Pin => {
                let (ok, err) = match d.results.first() {
                    Some(Ok(r)) if r.fin == Final::Ok => (true, None),
                    Some(Ok(Reply {
                        fin: Final::Cme(e), ..
                    })) => (false, Some(pin_error(e))),
                    other => (false, Some(at_error_text(other))),
                };
                // never the PIN, only the outcome
                eprintln!(
                    "walpad-core: SIM PIN {}",
                    if ok { "accepted" } else { "rejected" }
                );
                self.notice(json!({"kind": "mobile_pin", "ok": ok, "error": err}));
                self.next_poll = now;
            }
        }
        if timed_out {
            self.st.reason = Some("the modem does not answer (AT timeout)".into());
        } else if self.st.modem == ModemState::Ready {
            self.st.reason = None;
        }
    }

    fn handle_poll(&mut self, results: &[Outcome], ccid: bool, full: bool, now: Instant) {
        let mut it = results.iter();
        let mut next = || it.next().and_then(|o| o.as_ref().ok());
        let cfun = next().and_then(parse::cfun);
        let sim = next().map(parse::cpin);
        if cfun.is_some() {
            self.st.radio_on = cfun;
        }
        let old_sim = self.st.sim;
        if let Some(s) = sim {
            self.st.sim = s;
        }
        if ccid {
            if let Some(c) = next().and_then(parse::iccid) {
                self.iccid = Some(c);
            }
        }
        let usable = self.st.radio_on == Some(true) && self.st.sim == SimState::Ready;
        if full && usable {
            let sig = next().and_then(parse::csq);
            let regs = [
                next().and_then(|r| parse::reg(r, "+CREG:")),
                next().and_then(|r| parse::reg(r, "+CGREG:")),
                next().and_then(|r| parse::reg(r, "+CEREG:")),
            ];
            let _set_format = next();
            let (oper, tech) = next().map(parse::cops).unwrap_or((None, None));
            self.st.signal_bars = sig.map(|s| s.0);
            self.st.signal_dbm = sig.map(|s| s.1);
            self.st.registration = parse::combine_reg(&regs);
            let registered = matches!(self.st.registration, MobileReg::Home | MobileReg::Roaming);
            self.st.operator = oper.filter(|_| registered);
            self.st.tech = tech.filter(|_| registered).map(String::from);
        } else if !usable {
            self.clear_network();
        }
        if self.st.sim != old_sim {
            eprintln!("walpad-core: SIM {:?} -> {:?}", old_sim, self.st.sim);
            if self.st.sim != SimState::Ready {
                self.iccid = None;
            }
        }
        // apply the persisted radio choice once the modem told us where it stands
        if !self.pref_applied && self.st.radio_on.is_some() {
            self.pref_applied = true;
            if self.st.radio_on != Some(self.radio_pref) {
                self.queue_radio(self.radio_pref);
            }
        }
        // a SIM change, or a usable SIM seen by a short poll: the full poll follows at once
        self.next_poll = if self.st.sim != old_sim || (usable && !full) {
            now
        } else if self.st.radio_on == Some(true) {
            now + self.t.poll_on
        } else {
            now + self.t.poll_off
        };
    }

    fn queue_radio(&mut self, on: bool) {
        self.queue.retain(|j| !matches!(j.kind, JobKind::Radio(_)));
        self.queue.push_back(Job {
            kind: JobKind::Radio(on),
            cmds: vec![AtCmd::new(if on { "AT+CFUN=1" } else { "AT+CFUN=0" })],
        });
    }

    /// Advance the worker and the script. Returns true when the status changed.
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
        if matches!(self.st.modem, ModemState::Present | ModemState::Failed)
            && self.run.is_none()
            && now >= self.next_check
        {
            self.next_check = now + self.t.recheck;
            if self.tty.exists() {
                self.become_ready(now);
            }
        }
        self.dispatch(now);
        self.st != before
    }

    // ------------------------------------------------------------------ commands
    /// `mobile_radio {on}`: persist the choice; send AT+CFUN when the modem is ready.
    pub fn set_radio(&mut self, on: bool, now: Instant) -> Result<Value, MobileError> {
        if self.st.modem == ModemState::Absent {
            return Err(MobileError::Refused(
                "this system has no modem support".into(),
            ));
        }
        if let Some(d) = self.radio_file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        crate::hotspot::write_600(&self.radio_file, if on { b"1\n" } else { b"0\n" })
            .map_err(|e| MobileError::Failed(format!("cannot store the radio switch: {e}")))?;
        self.radio_pref = on;
        let ready = self.st.modem == ModemState::Ready;
        if ready {
            self.pref_applied = true;
            self.queue_radio(on);
            self.dispatch(now);
        }
        Ok(json!({"on": on, "applied": ready}))
    }

    /// `mobile_data {on}`.
    pub fn set_data(&mut self, on: bool, now: Instant) -> Result<Value, MobileError> {
        if self.st.modem != ModemState::Ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        if self.pending_data.is_some()
            || self
                .run
                .as_ref()
                .is_some_and(|r| matches!(r.verb, Verb::DataOn | Verb::DataOff))
        {
            return Err(MobileError::Busy("mobile data is switching".into()));
        }
        if on {
            if self.st.radio_on != Some(true) {
                return Err(MobileError::Refused(
                    "turn the mobile radio on first".into(),
                ));
            }
            if self.st.sim != SimState::Ready {
                return Err(MobileError::Refused("no usable SIM card".into()));
            }
            self.st.data = MobileData::Connecting;
        }
        self.st.data_reason = None;
        if self.run.is_some() || self.inflight {
            // a status run or an AT job holds the channel: start right after it
            self.pending_data = Some(on);
        } else {
            self.spawn(if on { Verb::DataOn } else { Verb::DataOff }, now)
                .map_err(|e| {
                    self.st.data = MobileData::Failed;
                    self.st.data_reason = Some(e.clone());
                    MobileError::Failed(e)
                })?;
        }
        Ok(json!({"data": self.st.data}))
    }

    /// `mobile_apn {apn}`: stored in /etc/walpad/mobile.conf (0600); used by the next `data on`.
    pub fn set_apn(&mut self, apn: &str) -> Result<Value, MobileError> {
        if !valid_apn(apn) {
            return Err(MobileError::Invalid(
                "apn must be 1-62 characters of A-Z a-z 0-9 . - _".into(),
            ));
        }
        let text = format!(
            "# walpad-core mobile_apn (owner request 2026-11-10); read by walpad-modem data on\napn={apn}\n"
        );
        crate::hotspot::write_600(&self.conf, text.as_bytes())
            .map_err(|e| MobileError::Failed(format!("cannot store the APN: {e}")))?;
        self.st.apn = Some(apn.to_string());
        eprintln!("walpad-core: mobile APN set to {apn}");
        Ok(json!({"apn": apn, "reconnect": self.st.data == MobileData::On}))
    }

    /// `sim_pin {pin}`: AT+CPIN="<pin>" over the channel only.
    pub fn sim_pin(&mut self, pin: &Secret, now: Instant) -> Result<Value, MobileError> {
        if !valid_sim_pin(pin.expose()) {
            return Err(MobileError::Invalid("pin must be 4-8 digits".into()));
        }
        if self.st.modem != ModemState::Ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        match self.st.sim {
            SimState::PinRequired => {}
            SimState::PukRequired => {
                return Err(MobileError::Refused(
                    "the SIM is locked with a PUK: unlock it in a phone first".into(),
                ))
            }
            _ => {
                return Err(MobileError::Refused(
                    "the SIM does not ask for a PIN".into(),
                ))
            }
        }
        if self.queue.iter().any(|j| j.kind == JobKind::Pin) {
            return Err(MobileError::Busy("a PIN is being checked".into()));
        }
        self.queue.push_back(Job {
            kind: JobKind::Pin,
            cmds: vec![AtCmd::secret(format!("AT+CPIN=\"{}\"", pin.expose()))],
        });
        self.dispatch(now);
        Ok(json!({"accepted": true}))
    }

    /// `mobile_info`: identifiers on request only (never in status, never logged).
    pub fn info(&self) -> Value {
        json!({
            "modem": self.st.modem,
            "firmware": self.st.firmware,
            "imei": self.imei,
            "iccid": self.iccid,
        })
    }
}

use at::Reply;

fn job_notice(k: JobKind) -> &'static str {
    match k {
        JobKind::Radio(_) => "mobile_radio",
        _ => "mobile_pin",
    }
}

fn at_error_text(o: Option<&Outcome>) -> String {
    match o {
        Some(Err(AtError::Timeout)) => "the modem did not answer".into(),
        Some(Err(AtError::Open(_))) => "the modem is not available".into(),
        Some(Err(AtError::Io(e))) => format!("modem I/O error: {}", parse::clean(e, 60)),
        Some(Ok(Reply {
            fin: Final::Cme(e), ..
        })) => format!("modem error {}", parse::clean(e, 40)),
        _ => "the modem refused the command".into(),
    }
}

fn pin_error(e: &str) -> String {
    let t = e.trim().to_ascii_lowercase();
    if t == "16" || t.contains("incorrect password") {
        "Wrong PIN".into()
    } else if t == "12" || t.contains("puk") {
        "SIM locked: PUK required".into()
    } else {
        format!("PIN not accepted (error {})", parse::clean(e, 30))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_and_output_fields() {
        assert_eq!(parse_conf(""), None);
        assert_eq!(
            parse_conf("# x\napn=gpinternet\n").as_deref(),
            Some("gpinternet")
        );
        assert_eq!(parse_conf("apn=bad apn\n"), None);
        let out =
            "[modem] data on\nstate=failed\nreason=data link not supported yet\nip=10.1.2.3\n";
        assert_eq!(field(out, "state"), Some("failed"));
        assert_eq!(
            Modem::reason_of(out, Some(1)),
            "data link not supported yet"
        );
        assert_eq!(
            Modem::reason_of("[modem] after 60s: md_en=0\n", Some(0)),
            "after 60s: md_en=0"
        );
        assert_eq!(
            Modem::reason_of("", Some(3)),
            "walpad-modem failed (exit 3)"
        );
    }

    #[test]
    fn pin_errors_are_friendly() {
        assert_eq!(pin_error("16"), "Wrong PIN");
        assert_eq!(pin_error("incorrect password"), "Wrong PIN");
        assert_eq!(pin_error("12"), "SIM locked: PUK required");
        assert_eq!(pin_error("100"), "PIN not accepted (error 100)");
    }

    fn reply(lines: &[&str], fin: Final) -> Outcome {
        Ok(Reply {
            lines: lines.iter().map(|s| s.to_string()).collect(),
            fin,
        })
    }

    fn ready_modem() -> Modem {
        let d = std::env::temp_dir().join(format!("walpad-modem-unit-{}", std::process::id()));
        let mut m = Modem::new(&Root::new(&d));
        m.st.modem = ModemState::Ready;
        m
    }

    #[test]
    fn polls_build_from_the_known_state_and_results_fill_the_status() {
        let now = Instant::now();
        let mut m = ready_modem();
        let j = m.poll_job();
        assert_eq!(
            j.kind,
            JobKind::Poll {
                ccid: false,
                full: false
            }
        );
        assert_eq!(j.cmds.len(), 2);
        // radio off at boot, SIM ready: a quick follow-up poll, the radio switch is queued
        m.handle_poll(
            &[
                reply(&["+CFUN: 0"], Final::Ok),
                reply(&["+CPIN: READY"], Final::Ok),
            ],
            false,
            false,
            now,
        );
        assert_eq!(m.st.radio_on, Some(false));
        assert_eq!(m.st.sim, SimState::Ready);
        assert_eq!(m.st.registration, MobileReg::NotRegistered);
        assert_eq!(m.next_poll, now, "SIM changed: poll again at once");
        assert!(matches!(
            m.queue.back().map(|j| j.kind),
            Some(JobKind::Radio(true))
        ));
        m.st.radio_on = Some(true);
        let j = m.poll_job();
        assert_eq!(
            j.kind,
            JobKind::Poll {
                ccid: true,
                full: true
            }
        );
        assert_eq!(j.cmds.len(), 9);
        m.handle_poll(
            &[
                reply(&["+CFUN: 1"], Final::Ok),
                reply(&["+EIND: 64", "+CPIN: READY"], Final::Ok),
                reply(&["+CCID: \"8988012345678901234F\""], Final::Ok),
                reply(&["+CSQ: 29, 5"], Final::Ok),
                reply(&["+CREG: 0,1"], Final::Ok),
                reply(&["+CGREG: 0,1"], Final::Ok),
                reply(&["+CEREG: 0,1"], Final::Ok),
                reply(&[], Final::Ok),
                reply(&["+COPS: 0,0,\"Grameenphone\",3"], Final::Ok),
            ],
            true,
            true,
            now,
        );
        assert_eq!(m.st.signal_bars, Some(4));
        assert_eq!(m.st.signal_dbm, Some(-55));
        assert_eq!(m.st.registration, MobileReg::Home);
        assert_eq!(m.st.operator.as_deref(), Some("Grameenphone"));
        assert_eq!(m.st.tech.as_deref(), Some("2G"));
        assert!(m.iccid.is_some());
        assert_eq!(m.next_poll, now + m.t.poll_on);
        // the SIM is pulled: the network facts go, the ICCID is forgotten
        m.handle_poll(
            &[
                reply(&["+CFUN: 1"], Final::Ok),
                reply(&[], Final::Cme("10".into())),
            ],
            false,
            false,
            now,
        );
        assert_eq!(m.st.sim, SimState::Absent);
        assert_eq!(m.st.operator, None);
        assert_eq!(m.st.signal_bars, None);
        assert!(m.iccid.is_none());
        // a timeout half way keeps what is known and stays honest about the rest
        m.st.sim = SimState::Ready;
        m.handle_poll(
            &[reply(&["+CFUN: 1"], Final::Ok), Err(AtError::Timeout)],
            false,
            true,
            now,
        );
        assert_eq!(m.st.sim, SimState::Ready);
        assert_eq!(m.st.signal_bars, None);
        assert_eq!(m.st.registration, MobileReg::Unknown);
    }

    #[test]
    fn data_needs_a_ready_modem_radio_and_sim() {
        let now = Instant::now();
        let mut m = ready_modem();
        m.st.modem = ModemState::Present;
        assert!(matches!(
            m.set_data(true, now),
            Err(MobileError::Refused(_))
        ));
        m.st.modem = ModemState::Ready;
        m.st.radio_on = Some(false);
        assert!(matches!(
            m.set_data(true, now),
            Err(MobileError::Refused(_))
        ));
        m.st.radio_on = Some(true);
        m.st.sim = SimState::PinRequired;
        assert!(matches!(
            m.set_data(true, now),
            Err(MobileError::Refused(_))
        ));
        m.st.sim = SimState::Ready;
        m.inflight = true;
        assert_eq!(m.set_data(true, now).unwrap()["data"], "connecting");
        assert_eq!(m.pending_data, Some(true));
        assert!(matches!(m.set_data(false, now), Err(MobileError::Busy(_))));
    }

    #[test]
    fn pin_is_only_sent_when_the_sim_asks_and_never_printed() {
        let now = Instant::now();
        let mut m = ready_modem();
        let pin = Secret::new("4711");
        m.st.sim = SimState::Ready;
        assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Refused(_))));
        m.st.sim = SimState::PukRequired;
        assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Refused(_))));
        m.st.sim = SimState::PinRequired;
        m.inflight = true; // keep it queued
        assert!(m.sim_pin(&pin, now).is_ok());
        assert!(matches!(m.sim_pin(&pin, now), Err(MobileError::Busy(_))));
        let dbg = format!("{:?}", m.queue.back().map(|j| &j.cmds));
        assert!(
            dbg.contains("AT+CPIN=***") && !dbg.contains("4711"),
            "{dbg}"
        );
        assert!(matches!(
            m.sim_pin(&Secret::new("12\"4"), now),
            Err(MobileError::Invalid(_))
        ));
    }
}
