//! Mobile network screen and the Quick Settings Mobile data row (owner request 2026-11-10).
use super::*;
use walpad_proto::{MobileData, MobileReg, MobileStatus, ModemState, SimState};

fn set_mobile(app: &mut App, f: impl FnOnce(&mut MobileStatus)) {
    let mut s = app.model.status.clone().unwrap();
    f(&mut s.mobile);
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

fn registered(m: &mut MobileStatus) {
    m.modem = ModemState::Ready;
    m.radio_on = Some(true);
    m.sim = SimState::Ready;
    m.registration = MobileReg::Home;
    m.operator = Some("Grameenphone".into());
    m.tech = Some("4G".into());
    m.signal_bars = Some(4);
    m.signal_dbm = Some(-55);
}

fn notice(app: &mut App, v: Value) -> Outcome {
    let o = app.on_core(&CoreEvent::Notice(v), 2000);
    render(app);
    o
}

#[test]
fn quick_settings_row_opens_the_screen_and_the_switch_toggles_data() {
    let mut app = online();
    set_mobile(&mut app, registered);
    quick(&mut app);
    let out = tap(&mut app, Target::MobileDataSwitch);
    assert_eq!(out.cmds, vec![Cmd::MobileData(true)]);
    assert_eq!(app.model.overlay, Some(Overlay::QuickSettings));
    assert_eq!(
        Cmd::MobileData(true).request(),
        ("mobile_data", json!({"on": true}))
    );
    tap(&mut app, Target::MobileCard);
    assert_eq!(app.active, Tab::Mobile);
    assert!(app.model.overlay.is_none());
    // airplane mode from the screen
    assert_eq!(
        tap(&mut app, Target::MobileRadio).cmds,
        vec![Cmd::MobileRadio(false)]
    );
    set_mobile(&mut app, |m| {
        m.radio_on = Some(false);
        m.registration = MobileReg::NotRegistered;
    });
    assert!(
        tap(&mut app, Target::MobileDataSwitch).cmds.is_empty(),
        "no data with the radio off"
    );
    assert_eq!(
        tap(&mut app, Target::MobileRadio).cmds,
        vec![Cmd::MobileRadio(true)]
    );
}

#[test]
fn settings_has_a_mobile_network_entry() {
    let mut app = online();
    go(&mut app, Tab::Settings);
    tap(&mut app, Target::Page(Page::Mobile));
    assert_eq!(app.active, Tab::Mobile);
}

#[test]
fn sim_pin_sheet_sends_a_secret_and_waits_for_the_answer() {
    let mut app = online();
    set_mobile(&mut app, |m| {
        m.modem = ModemState::Ready;
        m.radio_on = Some(true);
        m.sim = SimState::PinRequired;
    });
    quick(&mut app);
    tap(&mut app, Target::MobileCard);
    tap(&mut app, Target::SimPinEntry);
    let s = sheet(&app);
    assert_eq!(s.purpose, SheetPurpose::SimPin);
    assert_eq!(s.layer, KeyboardLayer::Symbols, "digits first");
    type_text(&mut app, "12");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    assert!(sheet(&app).error.is_some());
    type_text(&mut app, "34");
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(out.cmds, vec![Cmd::SimPin(Secret::new("1234"))]);
    assert!(!format!("{:?}", out.cmds).contains("1234"), "never printed");
    assert_eq!(
        Cmd::SimPin(Secret::new("1234")).request(),
        ("sim_pin", json!({"pin": "1234"}))
    );
    app.note_sent(40, &out.cmds[0]);
    reply(&mut app, 40, json!({"accepted": true}));
    assert!(sheet(&app).connecting, "waits for the modem");
    notice(
        &mut app,
        json!({"kind":"mobile_pin","ok":false,"error":"Wrong PIN"}),
    );
    let s = sheet(&app);
    assert_eq!(
        (s.error.as_deref(), s.secret.as_str()),
        (Some("Wrong PIN"), "")
    );
    type_text(&mut app, "4321");
    tap(&mut app, Target::SheetConnect);
    notice(
        &mut app,
        json!({"kind":"mobile_pin","ok":true,"error":null}),
    );
    assert!(app.model.overlay.is_none());
    assert_eq!(app.model.toast.as_ref().unwrap().text, "SIM unlocked");
}

#[test]
fn apn_sheet_saves_and_imei_shows_on_request() {
    let mut app = online();
    set_mobile(&mut app, |m| {
        registered(m);
        m.apn = Some("internet".into());
    });
    quick(&mut app);
    tap(&mut app, Target::MobileCard);
    tap(&mut app, Target::MobileApn);
    assert_eq!(
        (sheet(&app).purpose, sheet(&app).secret.as_str()),
        (SheetPurpose::MobileApn, "internet")
    );
    for _ in 0..8 {
        app.on_gesture(&Gesture::Tap { x: 0, y: 0 }, 0); // outside: the sheet is modal
    }
    render(&mut app);
    let k = keyboard_layout(
        overlay::password_geom(&app.frame).keyboard,
        KeyboardLayer::Lower,
    )
    .into_iter()
    .find(|k| k.key == Key::Backspace)
    .unwrap();
    for _ in 0..8 {
        let (x, y) = k.rect.center();
        tap_at(&mut app, x, y, 1000);
    }
    type_text(&mut app, "gpinternet");
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(out.cmds, vec![Cmd::MobileApn("gpinternet".into())]);
    app.note_sent(41, &out.cmds[0]);
    reply(&mut app, 41, json!({"apn":"gpinternet","reconnect":false}));
    assert!(app.model.overlay.is_none());
    assert_eq!(app.model.toast.as_ref().unwrap().text, "APN saved");

    // IMEI: hidden, "Show" asks the core once
    let out = tap(&mut app, Target::ImeiShow);
    assert_eq!(out.cmds, vec![Cmd::MobileInfo]);
    assert_eq!(
        crate::mobile::imei_value(&app.model.mobile_info, true),
        "Loading..."
    );
    app.note_sent(42, &out.cmds[0]);
    reply(&mut app, 42, json!({"imei":"351234567890123","iccid":null}));
    assert_eq!(
        crate::mobile::imei_value(&app.model.mobile_info, app.model.imei_shown),
        "351234567890123"
    );
    assert!(tap(&mut app, Target::ImeiShow).cmds.is_empty(), "hide");
    assert!(!app.model.imei_shown);
}

#[test]
fn data_failure_and_header_bars() {
    let mut app = online();
    assert_eq!(
        crate::mobile::pill_cell(&app.model.status),
        None,
        "no modem: no bars"
    );
    set_mobile(&mut app, registered);
    assert_eq!(crate::mobile::pill_cell(&app.model.status), Some((4, TEXT)));
    notice(
        &mut app,
        json!({"kind":"mobile_data","state":"failed","reason":"data link not supported yet"}),
    );
    assert_eq!(
        app.model.toast.as_ref().unwrap().text,
        "Mobile data failed: data link not supported yet"
    );
    set_mobile(&mut app, |m| {
        m.data = MobileData::Failed;
        m.data_reason = Some("data link not supported yet".into());
    });
    quick(&mut app);
    assert_eq!(crate::mobile::qs_line(&app.model.status).0, "Data failed");
}
