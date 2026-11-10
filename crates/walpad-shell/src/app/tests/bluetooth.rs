//! Bluetooth screen, Quick Settings row and pairing dialog (owner request 2026-11-10).
use super::*;
use crate::bluetooth::PairState;
use walpad_proto::{BluetoothController, BluetoothDevice, BluetoothStatus};

const KBD: &str = "AA:BB:CC:00:11:22";
const PHONE: &str = "66:55:44:33:22:11";

fn dev(a: &str, name: &str, paired: bool, connected: bool) -> BluetoothDevice {
    BluetoothDevice {
        address: a.into(),
        name: name.into(),
        paired,
        connected,
        rssi: (!paired).then_some(-60),
        ..Default::default()
    }
}

fn set_bt(app: &mut App, f: impl FnOnce(&mut BluetoothStatus)) {
    let mut s = app.model.status.clone().unwrap();
    f(&mut s.bluetooth);
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

fn powered(b: &mut BluetoothStatus) {
    b.controller = BluetoothController::Present;
    b.powered = true;
    b.devices = vec![
        dev(KBD, "Logitech K380", true, false),
        dev(PHONE, "Test Phone", false, false),
    ];
}

fn notice(app: &mut App, v: Value) -> Outcome {
    let o = app.on_core(&CoreEvent::Notice(v), 2000);
    render(app);
    o
}

fn open_screen(app: &mut App) {
    quick(app);
    tap(app, Target::BtCard);
    assert_eq!(app.active, Tab::Bluetooth);
    assert!(app.model.overlay.is_none());
}

#[test]
fn quick_settings_row_has_a_switch_and_opens_the_screen() {
    let mut app = online();
    set_bt(&mut app, |b| b.controller = BluetoothController::Present);
    quick(&mut app);
    let out = tap(&mut app, Target::BtSwitch);
    assert_eq!(out.cmds, vec![Cmd::BtEnable(true)]);
    assert_eq!(app.model.overlay, Some(Overlay::QuickSettings));
    set_bt(&mut app, powered);
    quick(&mut app);
    assert_eq!(
        tap(&mut app, Target::BtSwitch).cmds,
        vec![Cmd::BtEnable(false)]
    );
    assert_eq!(
        Cmd::BtEnable(true).request(),
        ("bluetooth_enable", json!({"on": true}))
    );
    tap(&mut app, Target::BtCard);
    assert_eq!(app.active, Tab::Bluetooth);
}

#[test]
fn unsupported_bluetooth_ignores_the_switch() {
    let mut app = online();
    set_bt(&mut app, |b| {
        b.reason = Some("Bluetooth is not installed on this system".into())
    });
    quick(&mut app);
    assert!(tap(&mut app, Target::BtSwitch).cmds.is_empty());
}

#[test]
fn settings_has_a_bluetooth_entry() {
    let mut app = online();
    tap(&mut app, Target::Nav(crate::chrome::NAV_SETTINGS));
    assert_eq!(app.active, Tab::Settings);
    tap(&mut app, Target::Page(Page::Bluetooth));
    assert_eq!(app.active, Tab::Bluetooth);
}

#[test]
fn scan_button_toggles_discovery() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    assert_eq!(tap(&mut app, Target::BtScan).cmds, vec![Cmd::BtScan(true)]);
    assert_eq!(
        Cmd::BtScan(true).request(),
        ("bluetooth_scan", json!({"on": true}))
    );
    set_bt(&mut app, |b| b.discovering = true);
    assert!(app.model.spinning(), "the spinner runs while scanning");
    assert_eq!(tap(&mut app, Target::BtScan).cmds, vec![Cmd::BtScan(false)]);
}

#[test]
fn off_controller_shows_only_the_state_card() {
    let mut app = online();
    open_screen(&mut app);
    // no lists, no scan: nothing to tap but the switch
    assert!(app
        .content_hits
        .iter()
        .all(|(_, t)| matches!(t, Target::BtSwitch)));
}

#[test]
fn connect_disconnect_and_forget_from_my_devices() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    let out = tap(&mut app, Target::BtAction(0));
    assert_eq!(out.cmds, vec![Cmd::BtConnect(KBD.into())]);
    assert_eq!(app.model.bt_busy.as_deref(), Some(KBD));
    // one request at a time
    assert!(tap(&mut app, Target::BtAction(0)).cmds.is_empty());
    // the notice ends it; a connected device offers Disconnect
    notice(
        &mut app,
        json!({"kind":"bluetooth","op":"connect","address":KBD,"ok":true,"error":null}),
    );
    assert!(app.model.bt_busy.is_none());
    set_bt(&mut app, |b| b.devices[0].connected = true);
    assert_eq!(
        tap(&mut app, Target::BtAction(0)).cmds,
        vec![Cmd::BtDisconnect(KBD.into())]
    );
    notice(
        &mut app,
        json!({"kind":"bluetooth","op":"disconnect","address":KBD,"ok":true,"error":null}),
    );
    // forget asks first
    assert!(tap(&mut app, Target::BtForget(0)).cmds.is_empty());
    assert!(matches!(app.model.overlay, Some(Overlay::BtForget { .. })));
    assert_eq!(
        tap(&mut app, Target::DialogConfirm).cmds,
        vec![Cmd::BtForget(KBD.into())]
    );
    assert_eq!(
        Cmd::BtForget(KBD.into()).request(),
        ("bluetooth_forget", json!({"address": KBD}))
    );
}

