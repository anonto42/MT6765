use super::*;
use xim::InputStyle;

const A: IcKey = (100, 1, 1);
const B: IcKey = (100, 1, 2);
const C: IcKey = (200, 1, 1);

/// Focus-in at `now` and run the show delay: what comes out.
fn focus(m: &mut FocusKbd, ic: IcKey, now: u64) -> Option<KbdAction> {
    m.focus_in(ic, now);
    m.tick(now + SHOW_DELAY_MS)
}

#[test]
fn focus_in_shows_and_focus_out_hides_after_the_debounce() {
    let mut m = FocusKbd::new();
    m.focus_in(A, 0);
    assert_eq!(m.next_deadline(), Some(SHOW_DELAY_MS));
    assert_eq!(m.tick(SHOW_DELAY_MS - 1), None, "show delay");
    assert_eq!(m.tick(SHOW_DELAY_MS), Some(KbdAction::Show));
    assert!(m.visible());
    assert_eq!(focus(&mut m, A, 500), None, "already up");
    m.focus_out(A, 1000);
    assert_eq!(m.next_deadline(), Some(1000 + HIDE_DEBOUNCE_MS));
    assert_eq!(m.tick(1100), None, "debounced");
    assert!(m.visible());
    assert_eq!(m.tick(1250), Some(KbdAction::Hide));
    assert!(!m.visible());
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.tick(5000), None);
}

#[test]
fn a_focus_that_does_not_stay_never_shows_the_keyboard() {
    // window activation: the last input gets the focus back for a moment, the tap moves on
    let mut m = FocusKbd::new();
    m.focus_in(A, 0);
    m.focus_out(A, 80);
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.tick(1000), None);
    assert!(!m.visible());
    // repeated focus-ins do not push the delay back
    m.focus_in(A, 2000);
    m.focus_in(B, 2100);
    assert_eq!(m.tick(2000 + SHOW_DELAY_MS), Some(KbdAction::Show));
}

#[test]
fn moving_between_two_fields_does_not_flicker() {
    let mut m = FocusKbd::new();
    focus(&mut m, A, 0);
    // out of A, into B within the debounce
    m.focus_out(A, 1000);
    assert_eq!(focus(&mut m, B, 1050), None);
    assert_eq!(m.tick(2000), None);
    assert!(m.visible());
    // the other order (B in before A out) never even arms the timer
    m.focus_in(A, 3000);
    m.focus_out(B, 3000);
    assert_eq!(m.next_deadline(), None);
    assert_eq!(m.tick(9000), None);
    assert!(m.visible());
}

