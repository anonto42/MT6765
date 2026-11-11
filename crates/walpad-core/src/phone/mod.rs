//! Voice calls, text messages, contacts and the call log (owner request 2026-11-11, change
//! `phone-dialer`).
//!
//! * One AT channel: this module owns no tty. It only builds jobs for the modem module's worker
//!   (`modem::Modem::dispatch` asks `Phone::next_job`) and digests the results (`Phone::handle`),
//!   so the channel keeps exactly one opener and every command keeps its deadline.
//! * Signalling is polled: `AT+CLCC` every 1.5 s (1 s during a call). The channel is closed
//!   between jobs, so URCs (`RING`, `NO CARRIER`, `+CMTI`) that arrive between polls are lost;
//!   the polled call list makes the state machine independent of them. URCs that slip into a
//!   reply are used as hints.
//! * Messages are handled in PDU mode (`AT+CMGF=0`): GSM 7-bit, UCS2 for Bangla, long messages
//!   as concatenated parts. Received messages are copied into /var/lib/walpad/sms.json and then
//!   deleted from the SIM; polled every 20 s and after every call.
//! * Call AUDIO is not here: `walpad-callaudio start|stop` flips the codec mixer (experimental,
//!   unverified on the device) and is run as a bounded child process.
//! * Private: numbers, names and message text are never logged (only counts and outcomes).
pub mod parse;
pub mod pdu;
pub mod store;

use crate::modem::at::{AtCmd, AtError, Final, Job, Outcome, Reply};
use crate::modem::{JobKind, MobileError};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use store::{CallLog, CallRec, ContactStore, Sms, SmsStore};
use walpad_hw::Root;
use walpad_proto::{CallState, PhoneStatus};

pub const CALLAUDIO: &str = "/usr/local/sbin/walpad-callaudio";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Numbers that always dial, with or without a SIM (Bangladesh 999 / 199, EU 112, US 911).
const EMERGENCY: &[&str] = &["112", "999", "199", "911", "000"];

pub fn is_emergency(number: &str) -> bool {
    EMERGENCY.contains(&number.trim())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub clcc_idle: Duration,
    pub clcc_call: Duration,
    pub sms_poll: Duration,
    /// How long a finished call stays on screen as `ended`.
    pub ended_show: Duration,
    /// A dialled call not yet in the call list is given this long before it counts as failed.
    pub dial_grace: Duration,
    pub setup_retry: Duration,
    pub audio: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            clcc_idle: Duration::from_millis(1500),
            clcc_call: Duration::from_millis(1000),
            sms_poll: Duration::from_secs(20),
            ended_show: Duration::from_secs(3),
            dial_grace: Duration::from_secs(6),
            setup_retry: Duration::from_secs(15),
            audio: Duration::from_secs(10),
        }
    }
}

/// What the modem module knows about the line, refreshed before every dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    pub ready: bool,
    pub radio_on: bool,
    pub sim_ready: bool,
    pub registered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhoneKind {
    Setup,
    Clcc,
    Dial,
    Answer,
    Hangup,
    Dtmf,
    Mute,
    SmsSend,
    SmsPoll,
    SmsDelete,
}

enum Op {
    Setup,
    Clcc,
    Dial(String),
    Answer,
    Hangup { ath: bool },
    Dtmf(char),
    Mute(bool),
    SmsSend { id: u64, parts: Vec<pdu::Part> },
    SmsPoll,
    SmsDelete(Vec<u32>),
}

impl Op {
    fn kind(&self) -> PhoneKind {
        match self {
            Op::Setup => PhoneKind::Setup,
            Op::Clcc => PhoneKind::Clcc,
            Op::Dial(_) => PhoneKind::Dial,
            Op::Answer => PhoneKind::Answer,
            Op::Hangup { .. } => PhoneKind::Hangup,
            Op::Dtmf(_) => PhoneKind::Dtmf,
            Op::Mute(_) => PhoneKind::Mute,
            Op::SmsSend { .. } => PhoneKind::SmsSend,
            Op::SmsPoll => PhoneKind::SmsPoll,
            Op::SmsDelete(_) => PhoneKind::SmsDelete,
        }
    }

    fn cmds(&self) -> Vec<AtCmd> {
        match self {
            Op::Setup => [
                "AT+CMGF=0",
                "AT+CPMS=\"SM\",\"SM\",\"SM\"",
                "AT+CNMI=2,1,0,0,0",
                "AT+CLIP=1",
                "AT+CSCA?",
            ]
            .into_iter()
            .map(AtCmd::new)
            .collect(),
            Op::Clcc => vec![AtCmd::new("AT+CLCC")],
            Op::Dial(n) => vec![AtCmd::secret(format!("ATD{n};"))],
            Op::Answer => vec![AtCmd::new("ATA")],
            Op::Hangup { ath: false } => vec![AtCmd::new("AT+CHUP")],
            Op::Hangup { ath: true } => vec![AtCmd::new("ATH")],
            Op::Dtmf(c) => vec![AtCmd::new(format!("AT+VTS={c}"))],
            Op::Mute(on) => vec![AtCmd::new(format!("AT+CMUT={}", u8::from(*on)))],
            Op::SmsSend { parts, .. } => parts
                .iter()
                .map(|p| {
                    AtCmd::with_payload(
                        format!("AT+CMGS={}", p.tpdu_len),
                        p.hex.clone(),
                        // submitting to the network is slower than a status query
                        Duration::from_secs(20),
                    )
                })
                .collect(),
            Op::SmsPoll => vec![AtCmd::new("AT+CMGL=4").timeout(Duration::from_secs(10))],
            Op::SmsDelete(ix) => ix
                .iter()
                .map(|i| AtCmd::new(format!("AT+CMGD={i}")))
                .collect(),
        }
    }
}

