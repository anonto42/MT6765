use super::at::{AtError, Final, Outcome, Reply};
use super::parse;
use crate::phone::PhoneKind;
use std::process::Child;
use std::time::{Duration, Instant};
use walpad_proto::valid_apn;

pub const SCRIPT: &str = "/usr/local/sbin/walpad-modem";
pub const TTY: &str = "/dev/radio/pttycmd1";
pub const NOTI_TTY: &str = "/dev/radio/pttynoti";
pub const CONF: &str = "/etc/walpad/mobile.conf";
/// The owner's radio switch: `0` = off (airplane), anything else or missing = on.
pub const RADIO_FILE: &str = "/var/lib/walpad/mobile-radio";
/// /run is a tmpfs: present = `walpad-modem start` was already tried this boot.
pub const START_MARK: &str = "/run/walpad/modem-start-attempted";
pub(super) const OUT: &str = "/run/walpad/modem-out";
pub(super) const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

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
    /// Calls and messages (owner request 2026-11-11): the phone module builds and digests these.
    Phone(PhoneKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    Start,
    DataOn,
    DataOff,
    DataStatus,
}

impl Verb {
    pub(super) fn args(self) -> &'static [&'static str] {
        match self {
            Verb::Start => &["start"],
            Verb::DataOn => &["data", "on"],
            Verb::DataOff => &["data", "off"],
            Verb::DataStatus => &["data", "status"],
        }
    }
}

pub(super) struct Run {
    pub(super) verb: Verb,
    pub(super) child: Child,
    pub(super) deadline: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MobileError {
    Busy(String),
    Refused(String),
    Invalid(String),
    Failed(String),
}

/// One `key=value` line of the script's output.
pub(super) fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
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

pub(super) fn ok_final(o: Option<&Outcome>) -> bool {
    matches!(o, Some(Ok(r)) if r.fin == Final::Ok)
}

pub(super) fn job_notice(k: JobKind) -> &'static str {
    match k {
        JobKind::Radio(_) => "mobile_radio",
        _ => "mobile_pin",
    }
}

pub(super) fn at_error_text(o: Option<&Outcome>) -> String {
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

pub(super) fn pin_error(e: &str) -> String {
    let t = e.trim().to_ascii_lowercase();
    if t == "16" || t.contains("incorrect password") {
        "Wrong PIN".into()
    } else if t == "12" || t.contains("puk") {
        "SIM locked: PUK required".into()
    } else {
        format!("PIN not accepted (error {})", parse::clean(e, 30))
    }
}
