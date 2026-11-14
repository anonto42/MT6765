use crate::wpa::parse::unhex;
use std::process::Child;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    Start,
    Stop,
    Status,
}

impl Verb {
    pub(super) fn arg(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Status => "status",
        }
    }
}

pub(super) struct Run {
    pub(super) verb: Verb,
    pub(super) child: Child,
    pub(super) deadline: Instant,
    /// A cleanup `stop` after a failed/timed-out start: it must not overwrite the failure.
    pub(super) cleanup: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HotspotError {
    Busy,
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

/// Short, printable reason text from the script (never trusted to be tidy).
pub(super) fn clean_reason(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_string()
}

/// (ssid, has_secret) from the config file content.
pub fn parse_conf(text: &str) -> (Option<String>, bool) {
    let mut ssid = None;
    let mut secret = false;
    for l in text.lines() {
        if let Some(h) = l.strip_prefix("ssid2=") {
            ssid = unhex(h.trim()).map(|b| String::from_utf8_lossy(&b).into_owned());
        } else if l.starts_with("wpa_passphrase=") && l.len() > "wpa_passphrase=".len() {
            secret = true;
        }
    }
    (ssid, secret)
}