struct Call {
    number: Option<String>,
    incoming: bool,
    state: CallState,
    dialed_at: Instant,
    /// The call list showed it at least once.
    seen: bool,
    answered_at: Option<Instant>,
    /// Why the owner ended it (`hangup` / `rejected`), when they did.
    hangup: Option<&'static str>,
    /// Q.931 cause of the disconnect, when the modem told us.
    cause: Option<u16>,
    started_ms: u64,
}

struct Ended {
    until: Instant,
    reason: String,
    number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioWant {
    Start(bool),
    Stop,
}

pub struct Phone {
    t: Timing,
    gate: Gate,
    sms: SmsStore,
    contacts: ContactStore,
    calls: CallLog,
    script: PathBuf,
    root_base: PathBuf,
    queue: VecDeque<Op>,
    inflight: Option<Op>,
    setup_done: bool,
    next_setup: Instant,
    next_clcc: Instant,
    next_sms: Instant,
    call: Option<Call>,
    ended: Option<Ended>,
    muted: bool,
    speaker: bool,
    smsc: Option<String>,
    next_ref: u8,
    notices: Vec<Value>,
    dirty: bool,
    audio_want: Option<AudioWant>,
    audio_on: bool,
    audio_run: Option<(Child, Instant)>,
}

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn ok_final(o: Option<&Outcome>) -> bool {
    matches!(o, Some(Ok(r)) if r.fin == Final::Ok)
}

fn reply(o: Option<&Outcome>) -> Option<&Reply> {
    o.and_then(|o| o.as_ref().ok())
}

impl Phone {
    pub fn new(root: &Root) -> Phone {
        let now = Instant::now();
        Phone {
            t: Timing::default(),
            gate: Gate::default(),
            sms: SmsStore::load(root.p(store::SMS_FILE)),
            contacts: ContactStore::load(root.p(store::CONTACTS_FILE)),
            calls: CallLog::load(root.p(store::CALLS_FILE)),
            script: root.p(CALLAUDIO),
            root_base: root.base().to_path_buf(),
            queue: VecDeque::new(),
            inflight: None,
            setup_done: false,
            next_setup: now,
            next_clcc: now,
            next_sms: now,
            call: None,
            ended: None,
            muted: false,
            speaker: false,
            smsc: None,
            next_ref: (wall_ms() % 251) as u8 + 1,
            notices: Vec::new(),
            dirty: false,
            audio_want: None,
            audio_on: false,
            audio_run: None,
        }
    }

    pub fn set_timing(&mut self, t: Timing) {
        self.t = t;
    }

    pub fn take_notices(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.notices)
    }

    /// True once after the status changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// A call is on (or ringing): the modem module skips its slow network poll meanwhile.
    pub fn in_call(&self) -> bool {
        self.call.is_some()
    }

    fn name_of(&self, number: Option<&str>) -> Option<String> {
        number
            .and_then(|n| self.contacts.name_of(n))
            .map(String::from)
    }

    fn notice(&mut self, v: Value) {
        self.notices.push(v);
    }

    // ---------------------------------------------------------------- gate and time
    /// The line state from the modem module. Losing the modem ends a call and drops queued work.
    pub fn set_gate(&mut self, g: Gate, now: Instant) {
        if g == self.gate {
            return;
        }
        let was_ready = self.gate.ready;
        self.gate = g;
        self.dirty = true;
        if was_ready && !g.ready {
            self.inflight = None;
            self.queue.clear();
            self.setup_done = false;
            self.next_setup = now;
            if self.call.is_some() {
                self.end_call(Some("failed"), now);
            }
        }
        if !g.sim_ready {
            self.setup_done = false;
        }
    }

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

    fn dial_result(&mut self, o: Option<&Outcome>, now: Instant) {
        let reason = match o {
            Some(Ok(r)) => match &r.fin {
                Final::Ok => {
                    self.next_clcc = now;
                    return;
                }
                Final::Busy => "busy",
                Final::NoAnswer => "no_answer",
                _ => "failed",
            },
            // no answer in 5 s: the call list decides
            Some(Err(AtError::Timeout)) => {
                self.next_clcc = now;
                return;
            }
            _ => "failed",
        };
        if self.call.as_ref().is_some_and(|c| !c.seen) {
            self.end_call(Some(reason), now);
        }
    }

    // ---------------------------------------------------------------- call state machine
    fn apply_clcc(&mut self, r: &Reply, now: Instant) {
        use parse::ClccStat as S;
        if parse::new_sms_urc(&r.lines) {
            self.next_sms = now;
        }
        if let (Some(c), Some(call)) = (parse::ecpi_cause(&r.lines), self.call.as_mut()) {
            call.cause = Some(c);
        }
        // a premature `NO CARRIER` URC can cut the reply short: not a call list
        if r.fin != Final::Ok {
            return;
        }
        let list = parse::clcc(&r.lines);
        let primary = list
            .iter()
            .find(|c| c.stat == S::Active)
            .or_else(|| list.iter().find(|c| c.stat != S::Waiting))
            .or_else(|| list.first());
        let Some(c) = primary else {
            if let Some(call) = &self.call {
                if call.seen || now.duration_since(call.dialed_at) >= self.t.dial_grace {
                    self.end_call(None, now);
                }
            }
            return;
        };
        let state = match c.stat {
            S::Active => CallState::Active,
            S::Held => CallState::Held,
            S::Dialing | S::Alerting => CallState::Dialing,
            S::Incoming | S::Waiting => CallState::Ringing,
        };
        if self.call.is_none() {
            // a new call the owner did not start here (incoming, or placed elsewhere)
            self.ended = None;
            self.muted = false;
            self.call = Some(Call {
                number: c.number.clone(),
                incoming: c.incoming,
                state,
                dialed_at: now,
                seen: true,
                answered_at: (state == CallState::Active).then_some(now),
                hangup: None,
                cause: None,
                started_ms: wall_ms(),
            });
            self.dirty = true;
            if c.incoming && state == CallState::Ringing {
                let name = self.name_of(c.number.as_deref());
                self.notice(json!({"kind": "incoming_call", "number": c.number, "name": name}));
                eprintln!("walpad-core: incoming call");
            }
            if state == CallState::Active {
                self.audio_start();
            }
            self.next_clcc = now + self.t.clcc_call;
            return;
        }
        let mut start_audio = false;
        if let Some(call) = self.call.as_mut() {
            call.seen = true;
            if call.number.is_none() {
                call.number = c.number.clone();
                self.dirty = true;
            }
            if call.state != state {
                call.state = state;
                self.dirty = true;
                if state == CallState::Active {
                    if call.answered_at.is_none() {
                        call.answered_at = Some(now);
                    }
                    start_audio = true;
                }
            }
        }
        if start_audio {
            self.audio_start();
        }
    }

