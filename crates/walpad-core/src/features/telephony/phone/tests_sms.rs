use super::tests::{clcc, ok, phone, ready, root};
use super::*;
use crate::modem::at::{AtError, Final, Reply};
use crate::modem::{JobKind, MobileError};

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
    assert!(q.dial("*121#", now).is_ok());
    let j = q.next_job(now).unwrap();
    assert_eq!(j.kind, JobKind::Phone(PhoneKind::Ussd));
    q.handle(
        PhoneKind::Ussd,
        &[ok(&["+CUSD: 0,\"Balance: 50 Tk\",15"])],
        now,
    );
    let u = q.status(now).ussd.unwrap();
    assert_eq!(u.state, "done");
    assert_eq!(u.text.as_deref(), Some("Balance: 50 Tk"));
    for ok_number in ["112", "199", "+8801711234567", "*21#0171", "123,4"] {
        let mut r = Phone::new(&root());
        r.set_gate(ready(), now);
        let res = r.dial(ok_number, now);
        assert!(res.is_ok(), "{ok_number}: {res:?}");
    }
    assert!(is_emergency("112") && is_emergency("999") && is_emergency("199"));
    assert!(!is_emergency("1999"));
}

#[test]
fn sim_contacts_load_in_batches() {
    let (mut p, now) = phone();
    assert!(p.sim_contacts().is_ok());
    let j = p.next_job(now).unwrap();
    assert_eq!(j.kind, JobKind::Phone(PhoneKind::SimContacts));
    p.handle(
        PhoneKind::SimContacts,
        &[ok(&[]), ok(&["+CPBR: (1-2),40,14"])],
        now,
    );
    let j2 = p.next_job(now).unwrap();
    assert_eq!(j2.kind, JobKind::Phone(PhoneKind::SimContacts));
    p.handle(
        PhoneKind::SimContacts,
        &[ok(&["+CPBR: 1,\"+8801711234567\",145,\"GP Help\""])],
        now,
    );
    assert_eq!(p.contacts.name_of("01711234567"), Some("GP Help"));
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
