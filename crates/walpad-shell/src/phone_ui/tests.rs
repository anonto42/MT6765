use super::geom::body_top;
use super::*;
use serde_json::Value;
use walpad_proto::CallState;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

fn sms(id: u64, n: &str, t: &str, ts: u64, inc: bool, read: bool) -> SmsEntry {
    SmsEntry {
        id,
        number: n.into(),
        name: None,
        text: t.into(),
        ts,
        incoming: inc,
        read,
        state: if inc { "".into() } else { "sent".into() },
    }
}

fn model() -> PhoneModel {
    PhoneModel {
        tz: jiff::tz::TimeZone::UTC,
        now_ms: Some(1_605_000_000_000),
        ..Default::default()
    }
}

#[test]
fn wrap_breaks_words_and_long_runs() {
    assert_eq!(
        wrap("hello brave new world", 11),
        ["hello brave", "new world"]
    );
    assert_eq!(wrap("a\nb", 10), ["a", "b"]);
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("", 10), [""]);
    // Bangla counts characters, not bytes
    assert_eq!(wrap("আমি ভালো আছি", 8), ["আমি ভালো", "আছি"]);
}

#[test]
fn times_and_durations_read_naturally() {
    let tz = jiff::tz::TimeZone::UTC;
    let now = 1_605_000_000_000; // 2020-11-10 09:20 UTC
    assert_eq!(fmt_when(now - 60_000, now, &tz), "09:19");
    assert_eq!(fmt_when(now - 86_400_000, now, &tz), "Mon 09 Nov");
    assert_eq!(fmt_when(now - 400 * 86_400_000, now, &tz), "07 Oct 2019");
    assert_eq!(fmt_duration(5), "0:05");
    assert_eq!(fmt_duration(754), "12:34");
    assert_eq!(fmt_duration(3725), "1:02:05");
}

#[test]
fn conversations_group_national_and_international_numbers() {
    let mut m = model();
    m.sms = vec![
        sms(1, "01711234567", "hi", 10, true, true),
        sms(2, "+8801711234567", "hello back", 20, false, true),
        sms(3, "01555000111", "other", 15, true, false),
        sms(4, "+8801711234567", "again", 30, true, false),
    ];
    let c = m.conversations();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].count, 3);
    assert_eq!(c[0].unread, 1);
    assert_eq!(c[0].last_text, "again");
    assert_eq!(c[1].number, "01555000111");
    m.thread = Some("01711234567".into());
    let t = m.thread_messages();
    assert_eq!(
        t.iter().map(|x| x.id).collect::<Vec<_>>(),
        [4, 2, 1],
        "newest first"
    );
}

#[test]
fn contacts_filter_by_name_or_number() {
    let mut m = model();
    m.contacts = vec![
        ContactEntry {
            id: "c1".into(),
            name: "Zara".into(),
            numbers: vec!["01711234567".into()],
        },
        ContactEntry {
            id: "c2".into(),
            name: "amir".into(),
            numbers: vec!["+8801555000111".into()],
        },
    ];
    assert_eq!(m.filtered_contacts()[0].name, "amir");
    m.search = "ZAR".into();
    assert_eq!(m.filtered_contacts().len(), 1);
    m.search = "555".into();
    assert_eq!(m.filtered_contacts()[0].id, "c2");
    m.search = "zzz".into();
    assert!(m.filtered_contacts().is_empty());
    assert_eq!(m.contact_name("01555000111"), Some("amir"));
    assert_eq!(m.label("01999999999"), "01999999999");
}

#[test]
fn keypad_keys_are_big_and_do_not_overlap_in_both_orientations() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let g = keypad_geom(&f, body_top());
        assert_eq!(g.keys.len(), 13, "12 keys and +");
        let mut rs: Vec<Rect> = g.keys.iter().map(|k| k.0).collect();
        rs.push(g.call);
        rs.push(g.display);
        for r in &rs {
            assert!(r.h >= TOUCH_MIN && r.w >= TOUCH_MIN, "{r:?}");
            assert!(r.x >= f.padded().x && r.right() <= f.padded().right());
        }
        for (i, a) in rs.iter().enumerate() {
            for b in &rs[i + 1..] {
                let o = a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                assert!(!o, "{a:?} overlaps {b:?}");
            }
        }
        assert!(g.backspace.h >= TOUCH_MIN);
        let chars: String = g.keys.iter().map(|k| k.1).collect();
        assert_eq!(chars, "123456789*0#+");
    }
}