    fn audio_start(&mut self) {
        self.audio_on = true;
        self.audio_want = Some(AudioWant::Start(self.speaker));
    }

    /// The call is over. `reason` None = work it out from what is known.
    fn end_call(&mut self, reason: Option<&str>, now: Instant) {
        let Some(call) = self.call.take() else {
            return;
        };
        let answered = call.answered_at;
        let dur = answered.map_or(0, |t| now.duration_since(t).as_secs());
        let reason: String = match reason {
            Some(r) => r.into(),
            None => match (call.hangup, call.cause) {
                (Some(h), _) => h.into(),
                (None, Some(c)) => parse::cause_reason(c).into(),
                (None, None) if answered.is_some() => "remote".into(),
                (None, None) if call.incoming => "missed".into(),
                (None, None) => "remote".into(),
            },
        };
        let kind = match (call.incoming, answered.is_some()) {
            (true, true) => "in",
            (true, false) => "missed",
            (false, _) => "out",
        };
        self.calls.add(CallRec {
            number: call.number.clone().unwrap_or_default(),
            ts: call.started_ms,
            dur_s: dur,
            kind: kind.into(),
        });
        let name = self.name_of(call.number.as_deref());
        self.notice(
            json!({"kind": "call_ended", "reason": reason, "incoming": call.incoming,
            "number": call.number, "name": name, "duration_s": dur}),
        );
        eprintln!("walpad-core: call ended ({reason}, {dur} s)");
        self.ended = Some(Ended {
            until: now + self.t.ended_show,
            reason,
            number: call.number,
        });
        self.muted = false;
        self.dirty = true;
        if self.audio_on {
            self.audio_on = false;
            self.audio_want = Some(AudioWant::Stop);
        }
        // a message may have arrived during the call
        self.next_sms = now;
        self.next_clcc = now + self.t.clcc_idle;
    }

    // ---------------------------------------------------------------- commands
    fn need_line(&self, emergency: bool) -> Result<(), MobileError> {
        let g = self.gate;
        if !g.ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        if !g.radio_on {
            return Err(MobileError::Refused(
                "turn the mobile radio on first (airplane mode)".into(),
            ));
        }
        if !emergency {
            if !g.sim_ready {
                return Err(MobileError::Refused("no usable SIM card".into()));
            }
            if !g.registered {
                return Err(MobileError::Refused(
                    "not registered on a mobile network".into(),
                ));
            }
        }
        Ok(())
    }

    /// `phone_dial`: strict number check, then `ATD<number>;`.
    pub fn dial(&mut self, number: &str, now: Instant) -> Result<Value, MobileError> {
        if !walpad_proto::valid_dial_number(number) {
            return Err(MobileError::Invalid(
                "number must be 1-32 characters of 0-9 + * # ,".into(),
            ));
        }
        if (number.starts_with('*') || number.starts_with('#')) && number.ends_with('#') {
            return Err(MobileError::Refused(
                "USSD codes (like *123#) are not supported yet".into(),
            ));
        }
        self.need_line(is_emergency(number))?;
        if self.call.is_some() {
            return Err(MobileError::Busy("a call is already in progress".into()));
        }
        self.ended = None;
        self.muted = false;
        self.call = Some(Call {
            number: Some(number.to_string()),
            incoming: false,
            state: CallState::Dialing,
            dialed_at: now,
            seen: false,
            answered_at: None,
            hangup: None,
            cause: None,
            started_ms: wall_ms(),
        });
        self.queue.push_front(Op::Dial(number.to_string()));
        self.dirty = true;
        eprintln!("walpad-core: dialling");
        Ok(json!({"state": "dialing"}))
    }

    pub fn answer(&mut self, _now: Instant) -> Result<Value, MobileError> {
        match &self.call {
            Some(c) if c.state == CallState::Ringing => {
                self.queue.push_front(Op::Answer);
                Ok(json!({"state": "answering"}))
            }
            _ => Err(MobileError::Refused("no call is ringing".into())),
        }
    }

    /// End the call, or reject the ringing one.
    pub fn hangup(&mut self, now: Instant) -> Result<Value, MobileError> {
        let Some(c) = self.call.as_mut() else {
            return Err(MobileError::Refused("no call".into()));
        };
        c.hangup = Some(if c.incoming && c.state == CallState::Ringing {
            "rejected"
        } else {
            "hangup"
        });
        // a dial still queued never goes out: nothing to hang up
        let queued = self.queue.len();
        self.queue.retain(|o| !matches!(o, Op::Dial(_)));
        if self.queue.len() != queued {
            self.end_call(None, now);
            return Ok(json!({"state": "ended"}));
        }
        self.queue.push_front(Op::Hangup { ath: false });
        Ok(json!({"state": "ending"}))
    }

    pub fn dtmf(&mut self, digit: char, _now: Instant) -> Result<Value, MobileError> {
        if !matches!(self.call.as_ref().map(|c| c.state), Some(CallState::Active)) {
            return Err(MobileError::Refused("no active call".into()));
        }
        if !(digit.is_ascii_digit() || digit == '*' || digit == '#') {
            return Err(MobileError::Invalid("digit must be 0-9 * #".into()));
        }
        // tones are queued in order behind each other
        self.queue.push_back(Op::Dtmf(digit));
        Ok(json!({}))
    }

