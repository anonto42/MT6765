//! VPN and DNS screen (owner request 2026-11-10, change vpn-and-dns).
use super::*;
use crate::vpn_screen::VpnFile;
use walpad_proto::{VpnKind, VpnProfile, VpnState};

fn with_vpn(app: &mut App, f: impl FnOnce(&mut walpad_proto::VpnStatus)) {
    let mut s = app.model.status.clone().unwrap();
    f(&mut s.vpn);
    app.on_core(&CoreEvent::Status(Box::new(s)), 1500);
    render(app);
}

fn profiles(v: &mut walpad_proto::VpnStatus) {
    v.profile = Some("home".into());
    v.kind = Some(VpnKind::Wireguard);
    v.profiles = vec![
        VpnProfile {
            name: "home".into(),
            kind: VpnKind::Wireguard,
        },
        VpnProfile {
            name: "work".into(),
            kind: VpnKind::Openvpn,
        },
    ];
}

fn open(app: &mut App) {
    let out = tap(app, Target::Page(Page::Vpn));
    assert_eq!(app.active, Tab::Vpn);
    assert!(
        out.cmds.contains(&Cmd::VpnFiles),
        "asks for importable files"
    );
    render(app);
}

#[test]
fn switch_kill_switch_and_profile_send_commands() {
    let mut app = online();
    with_vpn(&mut app, profiles);
    app.model.settings_page = None;
    app.active = Tab::Settings;
    render(&mut app);
    open(&mut app);
    assert_eq!(tap(&mut app, Target::VpnSwitch).cmds, vec![Cmd::Vpn(true)]);
    assert_eq!(
        tap(&mut app, Target::VpnKill).cmds,
        vec![Cmd::VpnConfig {
            profile: None,
            kill_switch: Some(true),
            dns: None,
            proxy: None
        }]
    );
    assert_eq!(
        tap(&mut app, Target::VpnProfile).cmds,
        vec![Cmd::VpnConfig {
            profile: Some("work".into()),
            kill_switch: None,
            dns: None,
            proxy: None
        }]
    );
    // running: only the switch works, kill switch and profile are locked
    with_vpn(&mut app, |v| {
        profiles(v);
        v.state = VpnState::On;
    });
    assert!(tap(&mut app, Target::VpnKill).cmds.is_empty());
    assert!(tap(&mut app, Target::VpnProfile).cmds.is_empty());
    assert_eq!(tap(&mut app, Target::VpnSwitch).cmds, vec![Cmd::Vpn(false)]);
    assert_eq!(Cmd::Vpn(true).request(), ("vpn", json!({"on": true})));
    assert_eq!(Cmd::VpnKeep.request(), ("vpn_confirm", Value::Null));
}

#[test]
fn switch_without_a_profile_does_nothing() {
    let mut app = online();
    app.active = Tab::Vpn;
    render(&mut app);
    assert!(tap(&mut app, Target::VpnSwitch).cmds.is_empty());
}

#[test]
fn keep_button_confirms_the_kill_switch() {
    let mut app = online();
    app.active = Tab::Vpn;
    with_vpn(&mut app, |v| {
        profiles(v);
        v.state = VpnState::On;
        v.kill_switch = true;
        v.revert_in_s = Some(41);
    });
    assert_eq!(tap(&mut app, Target::VpnKeep).cmds, vec![Cmd::VpnKeep]);
    with_vpn(&mut app, |v| {
        profiles(v);
        v.state = VpnState::On;
        v.kill_switch = true;
        v.revert_in_s = None;
    });
    assert!(
        app.content_hits.iter().all(|(_, t)| *t != Target::VpnKeep),
        "no countdown: no Keep button"
    );
}

