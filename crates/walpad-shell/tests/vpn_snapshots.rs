//! VPN and DNS screen snapshots (owner request 2026-11-10, change vpn-and-dns).
use serde_json::json;
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, ScreenState, Status, VpnKind, VpnProfile, VpnState,
    VpnStatus, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::overlay::{Overlay, PasswordSheet, SheetPurpose};
use walpad_ui::components::{Painter, Tab};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn status(vpn: VpnStatus) -> Status {
    Status {
        battery: Battery {
            percent: Some(62),
            source: BatterySource::VoltageEstimate,
            state: Some("Discharging".into()),
            mv: Some(3920),
            temp_c: Some(34.0),
            low: false,
        },
        screen: ScreenState::On,
        brightness: 120,
        orientation: 0,
        rotation_locked: false,
        lid_closed: false,
        wifi: WifiStatus {
            radio: WifiRadio::Up,
            state: Some("COMPLETED".into()),
            ssid: Some("Home".into()),
            ip: Some("192.168.0.100".into()),
            signal_dbm: Some(-58),
            enabled: true,
        },
        hotspot: Default::default(),
        mobile: Default::default(),
        phone: Default::default(),
        bluetooth: Default::default(),
        lock: Default::default(),
        audio: Default::default(),
        vpn,
        apps: vec![AppState {
            app: "browser".into(),
            running: false,
        }],
        hardware: Hardware {
            touch: "ok".into(),
            gsensor: "ok".into(),
            audio: "unverified".into(),
            bluetooth: "unavailable".into(),
            sd_card: "unavailable".into(),
        },
    }
}

fn profiles() -> Vec<VpnProfile> {
    vec![
        VpnProfile {
            name: "home".into(),
            kind: VpnKind::Wireguard,
        },
        VpnProfile {
            name: "work".into(),
            kind: VpnKind::Openvpn,
        },
    ]
}

fn app(w: i32, h: i32, vpn: Option<VpnStatus>) -> App {
    let mut a = App::new(
        w,
        h,
        vec![AppEntry {
            id: "browser".into(),
            name: "Browser".into(),
            icon: Icon::Globe,
        }],
        WallTime::new(14, 22, "Monday, 09 November", true),
    );
    if let Some(v) = vpn {
        a.on_core(&CoreEvent::Status(Box::new(status(v))), 0);
    }
    a.active = Tab::Vpn;
    a
}

fn shot(name: &str, w: i32, h: i32, a: &mut App) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let mut c = Canvas::new(w as u32, h as u32, BG).unwrap();
    a.render(&mut Painter {
        cv: &mut c,
        fonts: &mut f,
    });
    assert_snapshot_in(&golden(), name, &c);
}

fn connected() -> VpnStatus {
    VpnStatus {
        state: VpnState::On,
        profile: Some("home".into()),
        kind: Some(VpnKind::Wireguard),
        profiles: profiles(),
        kill_switch: true,
        dns: vec!["1.1.1.1".into(), "9.9.9.9".into()],
        proxy: Some("http://10.0.0.2:3128".into()),
        handshake_age_s: Some(12),
        rx_bytes: Some(3_400_000),
        tx_bytes: Some(120_000),
        ..Default::default()
    }
}

#[test]
fn vpn_empty_and_offline() {
    let mut a = app(800, 1280, Some(VpnStatus::default()));
    a.model.vpn_files = vec![walpad_shell::vpn_screen::VpnFile {
        name: "home".into(),
        path: "/root/Downloads/home.conf".into(),
    }];
    shot("vpn_no_profile_import_offer", 800, 1280, &mut a);
    let mut b = app(800, 1280, None);
    shot("vpn_core_offline", 800, 1280, &mut b);
}

#[test]
fn vpn_connected_portrait_and_landscape() {
    let mut a = app(800, 1280, Some(connected()));
    shot("vpn_connected_portrait", 800, 1280, &mut a);
    let mut l = app(1280, 800, Some(connected()));
    shot("vpn_connected_landscape", 1280, 800, &mut l);
}

#[test]
fn vpn_off_failed_and_confirm() {
    let off = VpnStatus {
        profile: Some("home".into()),
        kind: Some(VpnKind::Wireguard),
        profiles: profiles(),
        ..Default::default()
    };
    shot(
        "vpn_off_with_profiles",
        800,
        1280,
        &mut app(800, 1280, Some(off.clone())),
    );
    let failed = VpnStatus {
        state: VpnState::Failed,
        reason: Some("no handshake with the server (check the endpoint and the keys)".into()),
        ..off
    };
    shot("vpn_failed", 800, 1280, &mut app(800, 1280, Some(failed)));
    let confirm = VpnStatus {
        revert_in_s: Some(41),
        ..connected()
    };
    shot(
        "vpn_keep_kill_switch",
        800,
        1280,
        &mut app(800, 1280, Some(confirm)),
    );
}

#[test]
fn vpn_sheets() {
    let mut a = app(800, 1280, Some(connected()));
    a.model.overlay = Some(Overlay::Password(PasswordSheet::vpn(
        SheetPurpose::VpnDns,
        "1.1.1.1, 9.9.9.9",
    )));
    shot("vpn_dns_sheet", 800, 1280, &mut a);
    let mut s = PasswordSheet::vpn(SheetPurpose::VpnProxy, "http://user:pw@h:80");
    s.error = Some("Use http://host:port, no user name or password".into());
    a.model.overlay = Some(Overlay::Password(s));
    shot("vpn_proxy_sheet_error", 800, 1280, &mut a);
}

#[test]
fn vpn_status_json_has_no_secret() {
    let j = serde_json::to_string(&connected()).unwrap();
    assert!(!j.contains("key") && !j.contains("secret"), "{j}");
    let _ = json!({});
}