    pub fn mute(&mut self, on: bool, _now: Instant) -> Result<Value, MobileError> {
        if !matches!(self.call.as_ref().map(|c| c.state), Some(CallState::Active)) {
            return Err(MobileError::Refused("no active call".into()));
        }
        self.queue.retain(|o| !matches!(o, Op::Mute(_)));
        self.queue.push_front(Op::Mute(on));
        Ok(json!({"on": on}))
    }

    /// Speaker or earpiece. The choice is kept for the next call; during a call the audio script
    /// switches the route (experimental).
    pub fn speaker(&mut self, on: bool, _now: Instant) -> Result<Value, MobileError> {
        self.speaker = on;
        self.dirty = true;
        if self.audio_on {
            self.audio_want = Some(AudioWant::Start(on));
        }
        Ok(json!({"on": on}))
    }

    pub fn sms_send(&mut self, number: &str, text: &str) -> Result<Value, MobileError> {
        if !walpad_proto::valid_sms_number(number) {
            return Err(MobileError::Invalid(
                "number must be 3-20 digits, optionally starting with +".into(),
            ));
        }
        self.need_line(false)?;
        let reference = self.next_ref;
        let parts = pdu::encode_submit(self.smsc.as_deref(), number, text, reference).map_err(
            |e| match e {
                pdu::EncodeError::TooLong => MobileError::Invalid(format!(
                    "the message is too long (at most {} parts)",
                    pdu::MAX_PARTS
                )),
                pdu::EncodeError::BadNumber => MobileError::Invalid("bad number".into()),
            },
        )?;
        self.next_ref = self.next_ref.wrapping_add(1).max(1);
        let n = parts.len();
        let id = self.sms.add(Sms {
            id: 0,
            number: number.to_string(),
            text: text.to_string(),
            ts: wall_ms(),
            incoming: false,
            read: true,
            state: "sending".into(),
        });
        self.queue.push_back(Op::SmsSend { id, parts });
        self.dirty = true;
        Ok(json!({"id": id, "parts": n}))
    }

    fn sms_result(&mut self, id: u64, parts: usize, results: &[Outcome]) {
        let all_ok = results.len() == parts && results.iter().all(|r| ok_final(Some(r)));
        let error = if all_ok {
            None
        } else {
            Some(match results.iter().find(|r| !ok_final(Some(r))) {
                Some(Err(AtError::Timeout)) => "the modem did not answer in time".to_string(),
                Some(Ok(Reply {
                    fin: Final::Cms(c), ..
                })) => format!(
                    "the network refused the message (error {})",
                    crate::modem::parse::clean(c, 12)
                ),
                _ => "the message could not be sent".to_string(),
            })
        };
        self.sms
            .set_state(id, if all_ok { "sent" } else { "failed" });
        self.dirty = true;
        eprintln!(
            "walpad-core: SMS {}",
            if all_ok { "sent" } else { "failed" }
        );
        self.notice(json!({"kind": "sms_sent", "id": id, "ok": all_ok, "error": error}));
    }

    /// Copy the messages the SIM holds into the store, then delete them from the SIM.
    fn import_sms(&mut self, r: &Reply, _now: Instant) {
        let listed = parse::cmgl(&r.lines);
        if listed.is_empty() {
            return;
        }
        struct Piece {
            index: u32,
            read: bool,
            d: pdu::Deliver,
        }
        let mut singles: Vec<Piece> = Vec::new();
        let mut multi: Vec<Piece> = Vec::new();
        let mut junk: Vec<u32> = Vec::new();
        for l in listed.iter().filter(|l| l.stat <= 1) {
            match pdu::decode_deliver(&l.pdu) {
                Some(d) => {
                    let p = Piece {
                        index: l.index,
                        read: l.stat == 1,
                        d,
                    };
                    if p.d.concat.is_some_and(|c| c.1 > 1) {
                        multi.push(p);
                    } else {
                        singles.push(p);
                    }
                }
                None => junk.push(l.index),
            }
        }
        // (sender, text, ts, read, SIM indices)
        let mut done: Vec<(String, String, u64, bool, Vec<u32>)> = Vec::new();
        for p in singles {
            done.push((
                p.d.sender,
                p.d.text,
                p.d.ts_ms.unwrap_or_else(wall_ms),
                p.read,
                vec![p.index],
            ));
        }
        // reassemble concatenated messages: complete sets only, the rest waits on the SIM
        let mut used = vec![false; multi.len()];
        for i in 0..multi.len() {
            if used[i] {
                continue;
            }
            let Some((refn, total, _)) = multi[i].d.concat else {
                continue;
            };
            let sender = multi[i].d.sender.clone();
            let group: Vec<usize> = (0..multi.len())
                .filter(|&j| {
                    !used[j]
                        && multi[j].d.sender == sender
                        && multi[j]
                            .d
                            .concat
                            .is_some_and(|c| c.0 == refn && c.1 == total)
                })
                .collect();
            let mut by_seq: Vec<Option<usize>> = vec![None; total as usize];
            for &j in &group {
                if let Some((_, _, seq)) = multi[j].d.concat {
                    if (1..=total).contains(&seq) && by_seq[(seq - 1) as usize].is_none() {
                        by_seq[(seq - 1) as usize] = Some(j);
                    }
                }
            }
            if by_seq.iter().all(Option::is_some) {
                let mut text = String::new();
                let mut idx = Vec::new();
                let mut read = true;
                let mut ts = u64::MAX;
                for j in by_seq.iter().flatten() {
                    text.push_str(&multi[*j].d.text);
                    idx.push(multi[*j].index);
                    read &= multi[*j].read;
                    ts = ts.min(multi[*j].d.ts_ms.unwrap_or_else(wall_ms));
                }
                for j in group {
                    used[j] = true;
                }
                done.push((sender, text, ts, read, idx));
            }
        }
        let mut delete: Vec<u32> = junk;
        let n = done.len();
        for (sender, text, ts, read, idx) in done {
            let id = self.sms.add(Sms {
                id: 0,
                number: sender.clone(),
                text,
                ts,
                incoming: true,
                read,
                state: String::new(),
            });
            delete.extend(idx);
            if !read {
                let name = self.name_of(Some(&sender));
                self.notice(
                    json!({"kind": "sms_received", "id": id, "number": sender, "name": name}),
                );
            }
            self.dirty = true;
        }
        if n > 0 {
            eprintln!("walpad-core: {n} text message(s) received");
        }
        if !delete.is_empty() {
            delete.sort_unstable();
            self.queue.push_back(Op::SmsDelete(delete));
        }
    }