#[test]
fn import_candidates_are_listed_and_imported() {
    let mut app = online();
    app.active = Tab::Vpn;
    app.note_sent(30, &Cmd::VpnFiles);
    app.on_core(
        &CoreEvent::Reply {
            id: 30,
            ok: true,
            data: json!({"files":[{"name":"home","path":"/root/Downloads/home.conf","kind":"wireguard"}]}),
            error: None,
        },
        0,
    );
    assert_eq!(
        app.model.vpn_files,
        vec![VpnFile {
            name: "home".into(),
            path: "/root/Downloads/home.conf".into()
        }]
    );
    render(&mut app);
    let out = tap(&mut app, Target::VpnImport(0));
    assert_eq!(
        out.cmds,
        vec![Cmd::VpnImport("/root/Downloads/home.conf".into())]
    );
    assert_eq!(
        Cmd::VpnImport("/root/a.conf".into()).request(),
        ("vpn_import", json!({"path": "/root/a.conf"}))
    );
    // success refreshes the list, failure toasts
    app.note_sent(31, &Cmd::VpnImport("/root/Downloads/home.conf".into()));
    let out = app.on_core(
        &CoreEvent::Reply {
            id: 31,
            ok: true,
            data: json!({"name":"home"}),
            error: None,
        },
        10,
    );
    assert!(out.cmds.contains(&Cmd::VpnFiles));
    app.note_sent(32, &Cmd::VpnImport("/root/x.conf".into()));
    app.on_core(
        &CoreEvent::Reply {
            id: 32,
            ok: false,
            data: Value::Null,
            error: Some(ReplyError {
                code: "invalid_args".into(),
                msg: "the profile runs scripts (PreUp/PostUp): not allowed".into(),
            }),
        },
        20,
    );
    assert!(app.model.toast.is_some());
}

#[test]
fn dns_and_proxy_sheets_validate_and_save() {
    let mut app = online();
    app.active = Tab::Vpn;
    render(&mut app);
    tap(&mut app, Target::VpnDns);
    assert_eq!(sheet(&app).purpose, SheetPurpose::VpnDns);
    // an invalid entry is refused in the sheet
    if let Some(Overlay::Password(s)) = &mut app.model.overlay {
        s.secret = "not-an-ip".into();
    }
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    assert!(sheet(&app).error.is_some());
    if let Some(Overlay::Password(s)) = &mut app.model.overlay {
        s.secret = "1.1.1.1, 9.9.9.9".into();
        s.error = None;
    }
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(
        out.cmds,
        vec![Cmd::VpnConfig {
            profile: None,
            kill_switch: None,
            dns: Some(vec!["1.1.1.1".into(), "9.9.9.9".into()]),
            proxy: None
        }]
    );
    // the reply closes the sheet
    app.note_sent(40, &out.cmds[0]);
    app.on_core(
        &CoreEvent::Reply {
            id: 40,
            ok: true,
            data: json!({}),
            error: None,
        },
        50,
    );
    assert!(app.model.overlay.is_none());
    tap(&mut app, Target::VpnProxy);
    assert_eq!(sheet(&app).purpose, SheetPurpose::VpnProxy);
    if let Some(Overlay::Password(s)) = &mut app.model.overlay {
        s.secret = "http://user:pw@h:80".into();
    }
    assert!(tap(&mut app, Target::SheetConnect).cmds.is_empty());
    if let Some(Overlay::Password(s)) = &mut app.model.overlay {
        s.secret = "http://10.0.0.2:3128".into();
    }
    let out = tap(&mut app, Target::SheetConnect);
    assert_eq!(
        out.cmds,
        vec![Cmd::VpnConfig {
            profile: None,
            kill_switch: None,
            dns: None,
            proxy: Some("http://10.0.0.2:3128".into())
        }]
    );
}

#[test]
fn failure_and_revert_events_toast() {
    let mut app = online();
    app.on_core(
        &CoreEvent::Wifi(
            json!({"kind":"vpn","state":"failed","reason":"reverted: the kill switch was not confirmed in 60 s"}),
        ),
        100,
    );
    let t = app.model.toast.clone().unwrap();
    assert!(t.text.starts_with("VPN: reverted"), "{}", t.text);
    assert_eq!(t.tone, Tone::Danger);
}
