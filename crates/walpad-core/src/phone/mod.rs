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

mod calls;
mod jobs;
mod ops;
mod sms;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_sms;
mod ussd_sim;

use ops::{AudioWant, Call, Ended, Op};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};
use store::{CallLog, ContactStore, SmsStore};
use walpad_hw::Root;
use walpad_proto::{CallState, PhoneStatus, UssdStatus};

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
    Ussd,
    UssdCancel,
    SimContacts,
    Answer,
    Hangup,
    Dtmf,
    Mute,
    SmsSend,
    SmsPoll,
    SmsDelete,
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
    ussd: Option<UssdStatus>,
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

pub(super) fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
            ussd: None,
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

    pub(super) fn name_of(&self, number: Option<&str>) -> Option<String> {
        number
            .and_then(|n| self.contacts.name_of(n))
            .map(String::from)
    }

    pub(super) fn notice(&mut self, v: Value) {
        self.notices.push(v);
    }

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
            audio: if self.script.exists() {
                "ready"
            } else {
                "unavailable"
            }
            .into(),
            ussd: self.ussd.clone(),
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