    // ---------------------------------------------------------------- data access
    pub fn sms_list(&self) -> Value {
        let msgs: Vec<Value> = self
            .sms
            .all()
            .iter()
            .map(|m| {
                json!({"id": m.id, "number": m.number, "name": self.name_of(Some(&m.number)),
                    "text": m.text, "ts": m.ts, "incoming": m.incoming, "read": m.read,
                    "state": m.state})
            })
            .collect();
        json!({"messages": msgs, "unread": self.sms.unread()})
    }

    pub fn sms_delete(&mut self, id: u64) -> Result<Value, MobileError> {
        let gone = self.sms.delete(id);
        self.dirty |= gone;
        Ok(json!({"deleted": gone}))
    }

    pub fn sms_read(&mut self, number: &str) -> Result<Value, MobileError> {
        let changed = self.sms.mark_read(number);
        self.dirty |= changed;
        Ok(json!({"unread": self.sms.unread()}))
    }

    pub fn contact_list(&self) -> Value {
        json!({"contacts": self.contacts.all().iter()
            .map(|c| json!({"id": c.id, "name": c.name, "numbers": c.numbers}))
            .collect::<Vec<_>>()})
    }

    pub fn contact_save(
        &mut self,
        id: Option<&str>,
        name: &str,
        numbers: Vec<String>,
    ) -> Result<Value, MobileError> {
        match self.contacts.save(id, name, numbers) {
            Some(id) => Ok(json!({"id": id})),
            None => Err(MobileError::Refused(
                "the contact list is full or the contact does not exist".into(),
            )),
        }
    }

    pub fn contact_delete(&mut self, id: &str) -> Result<Value, MobileError> {
        Ok(json!({"deleted": self.contacts.delete(id)}))
    }

    /// Import a vCard file (`root` re-maps the path in tests).
    pub fn contact_import(&mut self, path: &std::path::Path) -> Result<Value, MobileError> {
        let meta = std::fs::metadata(path)
            .map_err(|_| MobileError::Refused("cannot read the file".into()))?;
        if meta.len() > 1024 * 1024 {
            return Err(MobileError::Refused("the file is too large".into()));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|_| MobileError::Refused("cannot read the file".into()))?;
        let cards = store::parse_vcards(&text);
        let found = cards.len();
        let added = self.contacts.import(cards);
        Ok(json!({"found": found, "added": added}))
    }

    pub fn call_log(&self) -> Value {
        json!({"calls": self.calls.all().iter()
            .map(|c| json!({"number": c.number, "name": self.name_of(Some(&c.number)),
                "ts": c.ts, "dur_s": c.dur_s, "kind": c.kind}))
            .collect::<Vec<_>>()})
    }

    pub fn call_log_clear(&mut self) -> Result<Value, MobileError> {
        self.calls.clear();
        Ok(json!({}))
    }

    // ---------------------------------------------------------------- status
    pub fn status(&self, now: Instant) -> PhoneStatus {
        let g = self.gate;
        let reason = if !g.ready {
            Some("The modem is not ready")
        } else if !g.radio_on {
            Some("Airplane mode: the mobile radio is off")
        } else if !g.sim_ready {
            Some("No usable SIM card")
        } else if !g.registered {
            Some("Not registered on a mobile network")
        } else {
            None
        };
        let mut s = PhoneStatus {
            available: reason.is_none(),
            reason: reason.map(String::from),
            unread: self.sms.unread(),
            speaker: self.speaker,
            audio: "experimental".into(),
            ..Default::default()
        };
        if let Some(c) = &self.call {
            s.call = c.state;
            s.incoming = c.incoming;
            s.number = c.number.clone();
            s.name = self.name_of(c.number.as_deref());
            s.elapsed_s = c.answered_at.map_or(0, |t| now.duration_since(t).as_secs());
            s.muted = self.muted;
        } else if let Some(e) = &self.ended {
            s.call = CallState::Ended;
            s.number = e.number.clone();
            s.name = self.name_of(e.number.as_deref());
            s.end_reason = Some(e.reason.clone());
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modem::at::Final;

    fn root() -> Root {
        let d = std::env::temp_dir().join(format!(
            "walpad-phone-unit-{}-{}",
            std::process::id(),
            wall_ms()
        ));
        let _ = std::fs::create_dir_all(&d);
        Root::new(&d)
    }

    fn ok(lines: &[&str]) -> Outcome {
        Ok(Reply {
            lines: lines.iter().map(|s| s.to_string()).collect(),
            fin: Final::Ok,
        })
    }

    fn fin(f: Final) -> Outcome {
        Ok(Reply {
            lines: vec![],
            fin: f,
        })
    }

    fn ready() -> Gate {
        Gate {
            ready: true,
            radio_on: true,
            sim_ready: true,
            registered: true,
        }
    }

    /// A phone past its setup, with a registered line.
    fn phone() -> (Phone, Instant) {
        let mut p = Phone::new(&root());
        let now = Instant::now();
        p.set_gate(ready(), now);
        let j = p.next_job(now).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Setup));
        assert_eq!(j.cmds.len(), 5);
        p.handle(
            PhoneKind::Setup,
            &[
                ok(&[]),
                ok(&[]),
                ok(&[]),
                ok(&[]),
                ok(&["+CSCA: \"+8801716000000\",145"]),
            ],
            now,
        );
        assert_eq!(p.smsc.as_deref(), Some("+8801716000000"));
        (p, now)
    }