#[test]
fn call_button_needs_a_usable_line_and_a_number() {
    let mut st = crate::app::tests::status(120, &[]);
    st.phone.available = true;
    let st = Some(st);
    assert!(can_call(&st, true, "999"));
    assert!(!can_call(&st, true, ""));
    assert!(!can_call(&st, false, "999"));
    assert!(!can_call(&None, true, "999"));
    let mut busy = st.clone();
    busy.as_mut().unwrap().phone.call = CallState::Active;
    assert!(!can_call(&busy, true, "999"));
    let mut off = st.clone();
    off.as_mut().unwrap().phone.available = false;
    off.as_mut().unwrap().phone.reason = Some("No usable SIM card".into());
    assert!(!can_call(&off, true, "999"));
    assert_eq!(notice_line(&off, true).0, "No usable SIM card");
    assert_eq!(notice_line(&st, true).0, "");
    let mut exp = st.clone();
    exp.as_mut().unwrap().phone.audio = "experimental".into();
    assert!(notice_line(&exp, true).0.contains("experimental"));
    assert_eq!(notice_line(&st, false).1, DANGER);
}

#[test]
fn json_parsers_are_tolerant() {
    let calls = parse_calls(&serde_json::json!({"calls": [
        {"number": "999", "ts": 5, "dur_s": 7, "kind": "missed", "name": null},
        {"number": "1", "kind": "weird"}]}));
    assert_eq!(calls[0].kind, CallKind::Missed);
    assert_eq!(calls[1].kind, CallKind::Out);
    assert!(parse_calls(&Value::Null).is_empty());
    let c = parse_contacts(&serde_json::json!({"contacts": [
        {"id": "c1", "name": "A", "numbers": ["012"]}, {"name": "no id"}]}));
    assert_eq!(c.len(), 1);
    let s = parse_sms(&serde_json::json!({"messages": [
        {"id": 3, "number": "1", "text": "t", "ts": 1, "incoming": true, "read": false, "state": ""}]}));
    assert!(s[0].incoming && !s[0].read);
}

#[test]
fn thread_bubbles_stack_without_overlap_and_total_covers_them() {
    let mut m = model();
    m.view = PhoneView::Messages;
    m.thread = Some("01711234567".into());
    m.sms = (1..=6)
        .map(|i| {
            sms(
                i,
                "01711234567",
                &"word ".repeat(i as usize * 12),
                i * 10,
                i % 2 == 0,
                true,
            )
        })
        .collect();
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let msgs = m.thread_messages();
        let (g, bottom) = thread_geom(&f, 300, &msgs);
        for w in g.windows(2) {
            assert!(w[0].0.bottom() < w[1].0.y);
        }
        assert!(g
            .iter()
            .all(|(r, _)| r.right() <= f.padded().right() && r.x >= f.padded().x));
        let st = Some(crate::app::tests::status(120, &[]));
        assert!(layout_total(&f, &m, &st) >= bottom - 300);
    }
}

#[test]
fn sheets_have_a_cancel_and_stack_inside_the_screen() {
    let mut m = model();
    m.contacts = vec![ContactEntry {
        id: "c1".into(),
        name: "Zara".into(),
        numbers: vec!["01711234567".into(), "01811234567".into()],
    }];
    let (title, b) = sheet_buttons(&PhoneSheet::Contact("c1".into()), &m);
    assert_eq!(title, "Zara");
    let kinds: Vec<PhoneTarget> = b.iter().map(|x| x.0).collect();
    assert_eq!(kinds[0], PhoneTarget::SheetCall(0));
    assert_eq!(kinds[1], PhoneTarget::SheetCall(1));
    assert_eq!(*kinds.last().unwrap(), PhoneTarget::SheetClose);
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let (sheet, rs) = sheet_geom(&f, b.len());
        assert!(sheet.y >= 0 && sheet.bottom() <= f.h);
        assert!(rs
            .iter()
            .all(|r| r.h >= TOUCH_MIN && r.bottom() <= sheet.bottom()));
    }
    m.sms = vec![
        SmsEntry {
            state: "failed".into(),
            ..sms(9, "01711234567", "x", 1, false, true)
        },
        sms(8, "01711234567", "y", 1, false, true),
    ];
    let (_, failed) = sheet_buttons(&PhoneSheet::Message(9), &m);
    assert_eq!(failed[0].0, PhoneTarget::SheetResend);
    let (_, sent) = sheet_buttons(&PhoneSheet::Message(8), &m);
    assert_eq!(sent[0].0, PhoneTarget::SheetDelete);
}