#[test]
fn failed_connect_is_a_toast_and_frees_the_row() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    tap(&mut app, Target::BtAction(0));
    notice(
        &mut app,
        json!({"kind":"bluetooth","op":"connect","address":KBD,"ok":false,"error":"Could not connect"}),
    );
    assert!(app.model.bt_busy.is_none());
    let t = app.model.toast.clone().unwrap();
    assert!(t.text.contains("Logitech K380") && t.text.contains("Could not connect"));
}

#[test]
fn pairing_with_passkey_confirmation() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    let out = tap(&mut app, Target::BtAvail(0));
    assert_eq!(out.cmds, vec![Cmd::BtPair(PHONE.into())]);
    assert_eq!(
        Cmd::BtPair(PHONE.into()).request(),
        ("bluetooth_pair", json!({"address": PHONE}))
    );
    assert!(matches!(
        &app.model.overlay,
        Some(Overlay::BtPair {
            state: PairState::Waiting,
            ..
        })
    ));
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"confirm","passkey":"123456"}),
    );
    assert!(matches!(
        &app.model.overlay,
        Some(Overlay::BtPair { state: PairState::Confirm(c), name, .. })
            if c == "123456" && name == "Test Phone"
    ));
    let out = tap(&mut app, Target::BtPairAccept);
    assert_eq!(
        out.cmds,
        vec![Cmd::BtConfirm {
            address: PHONE.into(),
            accept: true
        }]
    );
    assert_eq!(
        Cmd::BtConfirm {
            address: PHONE.into(),
            accept: true
        }
        .request(),
        (
            "bluetooth_confirm",
            json!({"address": PHONE, "accept": true})
        )
    );
    assert!(matches!(
        &app.model.overlay,
        Some(Overlay::BtPair {
            state: PairState::Waiting,
            ..
        })
    ));
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"ok"}),
    );
    assert!(app.model.overlay.is_none() && app.model.bt_busy.is_none());
    assert!(app.model.toast.clone().unwrap().text.contains("Paired"));
}

#[test]
fn rejecting_or_failing_a_pairing() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    tap(&mut app, Target::BtAvail(0));
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"confirm","passkey":"000042"}),
    );
    let out = tap(&mut app, Target::BtPairReject);
    assert_eq!(
        out.cmds,
        vec![Cmd::BtConfirm {
            address: PHONE.into(),
            accept: false
        }]
    );
    assert!(app.model.overlay.is_none() && app.model.bt_busy.is_none());
    // a failure while the dialog is up is shown in it
    tap(&mut app, Target::BtAvail(0));
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"failed","error":"Authentication failed"}),
    );
    assert!(matches!(
        &app.model.overlay,
        Some(Overlay::BtPair { state: PairState::Failed(e), .. }) if e == "Authentication failed"
    ));
    tap(&mut app, Target::SheetCancel);
    assert!(app.model.overlay.is_none());
    // a failure with the dialog closed is a toast
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"failed","error":"Pairing timed out"}),
    );
    assert!(app
        .model
        .toast
        .clone()
        .unwrap()
        .text
        .contains("Pairing timed out"));
}

#[test]
fn keyboard_passkey_is_displayed() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    tap(&mut app, Target::BtAvail(0));
    notice(
        &mut app,
        json!({"kind":"bluetooth_pair","address":PHONE,"state":"passkey","passkey":"482913"}),
    );
    assert!(matches!(
        &app.model.overlay,
        Some(Overlay::BtPair { state: PairState::Display(c), .. }) if c == "482913"
    ));
    tap(&mut app, Target::SheetCancel);
    assert!(app.model.overlay.is_none());
}

#[test]
fn refused_pair_reply_closes_the_dialog() {
    let mut app = online();
    set_bt(&mut app, powered);
    open_screen(&mut app);
    let out = tap(&mut app, Target::BtAvail(0));
    app.note_sent(77, &out.cmds[0]);
    reply_err(&mut app, 77, "refused", "Bluetooth is turned off");
    assert!(app.model.overlay.is_none() && app.model.bt_busy.is_none());
    assert!(app.model.toast.clone().unwrap().text.contains("turned off"));
}

fn pill_w(app: &App) -> i32 {
    app.hits
        .iter()
        .find(|(_, t)| *t == Target::QuickSettings)
        .map(|(r, _)| r.w)
        .unwrap()
}

#[test]
fn header_pill_shows_the_mark_only_while_powered() {
    let mut app = online();
    let before = pill_w(&app);
    set_bt(&mut app, powered);
    assert!(pill_w(&app) > before);
}