    fn clcc(p: &mut Phone, lines: &[&str], now: Instant) {
        p.next_clcc = now;
        let keep = p.next_sms;
        p.next_sms = now + Duration::from_secs(3600);
        let j = p.next_job(now).unwrap();
        p.next_sms = keep;
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Clcc));
        p.handle(PhoneKind::Clcc, &[ok(lines)], now);
    }

    #[test]
    fn nothing_runs_until_the_modem_is_ready() {
        let now = Instant::now();
        let mut p = Phone::new(&root());
        assert!(p.next_job(now).is_none());
        p.set_gate(
            Gate {
                ready: true,
                ..Default::default()
            },
            now,
        );
        // modem ready but no SIM and radio off: no setup, no call list
        assert!(p.next_job(now).is_none());
        assert!(!p.status(now).available);
        assert_eq!(
            p.status(now).reason.as_deref(),
            Some("Airplane mode: the mobile radio is off")
        );
    }

    #[test]
    fn outgoing_call_runs_dialing_active_ended() {
        let (mut p, now) = phone();
        assert!(p.dial("01711234567", now).is_ok());
        assert_eq!(p.status(now).call, CallState::Dialing);
        assert!(p.status(now).number.is_some());
        assert!(matches!(
            p.dial("01711234568", now),
            Err(MobileError::Busy(_))
        ));
        // the dial job goes out first and prints without the number
        let j = p.next_job(now).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Dial));
        assert_eq!(format!("{:?}", j.cmds), "[ATD***]");
        p.handle(PhoneKind::Dial, &[ok(&[])], now);
        // not in the list yet: still dialling (grace)
        clcc(&mut p, &[], now + Duration::from_secs(1));
        assert_eq!(p.status(now).call, CallState::Dialing);
        clcc(&mut p, &["+CLCC: 1,0,3,0,0,\"01711234567\",129"], now);
        assert_eq!(p.status(now).call, CallState::Dialing);
        let t1 = now + Duration::from_secs(2);
        clcc(&mut p, &["+CLCC: 1,0,0,0,0,\"01711234567\",129"], t1);
        let s = p.status(t1 + Duration::from_secs(5));
        assert_eq!(s.call, CallState::Active);
        assert_eq!(s.elapsed_s, 5);
        assert!(!s.incoming);
        // the other side hangs up: the list is empty, the log gets an "out" record
        clcc(&mut p, &[], t1 + Duration::from_secs(7));
        let s = p.status(t1 + Duration::from_secs(7));
        assert_eq!(s.call, CallState::Ended);
        assert_eq!(s.end_reason.as_deref(), Some("remote"));
        let n = p.take_notices();
        assert_eq!(n.last().unwrap()["kind"], "call_ended");
        assert_eq!(n.last().unwrap()["duration_s"], 7);
        assert_eq!(p.calls.all().len(), 1);
        assert_eq!(p.calls.all()[0].kind, "out");
        // the ended banner clears by itself
        p.tick(t1 + Duration::from_secs(20));
        assert_eq!(p.status(t1).call, CallState::Idle);
    }

    #[test]
    fn incoming_call_answered_then_hung_up_locally() {
        let (mut p, now) = phone();
        p.contacts
            .save(None, "Rahim", vec!["+8801711234567".into()]);
        clcc(&mut p, &["+CLCC: 1,1,4,0,0,\"01711234567\",129"], now);
        let s = p.status(now);
        assert_eq!((s.call, s.incoming), (CallState::Ringing, true));
        assert_eq!(s.name.as_deref(), Some("Rahim"));
        let n = p.take_notices();
        assert_eq!(n[0]["kind"], "incoming_call");
        assert_eq!(n[0]["name"], "Rahim");
        // only one notice per call
        clcc(&mut p, &["+CLCC: 1,1,4,0,0,\"01711234567\",129"], now);
        assert!(p.take_notices().is_empty());
        assert!(p.answer(now).is_ok());
        let j = p.next_job(now).unwrap();
        assert_eq!(format!("{:?}", j.cmds), "[ATA]");
        p.handle(PhoneKind::Answer, &[ok(&[])], now);
        clcc(&mut p, &["+CLCC: 1,1,0,0,0,\"01711234567\",129"], now);
        assert_eq!(p.status(now).call, CallState::Active);
        // mute and dtmf only now
        assert!(p.mute(true, now).is_ok());
        assert!(p.dtmf('5', now).is_ok());
        assert!(p.dtmf('x', now).is_err());
        let j = p.next_job(now).unwrap();
        assert_eq!(format!("{:?}", j.cmds), "[AT+CMUT=1]");
        p.handle(PhoneKind::Mute, &[ok(&[])], now);
        assert!(p.status(now).muted);
        let j = p.next_job(now).unwrap();
        assert_eq!(format!("{:?}", j.cmds), "[AT+VTS=5]");
        p.handle(PhoneKind::Dtmf, &[ok(&[])], now);
        // hang up: AT+CHUP, and ATH when the modem does not know AT+CHUP
        assert!(p.hangup(now).is_ok());
        let j = p.next_job(now).unwrap();
        assert_eq!(format!("{:?}", j.cmds), "[AT+CHUP]");
        p.handle(PhoneKind::Hangup, &[fin(Final::Error)], now);
        let j = p.next_job(now).unwrap();
        assert_eq!(format!("{:?}", j.cmds), "[ATH]");
        p.handle(PhoneKind::Hangup, &[ok(&[])], now);
        clcc(&mut p, &[], now);
        let s = p.status(now);
        assert_eq!(s.call, CallState::Ended);
        assert_eq!(s.end_reason.as_deref(), Some("hangup"));
        assert!(!s.muted, "mute does not outlive the call");
        assert_eq!(p.calls.all()[0].kind, "in");
    }

    #[test]
    fn missed_and_rejected_calls_are_told_apart() {
        let (mut p, now) = phone();
        clcc(&mut p, &["+CLCC: 1,1,4,0,0,\"01711234567\",129"], now);
        clcc(&mut p, &[], now);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("missed"));
        assert_eq!(p.calls.all()[0].kind, "missed");
        p.tick(now + Duration::from_secs(60));
        clcc(&mut p, &["+CLCC: 1,1,4,0,0,\"01711234567\",129"], now);
        assert!(p.hangup(now).is_ok());
        p.next_job(now).unwrap();
        p.handle(PhoneKind::Hangup, &[ok(&[])], now);
        clcc(&mut p, &[], now);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("rejected"));
    }

    #[test]
    fn dial_failures_and_disconnect_causes() {
        let (mut p, now) = phone();
        p.dial("01711234567", now).unwrap();
        p.next_job(now).unwrap();
        p.handle(PhoneKind::Dial, &[fin(Final::Busy)], now);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("busy"));
        p.tick(now + Duration::from_secs(60));
        p.dial("01711234567", now).unwrap();
        p.next_job(now).unwrap();
        p.handle(PhoneKind::Dial, &[fin(Final::Cme("30".into()))], now);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("failed"));
        p.tick(now + Duration::from_secs(60));
        // alerting, then the far end rejects: the ECPI cause in the poll reply says so
        p.dial("01711234567", now).unwrap();
        p.next_job(now).unwrap();
        p.handle(PhoneKind::Dial, &[ok(&[])], now);
        clcc(&mut p, &["+CLCC: 1,0,3,0,0,\"01711234567\",129"], now);
        clcc(&mut p, &["+ECPI: 1,6,1,0,0,0,\"\",0,\"\",21"], now);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("rejected"));
        p.tick(now + Duration::from_secs(60));
        // never listed: failed once the grace is over
        p.dial("01711234567", now).unwrap();
        p.next_job(now).unwrap();
        p.handle(PhoneKind::Dial, &[ok(&[])], now);
        let late = now + Duration::from_secs(30);
        let j = p.next_job(late).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Clcc));
        p.handle(PhoneKind::Clcc, &[ok(&[])], late);
        assert_eq!(p.status(late).end_reason.as_deref(), Some("remote"));
    }

    #[test]
    fn a_cut_short_reply_does_not_end_the_call() {
        let (mut p, now) = phone();
        clcc(&mut p, &["+CLCC: 1,1,0,0,0,\"01711234567\",129"], now);
        p.next_sms = now + Duration::from_secs(3600);
        // `NO CARRIER` URC inside the reply of the poll: final is NoCarrier, lines empty
        let j = p.next_job(now + Duration::from_secs(5)).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Clcc));
        p.handle(PhoneKind::Clcc, &[fin(Final::NoCarrier)], now);
        assert_eq!(p.status(now).call, CallState::Active);
        // and a timeout keeps it as well
        let j = p.next_job(now + Duration::from_secs(10)).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::Clcc));
        p.handle(PhoneKind::Clcc, &[Err(AtError::Timeout)], now);
        assert_eq!(p.status(now).call, CallState::Active);
    }

    #[test]
    fn dial_is_checked_and_gated() {
        let now = Instant::now();
        let mut p = Phone::new(&root());
        assert!(matches!(p.dial("999", now), Err(MobileError::Refused(_))));
        // radio on, no SIM, not registered: only the emergency numbers go
        p.set_gate(
            Gate {
                ready: true,
                radio_on: true,
                ..Default::default()
            },
            now,
        );
        assert!(matches!(
            p.dial("01711234567", now),
            Err(MobileError::Refused(_))
        ));
        assert!(p.dial("999", now).is_ok());
        let mut q = Phone::new(&root());
        q.set_gate(ready(), now);
        for bad in ["", "abc", "017;rm", "0171 123", "+".repeat(33).as_str()] {
            assert!(
                matches!(q.dial(bad, now), Err(MobileError::Invalid(_))),
                "{bad}"
            );
        }
        assert!(matches!(q.dial("*121#", now), Err(MobileError::Refused(_))));
        for ok_number in ["112", "199", "+8801711234567", "*21#0171", "123,4"] {
            let mut r = Phone::new(&root());
            r.set_gate(ready(), now);
            let res = r.dial(ok_number, now);
            // `*21#0171` is not USSD (does not start with * and end with #)
            assert!(res.is_ok(), "{ok_number}: {res:?}");
        }
        assert!(is_emergency("112") && is_emergency("999") && is_emergency("199"));
        assert!(!is_emergency("1999"));
    }

    #[test]
    fn losing_the_modem_ends_the_call() {
        let (mut p, now) = phone();
        p.dial("01711234567", now).unwrap();
        p.set_gate(Gate::default(), now);
        assert_eq!(p.status(now).call, CallState::Ended);
        assert_eq!(p.status(now).end_reason.as_deref(), Some("failed"));
        assert!(p.next_job(now).is_none());
    }

    #[test]
    fn received_messages_are_stored_and_deleted_from_the_sim() {
        let (mut p, now) = phone();
        // two GSM 7-bit messages (one unread), one half of a concatenated pair, junk
        let one = pdu_deliver("+8801711234567", "hello", None);
        let two = pdu_deliver("01555000111", "second", None);
        let parts = pdu::encode_submit(None, "01711234567", &"x".repeat(170), 4).unwrap();
        assert_eq!(parts.len(), 2);
        let as_deliver = |h: &str| pdu_from_submit(h);
        let (a, b) = (as_deliver(&parts[0].hex), as_deliver(&parts[1].hex));
        p.contacts.save(None, "Rahim", vec!["01711234567".into()]);
        let lines: Vec<String> = [
            ("1", "0", &one),
            ("2", "1", &two),
            ("3", "0", &a),
            ("4", "0", &"FF".to_string()),
            ("5", "3", &"00".to_string()),
        ]
        .iter()
        .flat_map(|(i, s, h)| [format!("+CMGL: {i},{s},,10"), (*h).clone()])
        .collect();
        clcc(&mut p, &[], now);
        let j = p.next_job(now).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::SmsPoll));
        assert_eq!(format!("{:?}", j.cmds), "[AT+CMGL=4]");
        p.handle(
            PhoneKind::SmsPoll,
            &[Ok(Reply {
                lines,
                fin: Final::Ok,
            })],
            now,
        );
        // the half of a pair waits on the SIM; the rest is in the store
        assert_eq!(p.sms.all().len(), 2);
        assert_eq!(p.sms.unread(), 1);
        let n = p.take_notices();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0]["kind"], "sms_received");
        let j = p.next_job(now + Duration::from_secs(31)).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::SmsDelete));
        assert_eq!(
            format!("{:?}", j.cmds),
            "[AT+CMGD=1, AT+CMGD=2, AT+CMGD=4]",
            "delivered messages and junk go, the unfinished pair stays"
        );
        p.handle(PhoneKind::SmsDelete, &[ok(&[]), ok(&[]), ok(&[])], now);
        // the second half arrives later: now the pair is complete
        let lines = vec![
            "+CMGL: 3,0,,10".to_string(),
            a.clone(),
            "+CMGL: 6,0,,10".to_string(),
            b.clone(),
        ];
        p.next_sms = now;
        clcc(&mut p, &[], now);
        let j = p.next_job(now).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::SmsPoll));
        p.handle(
            PhoneKind::SmsPoll,
            &[Ok(Reply {
                lines,
                fin: Final::Ok,
            })],
            now,
        );
        assert_eq!(p.sms.all().len(), 3);
        assert_eq!(p.sms.all()[2].text, "x".repeat(170));
        let l = p.sms_list();
        assert_eq!(l["messages"][2]["name"], "Rahim");
        assert_eq!(l["unread"], 2);
        // both unread messages are from the same party (national and international form)
        assert_eq!(p.sms_read("+8801711234567").unwrap()["unread"], 0);
    }

    fn pdu_deliver(sender: &str, text: &str, _x: Option<()>) -> String {
        // build a DELIVER from our own SUBMIT encoder
        let parts = pdu::encode_submit(None, sender, text, 1).unwrap();
        pdu_from_submit(&parts[0].hex)
    }

    fn pdu_from_submit(h: &str) -> String {
        let b = pdu::hex_decode(h).unwrap();
        let mut i = 1 + b[0] as usize;
        let first = b[i];
        i += 2;
        let da_len = b[i] as usize;
        let oa_end = i + 2 + da_len.div_ceil(2);
        let oa = &b[i..oa_end];
        let (pid, dcs) = (b[oa_end], b[oa_end + 1]);
        let ud = &b[oa_end + 3..];
        let mut d = vec![0x00, first & 0x40];
        d.extend(oa);
        d.extend([pid, dcs]);
        d.extend([0x11, 0x21, 0x31, 0x21, 0x43, 0x65, 0x23]);
        d.extend(ud);
        pdu::hex_encode(&d)
    }

    #[test]
    fn sending_builds_pdu_jobs_and_records_the_outcome() {
        let (mut p, now) = phone();
        let r = p.sms_send("01711234567", "আমি ভালো আছি").unwrap();
        assert_eq!(r["parts"], 1);
        let j = p.next_job(now).unwrap();
        assert_eq!(j.kind, JobKind::Phone(PhoneKind::SmsSend));
        // the PDU and the number never print
        let dbg = format!("{:?}", j.cmds);
        assert_eq!(dbg, "[AT+CMGS=***]");
        p.handle(PhoneKind::SmsSend, &[ok(&["+CMGS: 7"])], now);
        assert_eq!(p.sms.all()[0].state, "sent");
        let n = p.take_notices();
        assert_eq!(n[0]["kind"], "sms_sent");
        assert_eq!(n[0]["ok"], true);
        // a network refusal is reported with its code, a timeout in words
        p.sms_send("01711234567", "x").unwrap();
        p.next_job(now).unwrap();
        p.handle(
            PhoneKind::SmsSend,
            &[Ok(Reply {
                lines: vec![],
                fin: Final::Cms("330".into()),
            })],
            now,
        );
        assert_eq!(p.sms.all()[1].state, "failed");
        let n = p.take_notices();
        assert!(n[0]["error"].as_str().unwrap().contains("330"));
        p.sms_send("01711234567", "x").unwrap();
        p.next_job(now).unwrap();
        p.handle(PhoneKind::SmsSend, &[Err(AtError::Timeout)], now);
        assert_eq!(p.sms.all()[2].state, "failed");
        // limits
        assert!(matches!(
            p.sms_send("01711234567", &"a".repeat(1000)),
            Err(MobileError::Invalid(_))
        ));
        assert!(matches!(
            p.sms_send("12", "x"),
            Err(MobileError::Invalid(_))
        ));
        p.set_gate(Gate::default(), now);
        assert!(matches!(
            p.sms_send("01711234567", "x"),
            Err(MobileError::Refused(_))
        ));
    }

    #[test]
    fn contacts_and_call_log_through_the_api() {
        let (mut p, _) = phone();
        let id = p
            .contact_save(None, "Mina", vec!["01711234567".into()])
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(p.contact_list()["contacts"][0]["name"], "Mina");
        assert_eq!(p.contact_delete(&id).unwrap()["deleted"], true);
        assert!(p.contact_save(Some("zz"), "x", vec![]).is_err());
        assert_eq!(p.call_log()["calls"].as_array().unwrap().len(), 0);
        p.call_log_clear().unwrap();
    }
}
