use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("walpad-phone-store-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn numbers_match_across_formats() {
    assert!(same_number("01711234567", "+8801711234567"));
    assert!(same_number("+880 1711-234567", "01711234567"));
    assert!(same_number("999", "999"));
    assert!(
        !same_number("999", "1999"),
        "short numbers match exactly only"
    );
    assert!(!same_number("01711234567", "01711234568"));
    assert!(!same_number("", "123"));
}

#[test]
fn sms_store_persists_trims_and_counts_unread() {
    let d = tmp("sms");
    let p = d.join("sms.json");
    let mut s = SmsStore::load(p.clone());
    let mk = |n: &str, inc: bool| Sms {
        id: 0,
        number: n.into(),
        text: "t".into(),
        ts: 1,
        incoming: inc,
        read: !inc,
        state: if inc { "".into() } else { "sending".into() },
    };
    let a = s.add(mk("01711234567", true));
    let b = s.add(mk("01711234567", false));
    assert_eq!((a, b), (1, 2));
    assert_eq!(s.unread(), 1);
    assert_eq!(
        std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // reload: the outgoing one that never finished is failed, ids continue
    let mut s2 = SmsStore::load(p.clone());
    assert_eq!(s2.all()[1].state, "failed");
    assert_eq!(s2.add(mk("999", true)), 3);
    assert!(s2.mark_read("+8801711234567"));
    assert_eq!(s2.unread(), 1);
    assert!(s2.delete(1));
    assert!(!s2.delete(1));
    // cap
    for _ in 0..MAX_SMS + 10 {
        s2.add(mk("999", false));
    }
    assert_eq!(s2.all().len(), MAX_SMS);
    // a damaged file starts empty instead of failing
    std::fs::write(&p, "{not json").unwrap();
    assert!(SmsStore::load(p).all().is_empty());
}

#[test]
fn contacts_save_lookup_delete_import() {
    let d = tmp("contacts");
    let mut c = ContactStore::load(d.join("contacts.json"));
    let id = c.save(None, "Zara", vec!["01711234567".into()]).unwrap();
    assert!(c.save(Some("nope"), "x", vec![]).is_none());
    c.save(None, "amir", vec!["+8801811000000".into()]).unwrap();
    assert_eq!(c.all()[0].name, "amir", "sorted case-insensitively");
    assert_eq!(c.name_of("+8801711234567"), Some("Zara"));
    assert_eq!(c.name_of("01999999999"), None);
    c.save(Some(&id), "Zara K", vec!["01711234567".into()])
        .unwrap();
    assert_eq!(c.name_of("01711234567"), Some("Zara K"));
    let n = c.import(vec![
        ("Zara K".into(), vec!["+8801711234567".into()]),
        ("New".into(), vec!["01555000111".into()]),
    ]);
    assert_eq!(n, 1, "the duplicate is skipped");
    assert!(c.delete(&id));
    assert!(!c.delete(&id));
    // persisted
    let again = ContactStore::load(d.join("contacts.json"));
    assert_eq!(again.all().len(), 2);
}

#[test]
fn vcards_are_parsed_loosely() {
    let v = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Rahim Uddin\r\nTEL;TYPE=CELL:+880 1711-234567\r\nTEL;CELL:01811000000\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nVERSION:2.1\r\nN:Begum;Salma;;;\r\nTEL:01555000111\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nFN:No Number\r\nEND:VCARD\r\n\
             BEGIN:VCARD\r\nFN:Bad\r\nTEL:12\r\nEND:VCARD\r\n";
    let c = parse_vcards(v);
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].0, "Rahim Uddin");
    assert_eq!(c[0].1, ["+8801711234567", "01811000000"]);
    assert_eq!(c[1].0, "Salma Begum");
}

#[test]
fn call_log_is_capped() {
    let d = tmp("calls");
    let mut l = CallLog::load(d.join("calls.json"));
    for i in 0..MAX_CALLS + 5 {
        l.add(CallRec {
            number: "999".into(),
            ts: i as u64,
            dur_s: 1,
            kind: "out".into(),
        });
    }
    assert_eq!(l.all().len(), MAX_CALLS);
    assert_eq!(l.all()[0].ts, 5);
    l.clear();
    assert!(CallLog::load(d.join("calls.json")).all().is_empty());
}
