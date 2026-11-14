//! AT operation queue types, call state structs, and reply helpers for [`super::Phone`].
use super::{pdu, PhoneKind};
use crate::modem::at::{AtCmd, Final, Outcome, Reply};
use std::time::{Duration, Instant};
use walpad_proto::CallState;

pub(super) enum Op {
    Setup,
    Clcc,
    Dial(String),
    Ussd(String),
    UssdCancel,
    SimContactsRange,
    SimContactsRead(u16, u16),
    Answer,
    Hangup { ath: bool },
    Dtmf(char),
    Mute(bool),
    SmsSend { id: u64, parts: Vec<pdu::Part> },
    SmsPoll,
    SmsDelete(Vec<u32>),
}

impl Op {
    pub(super) fn kind(&self) -> PhoneKind {
        match self {
            Op::Setup => PhoneKind::Setup,
            Op::Clcc => PhoneKind::Clcc,
            Op::Dial(_) => PhoneKind::Dial,
            Op::Ussd(_) => PhoneKind::Ussd,
            Op::UssdCancel => PhoneKind::UssdCancel,
            Op::SimContactsRange | Op::SimContactsRead(..) => PhoneKind::SimContacts,
            Op::Answer => PhoneKind::Answer,
            Op::Hangup { .. } => PhoneKind::Hangup,
            Op::Dtmf(_) => PhoneKind::Dtmf,
            Op::Mute(_) => PhoneKind::Mute,
            Op::SmsSend { .. } => PhoneKind::SmsSend,
            Op::SmsPoll => PhoneKind::SmsPoll,
            Op::SmsDelete(_) => PhoneKind::SmsDelete,
        }
    }

    pub(super) fn cmds(&self) -> Vec<AtCmd> {
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
            Op::Ussd(code) => {
                vec![AtCmd::secret(format!("AT+CUSD=1,\"{code}\",15"))
                    .timeout(Duration::from_secs(20))]
            }
            Op::UssdCancel => vec![AtCmd::new("AT+CUSD=2")],
            Op::SimContactsRange => {
                vec![AtCmd::new("AT+CPBS=\"SM\""), AtCmd::new("AT+CPBR=?")]
            }
            Op::SimContactsRead(a, b) => {
                vec![AtCmd::new(format!("AT+CPBR={a},{b}")).timeout(Duration::from_secs(10))]
            }
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

pub(super) struct Call {
    pub(super) number: Option<String>,
    pub(super) incoming: bool,
    pub(super) state: CallState,
    pub(super) dialed_at: Instant,
    /// The call list showed it at least once.
    pub(super) seen: bool,
    pub(super) answered_at: Option<Instant>,
    /// Why the owner ended it (`hangup` / `rejected`), when they did.
    pub(super) hangup: Option<&'static str>,
    /// Q.931 cause of the disconnect, when the modem told us.
    pub(super) cause: Option<u16>,
    pub(super) started_ms: u64,
}

pub(super) struct Ended {
    pub(super) until: Instant,
    pub(super) reason: String,
    pub(super) number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AudioWant {
    Start(bool),
    Stop,
}

pub(super) fn ok_final(o: Option<&Outcome>) -> bool {
    matches!(o, Some(Ok(r)) if r.fin == Final::Ok)
}

pub(super) fn reply(o: Option<&Outcome>) -> Option<&Reply> {
    o.and_then(|o| o.as_ref().ok())
}
