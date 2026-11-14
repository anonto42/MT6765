//! Wi-Fi switch and hotspot (owner request 2026-11-10): Network screen and Quick Settings.
use super::*;
use crate::overlay::PasswordSheet;
use crate::panels;
use walpad_proto::{HotspotInternet, HotspotState, HotspotStatus};

fn with_status(app: &mut App, f: impl FnOnce(&mut Status)) {
    let mut s = app.model.status.clone().unwrap();
    f(&mut s);
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

#[test]
fn wifi_switch_in_quick_settings_and_on_the_network_screen() {
    let mut app = online();
    quick(&mut app);
    let out = tap(&mut app, Target::WifiSwitch);
    assert_eq!(out.cmds, vec![Cmd::WifiEnable(false)]);
    assert_eq!(
        app.model.overlay,
        Some(Overlay::QuickSettings),
        "a quick toggle keeps the dropdown open"
    );
    with_status(&mut app, |s| {
        s.wifi.enabled = false;
        s.wifi.ssid = None;
    });
    assert_eq!(panels::wifi_row(&app.model).0, "Off");
    assert_eq!(
        tap(&mut app, Target::WifiSwitch).cmds,
        vec![Cmd::WifiEnable(true)]
    );
    // the row itself opens the Network screen, whose header has the same switch
    tap(&mut app, Target::WifiCard);
    assert_eq!(app.active, Tab::Network);
    assert!(app.model.overlay.is_none());
    assert_eq!(
        tap(&mut app, Target::WifiSwitch).cmds,
        vec![Cmd::WifiEnable(true)]
    );
    // switched off: no Scan
    assert!(tap(&mut app, Target::Scan).cmds.is_empty());
    assert_eq!(
        Cmd::WifiEnable(false).request(),
        ("wifi_enable", json!({"on": false}))
    );
}

#[test]
fn hotspot_without_password_opens_the_password_sheet() {
    let mut app = online();
    quick(&mut app);
    let out = tap(&mut app, Target::HotspotSwitch);
    assert_eq!(
        out.cmds,
        vec![Cmd::WifiListSaved],
        "Network opens, no hotspot command"
    );
    assert_eq!(app.active, Tab::Network);
    let s = sheet(&app);
    assert_eq!(
        (s.purpose, s.error.as_deref()),
        (SheetPurpose::HotspotPassword, Some("Set a password first"))
    );
    // Save stays disabled below 8 characters
    type_text(&mut app, "short");
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    type_text(&mut app, " pass");
    let out = tap(&mut app, Target::SheetConnect);
    let [cmd @ Cmd::HotspotConfig {
        ssid: None,
        secret: Some(sec),
    }] = &out.cmds[..]
    else {
        panic!("{:?}", out.cmds)
    };
    assert_eq!(sec.expose(), "short pass");
    assert!(!format!("{cmd:?}").contains("short"), "{cmd:?}");
    assert_eq!(
        cmd.request(),
        ("hotspot_config", json!({"secret": "short pass"}))
    );
    assert!(sheet(&app).connecting);
    app.note_sent(40, cmd);
    reply(&mut app, 40, json!({"ssid": "WalPad", "has_secret": true}));
    assert!(app.model.overlay.is_none());
    assert_eq!(
        app.model.toast.as_ref().map(|t| t.text.as_str()),
        Some("Hotspot settings saved")
    );
}

#[test]
fn hotspot_name_sheet_and_rejected_save() {
    let mut app = online();
    go(&mut app, Tab::Network);
    tap(&mut app, Target::HotspotName);
    let s = sheet(&app);
    assert_eq!(
        (s.purpose, s.secret.as_str(), s.shown),
        (SheetPurpose::HotspotName, "WalPad", true)
    );
    type_text(&mut app, " two");
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(
        out.cmds,
        vec![Cmd::HotspotConfig {
            ssid: Some("WalPad two".into()),
            secret: None
        }]
    );
    app.note_sent(41, &out.cmds[0]);
    reply_err(&mut app, 41, "busy", "the hotspot is starting or stopping");
    let s = sheet(&app);
    assert_eq!(
        (s.connecting, s.error.as_deref()),
        (false, Some("the hotspot is starting or stopping"))
    );
    // a name is capped at 32 bytes
    let mut long = PasswordSheet::hotspot(SheetPurpose::HotspotName, &"x".repeat(32));
    assert!(long.valid());
    long.secret.push('y');
    assert!(!long.valid() && long.max_len() == 32);
}

#[test]
fn hotspot_switch_turns_it_on_and_off_and_events_toast() {
    let mut app = online();
    with_status(&mut app, |s| {
        s.hotspot = HotspotStatus {
            has_secret: true,
            ssid: Some("Tab".into()),
            ..Default::default()
        }
    });
    quick(&mut app);
    assert_eq!(
        tap(&mut app, Target::HotspotSwitch).cmds,
        vec![Cmd::Hotspot(true)]
    );
    assert_eq!(app.model.overlay, Some(Overlay::QuickSettings));
    with_status(&mut app, |s| {
        s.hotspot.state = HotspotState::On;
        s.hotspot.clients = Some(2);
        s.hotspot.exclusive = true;
        s.hotspot.internet = Some(HotspotInternet::NoUplink);
    });
    assert_eq!(panels::hotspot_row(&app.model).0, "On, 2 devices");
    wifi_event(
        &mut app,
        json!({"kind":"hotspot","state":"on","exclusive":true}),
    );
    assert_eq!(
        app.model.toast.as_ref().map(|t| t.text.as_str()),
        Some("Hotspot on, Wi-Fi client paused")
    );
    assert_eq!(
        tap(&mut app, Target::HotspotSwitch).cmds,
        vec![Cmd::Hotspot(false)]
    );
    // the row opens the Network screen
    tap(&mut app, Target::HotspotCard);
    assert_eq!(app.active, Tab::Network);
    assert_eq!(
        tap(&mut app, Target::HotspotSwitch).cmds,
        vec![Cmd::Hotspot(false)]
    );
    wifi_event(
        &mut app,
        json!({"kind":"hotspot","state":"failed","reason":"hostapd is not installed"}),
    );
    assert_eq!(
        app.model.toast.as_ref().map(|t| (t.text.as_str(), t.tone)),
        Some(("Hotspot failed: hostapd is not installed", Tone::Danger))
    );
}

#[test]
fn switches_do_nothing_while_the_core_is_away() {
    let mut app = App::new(800, 1280, catalog(), wall());
    render(&mut app);
    quick(&mut app);
    assert!(tap(&mut app, Target::WifiSwitch).cmds.is_empty());
    assert!(tap(&mut app, Target::HotspotSwitch).cmds.is_empty());
    assert!(app.model.overlay.as_ref().is_some_and(Overlay::is_popover));
}
