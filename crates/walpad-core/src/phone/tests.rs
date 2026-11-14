use super::*;
use crate::modem::at::{AtError, Final, Outcome, Reply};
use crate::modem::{JobKind, MobileError};

pub(super) fn root() -> Root {
    let d = std::env::temp_dir().join(format!(
        "walpad-phone-unit-{}-{}",
        std::process::id(),
        wall_ms()
    ));
    let _ = std::fs::create_dir_all(&d);
    Root::new(&d)
}

pub(super) fn ok(lines: &[&str]) -> Outcome {
    Ok(Reply {
        lines: lines.iter().map(|s| s.to_string()).collect(),
        fin: Final::Ok,
    })
}

pub(super) fn fin(f: Final) -> Outcome {
    Ok(Reply {
        lines: vec![],
        fin: f,
    })
}

pub(super) fn ready() -> Gate {
    Gate {
        ready: true,
        radio_on: true,
        sim_ready: true,
        registered: true,
    }
}

/// A phone past its setup, with a registered line.
pub(super) fn phone() -> (Phone, Instant) {
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

pub(super) fn clcc(p: &mut Phone, lines: &[&str], now: Instant) {
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