#[test]
fn destroyed_context_and_vanished_client_hide() {
    let mut m = FocusKbd::new();
    focus(&mut m, C, 0);
    m.focus_out(C, 1000); // destroy is reported like a focus-out
    assert_eq!(m.tick(1000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
    focus(&mut m, A, 2000);
    m.focus_in(C, 2000);
    m.client_gone(100, 2010);
    assert_eq!(m.tick(3000), None, "C still has the focus");
    m.client_gone(200, 3000);
    assert_eq!(m.tick(3000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
    // a client without focused inputs going away changes nothing
    m.client_gone(300, 4000);
    assert_eq!(m.next_deadline(), None);
}

#[test]
fn minimise_hides_until_the_next_focus_in() {
    let mut m = FocusKbd::new();
    focus(&mut m, A, 0);
    m.manual(false); // the minimise key while A has the focus
    assert!(!m.visible());
    assert_eq!(m.tick(10_000), None, "stays down");
    m.focus_out(A, 10_000);
    assert_eq!(m.tick(20_000), None);
    assert_eq!(
        focus(&mut m, B, 20_000),
        Some(KbdAction::Show),
        "next focus-in"
    );
    // minimise, then the focus moves on within the debounce: the new field shows it again
    m.manual(false);
    m.focus_out(B, 30_000);
    assert_eq!(focus(&mut m, A, 30_100), Some(KbdAction::Show));
    // minimise right after a focus-in: the pending show is dropped
    let mut m = FocusKbd::new();
    m.focus_in(A, 0);
    m.manual(false);
    assert_eq!(m.tick(SHOW_DELAY_MS), None);
}

#[test]
fn manual_keyboard_without_focus_stays_and_with_focus_follows_it() {
    let mut m = FocusKbd::new();
    // the dock button with nothing focused: stays up whatever happens elsewhere
    m.manual(true);
    m.focus_out(A, 0);
    assert_eq!(m.tick(1000), None);
    assert!(m.visible());
    // a focus-in takes it over: leaving the input then closes it
    assert_eq!(focus(&mut m, A, 1000), None);
    m.focus_out(A, 2000);
    assert_eq!(m.tick(2000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
    // opened by hand while a field has the focus: leaving the field closes it
    focus(&mut m, B, 3000);
    m.manual(false);
    m.manual(true);
    m.focus_out(B, 5000);
    assert_eq!(m.tick(5000 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
    // a manual hide cancels a pending hide timer (nothing left to do)
    focus(&mut m, A, 6000);
    m.focus_out(A, 7000);
    m.manual(false);
    assert_eq!(m.next_deadline(), None);
}

#[test]
fn terminal_fallback_only_without_xim() {
    // no XIM server: the terminal gets the keyboard at once
    let mut m = FocusKbd::new();
    assert_eq!(m.app_started(0, false), Some(KbdAction::Show));
    assert_eq!(m.app_started(0, false), None, "already up");
    // XIM running, but the app never connects: shown after the fallback delay
    let mut m = FocusKbd::new();
    assert_eq!(m.app_started(1000, true), None);
    assert_eq!(m.next_deadline(), Some(1000 + FALLBACK_MS));
    assert_eq!(m.tick(1000 + FALLBACK_MS - 1), None);
    assert_eq!(m.tick(1000 + FALLBACK_MS), Some(KbdAction::Show));
    // the fallback keyboard does not hide on a stray focus-out (it was not opened by focus)
    m.focus_out(A, 9000);
    assert_eq!(m.tick(20_000), None);
    assert!(m.visible());
    // XIM client connects (st does at start-up): its focus decides, no fallback
    let mut m = FocusKbd::new();
    m.app_started(0, true);
    m.client_connected();
    assert_eq!(m.tick(FALLBACK_MS * 2), None);
    assert!(!m.visible());
    // a focus-in within the delay cancels it as well
    let mut m = FocusKbd::new();
    m.app_started(0, true);
    assert_eq!(focus(&mut m, A, 10), Some(KbdAction::Show));
    m.focus_out(A, 500);
    assert_eq!(m.tick(500 + HIDE_DEBOUNCE_MS), Some(KbdAction::Hide));
    assert_eq!(m.tick(FALLBACK_MS * 2), None, "no late fallback");
    // a manual hide cancels it
    let mut m = FocusKbd::new();
    m.app_started(0, true);
    m.manual(false);
    assert_eq!(m.tick(FALLBACK_MS * 2), None);
}

#[test]
fn active_window_change_shows_and_hides_the_fallback_keyboard() {
    // owner report 2026-11-13: once up, the fallback keyboard never hid on switching away -
    // nothing watched for the active window leaving, only app_started (first launch) raised
    // it. Becoming active is the same trigger `app_started` would have been (covers
    // switching back to an *already-running* terminal/Kali/Scanner too, which never sends a
    // fresh "started" event at all).
    let mut m = FocusKbd::new();
    m.active_window_changed(0, true);
    assert_eq!(m.next_deadline(), Some(FALLBACK_MS));
    assert_eq!(m.tick(FALLBACK_MS - 1), None);
    assert_eq!(m.tick(FALLBACK_MS), Some(KbdAction::Show));
    // switching to a non-fallback window hides it (after the usual debounce)
    m.active_window_changed(FALLBACK_MS, false);
    assert_eq!(m.tick(FALLBACK_MS + HIDE_DEBOUNCE_MS - 1), None);
    assert_eq!(
        m.tick(FALLBACK_MS + HIDE_DEBOUNCE_MS),
        Some(KbdAction::Hide)
    );
    // switching straight back to a fallback app before the hide lands cancels it (no flicker)
    let mut m = FocusKbd::new();
    m.active_window_changed(0, true);
    m.tick(FALLBACK_MS);
    assert!(m.visible());
    m.active_window_changed(FALLBACK_MS + 10, false);
    m.active_window_changed(FALLBACK_MS + 20, true);
    assert_eq!(
        m.tick(FALLBACK_MS + 20 + HIDE_DEBOUNCE_MS),
        None,
        "still up, never hid"
    );
    assert!(m.visible());
    // a keyboard a real focus-in brought up is left alone: active-window changes do not
    // touch it, only a real focus-out does (same contract as before this change)
    let mut m = FocusKbd::new();
    focus(&mut m, A, 0);
    m.tick(SHOW_DELAY_MS);
    assert!(m.visible());
    m.active_window_changed(SHOW_DELAY_MS + 10, false);
    assert_eq!(
        m.next_deadline(),
        None,
        "no hide armed - by_focus governs it, not this"
    );
    assert!(m.visible());
}

#[test]
fn the_server_offers_only_styles_without_preedit_or_status() {
    for s in input_styles() {
        assert!(s.intersects(InputStyle::PREEDIT_NOTHING | InputStyle::PREEDIT_NONE));
        assert!(s.intersects(InputStyle::STATUS_NOTHING | InputStyle::STATUS_NONE));
        assert!(!s.intersects(
            InputStyle::PREEDIT_CALLBACKS
                | InputStyle::PREEDIT_POSITION
                | InputStyle::PREEDIT_AREA
                | InputStyle::STATUS_CALLBACKS
                | InputStyle::STATUS_AREA
        ));
    }
    assert!(LOCALES.split(',').any(|l| l == "C"));
    assert!(LOCALES.split(',').any(|l| l == "en_US.UTF-8"));
    assert!(!LOCALES.contains(' '));
}
