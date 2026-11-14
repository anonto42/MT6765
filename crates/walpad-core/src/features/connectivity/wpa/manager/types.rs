use super::super::ctrl::CtrlError;
use super::super::parse::ScanEntry;
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum WifiEvent {
    Scan {
        networks: Vec<ScanEntry>,
    },
    /// `error`: wrong_password | timeout | failed
    Connect {
        ok: bool,
        error: Option<&'static str>,
        ssid: Vec<u8>,
    },
    Disconnected,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WifiError {
    /// The supplicant control socket is not reachable.
    Unavailable,
    Busy,
    Invalid(String),
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiLive {
    pub state: Option<String>,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub signal_dbm: Option<i32>,
}

pub(super) enum Op {
    Idle,
    Scanning {
        deadline: Instant,
    },
    Connecting {
        id: u32,
        ssid: Vec<u8>,
        deadline: Instant,
        was_new: bool,
        others: Vec<u32>,
        next_poll: Instant,
    },
}

pub(super) fn map(e: CtrlError) -> WifiError {
    match e {
        CtrlError::Down(_) | CtrlError::Timeout => WifiError::Unavailable,
        CtrlError::Fail(m) => WifiError::Failed(m),
    }
}
