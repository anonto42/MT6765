use super::*;

fn st(call: CallState) -> PhoneStatus {
    PhoneStatus {
        available: true,
        call,
        incoming: call == CallState::Ringing,
        number: Some("01711234567".into()),
        audio: "experimental".into(),
        ..Default::default()
    }
}

#[test]
fn buttons_are_big_and_inside_the_screen() {
    for area in [(800, 1280), (1280, 800)] {
        for (call, kp) in [
            (CallState::Ringing, false),
            (CallState::Dialing, false),
            (CallState::Active, false),
            (CallState::Active, true),
            (CallState::Held, false),
        ] {
            let l = layout(area, call, kp);
            assert!(!l.is_empty());
            for (i, (r, t)) in l.iter().enumerate() {
                assert!(r.w >= 56 && r.h >= 56, "{t:?} {r:?}");
                assert!(
                    r.x >= 0 && r.y >= 0 && r.right() <= area.0 && r.bottom() <= area.1,
                    "{call:?} {t:?} {r:?} in {area:?}"
                );
                for (r2, t2) in &l[i + 1..] {
                    let o = r.x < r2.right()
                        && r2.x < r.right()
                        && r.y < r2.bottom()
                        && r2.y < r.bottom();
                    assert!(!o, "{call:?}: {t:?} overlaps {t2:?}");
                }
            }
        }
    }
    assert!(layout((800, 1280), CallState::Ended, false).is_empty());
    assert!(layout((800, 1280), CallState::Idle, false).is_empty());
}

#[test]
fn ringing_offers_answer_and_decline_only() {
    let l = layout((800, 1280), CallState::Ringing, false);
    let t: Vec<CallTarget> = l.iter().map(|x| x.1).collect();
    assert_eq!(t, [CallTarget::Decline, CallTarget::Answer]);
}

#[test]
fn taps_become_acts_and_a_slid_off_press_is_cancelled() {
    let mut s = CallScreen::new((800, 1280));
    s.set_status(&st(CallState::Ringing), 0);
    let l = layout(s.area, CallState::Ringing, false);
    let (a, _) = l.iter().find(|x| x.1 == CallTarget::Answer).unwrap();
    let (cx, cy) = a.center();
    assert_eq!(s.down(cx, cy), Out::Redraw);
    assert_eq!(s.up(cx, cy), Out::Act(CallAct::Answer));
    // press on answer, release elsewhere: nothing
    s.down(cx, cy);
    assert_eq!(s.up(5, 5), Out::Redraw);
    s.down(cx, cy);
    assert_eq!(s.moved(5, 5), Out::Redraw);
    assert_eq!(s.up(cx, cy), Out::None);
    // a tap in the empty area does nothing
    assert_eq!(s.down(5, 5), Out::None);
    assert_eq!(s.up(5, 5), Out::None);
    // the corner of a circle's box is not part of the button
    let corner = (a.x + 3, a.y + 3);
    assert_eq!(s.down(corner.0, corner.1), Out::None);
    // decline = hangup
    let (d, _) = l.iter().find(|x| x.1 == CallTarget::Decline).unwrap();
    let (dx, dy) = d.center();
    s.down(dx, dy);
    assert_eq!(s.up(dx, dy), Out::Act(CallAct::Hangup));
}

#[test]
fn in_call_controls_toggle_and_send_tones() {
    let mut s = CallScreen::new((1280, 800));
    let mut a = st(CallState::Active);
    a.elapsed_s = 59;
    s.set_status(&a, 1000);
    let tap = |s: &mut CallScreen, t: CallTarget| {
        let (r, _) = layout(s.area, s.st.call, s.keypad)
            .into_iter()
            .find(|x| x.1 == t)
            .unwrap();
        let (x, y) = r.center();
        s.down(x, y);
        s.up(x, y)
    };
    assert_eq!(tap(&mut s, CallTarget::Mute), Out::Act(CallAct::Mute(true)));
    a.muted = true;
    a.speaker = true;
    s.set_status(&a, 1000);
    assert_eq!(
        tap(&mut s, CallTarget::Mute),
        Out::Act(CallAct::Mute(false))
    );
    assert_eq!(
        tap(&mut s, CallTarget::Speaker),
        Out::Act(CallAct::Speaker(false))
    );
    assert_eq!(tap(&mut s, CallTarget::Keypad), Out::Redraw);
    assert!(s.keypad);
    assert_eq!(
        tap(&mut s, CallTarget::Digit('5')),
        Out::Act(CallAct::Dtmf('5'))
    );
    assert_eq!(
        tap(&mut s, CallTarget::Digit('#')),
        Out::Act(CallAct::Dtmf('#'))
    );
    assert_eq!(s.typed, "5#");
    assert_eq!(tap(&mut s, CallTarget::HideKeypad), Out::Redraw);
    assert!(!s.keypad);
    assert_eq!(tap(&mut s, CallTarget::Hangup), Out::Act(CallAct::Hangup));
    // a new call starts with a clean dial pad
    s.keypad = true;
    s.set_status(&st(CallState::Idle), 5000);
    s.set_status(&st(CallState::Dialing), 6000);
    assert!(!s.keypad && s.typed.is_empty());
}

#[test]
fn timer_counts_on_from_the_core_and_ticks_each_second() {
    let mut s = CallScreen::new((800, 1280));
    let mut a = st(CallState::Active);
    a.elapsed_s = 59;
    s.set_status(&a, 10_000);
    assert_eq!(s.headline(), "0:59");
    assert!(!s.tick(10_500));
    assert!(s.tick(11_000));
    assert_eq!(s.headline(), "1:00");
    assert_eq!(s.next_deadline(11_250), Some(750));
    s.set_status(&st(CallState::Ringing), 20_000);
    assert_eq!(s.next_deadline(20_000), None);
    assert_eq!(s.headline(), "Incoming call");
    let mut e = st(CallState::Ended);
    e.end_reason = Some("busy".into());
    s.set_status(&e, 30_000);
    assert_eq!(s.headline(), "Busy");
    assert!(s.wanted());
    s.set_status(&st(CallState::Idle), 31_000);
    assert!(!s.wanted());
}

#[test]
fn the_caller_shows_by_name_or_number() {
    let mut s = CallScreen::new((800, 1280));
    s.set_status(&st(CallState::Ringing), 0);
    assert_eq!(s.who(), ("01711234567".to_string(), None));
    let mut n = st(CallState::Ringing);
    n.name = Some("Rahim".into());
    s.set_status(&n, 0);
    assert_eq!(
        s.who(),
        ("Rahim".to_string(), Some("01711234567".to_string()))
    );
    n.number = None;
    n.name = None;
    s.set_status(&n, 0);
    assert_eq!(s.who().0, "Unknown number");
}
