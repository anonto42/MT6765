//! M5.5 snapshots: Apps and Network screens, the password sheet, the app sheet, the forget dialog.
use serde_json::json;
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, ScreenState, Status, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, Cmd, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::overlay::{AppSheet, Overlay, PasswordSheet};
use walpad_ui::components::{Painter, Tab, Tone};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}
fn catalog() -> Vec<AppEntry> {
    vec![
        AppEntry {
            id: "browser".into(),
            name: "Browser".into(),
            icon: Icon::Globe,
        },
        AppEntry {
            id: "terminal".into(),
            name: "Terminal".into(),
            icon: Icon::Terminal,
        },
        AppEntry {
            id: "files".into(),
            name: "Files".into(),
            icon: Icon::Folder,
        },
    ]
}
fn status() -> Status {
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
        },
        apps: vec![
            AppState {
                app: "browser".into(),
                running: true,
            },
            AppState {
                app: "terminal".into(),
                running: false,
            },
            AppState {
                app: "files".into(),
                running: false,
            },
        ],
        hardware: Hardware {
            touch: "ok".into(),
            gsensor: "ok".into(),
            audio: "unverified".into(),
            bluetooth: "unavailable".into(),
            sd_card: "unavailable".into(),
        },
    }
}
fn app(w: i32, h: i32, tab: Tab, online: bool) -> App {
    let mut a = App::new(
        w,
        h,
        catalog(),
        WallTime::new(14, 22, "Monday, 09 November", true),
    );
    if online {
        a.on_core(&CoreEvent::Status(Box::new(status())), 0);
    }
    a.active = tab;
    a
}
fn saved(a: &mut App) {
    a.note_sent(20, &Cmd::WifiListSaved);
    a.on_core(&CoreEvent::Reply { id: 20, ok: true, data: json!([{"id":0,"ssid":"Home"},{"id":1,"ssid":"Work Office"},{"id":2,"ssid":"Café"}]), error: None }, 0);
}
fn scan(a: &mut App) {
    let nets = [
        ("Home", "wpa", -52),
        ("Cafe Open", "open", -60),
        ("Neighbour", "wpa", -68),
        ("Guest WiFi", "open", -74),
        ("Work Office", "wpa", -79),
        ("Old Router", "wpa", -88),
    ];
    let list: Vec<_> = nets.iter().map(|(s, sec, d)| json!({"ssid_hex": s.bytes().map(|b| format!("{b:02x}")).collect::<String>(), "ssid": s, "signal_dbm": d, "security": sec, "freq_mhz": 2412})).collect();
    a.on_core(&CoreEvent::Wifi(json!({"kind":"scan","networks": list})), 0);
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

#[test]
fn apps_portrait_and_landscape() {
    shot(
        "apps_portrait",
        800,
        1280,
        &mut app(800, 1280, Tab::Apps, true),
    );
    shot(
        "apps_landscape",
        1280,
        800,
        &mut app(1280, 800, Tab::Apps, true),
    );
}
#[test]
fn apps_options_sheet() {
    let mut a = app(800, 1280, Tab::Apps, true);
    a.model.overlay = Some(Overlay::App(AppSheet {
        id: "browser".into(),
        name: "Browser".into(),
        running: true,
        pinned: true,
    }));
    shot("apps_sheet_running", 800, 1280, &mut a);
}
#[test]
fn network_connected_with_results_and_saved() {
    let mut a = app(800, 1280, Tab::Network, true);
    scan(&mut a);
    saved(&mut a);
    shot("network_portrait", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Network, true);
    scan(&mut l);
    saved(&mut l);
    shot("network_landscape", 1280, 800, &mut l);
}
#[test]
fn network_scanning_connecting_and_empty_states() {
    let mut a = app(800, 1280, Tab::Network, true);
    a.model.net.scanning = true;
    shot("network_scanning", 800, 1280, &mut a);
    let mut b = app(800, 1280, Tab::Network, true);
    scan(&mut b);
    b.model.net.connecting = Some("4e65696768626f7572".into());
    b.model.spinner_deg = 90;
    shot("network_connecting", 800, 1280, &mut b);
    let mut c = app(800, 1280, Tab::Network, true);
    c.model.net.scanned = true;
    shot("network_no_networks", 800, 1280, &mut c);
    let mut d = app(800, 1280, Tab::Network, true);
    d.model.net.error = Some("Wi-Fi control is unavailable".into());
    shot("network_scan_error", 800, 1280, &mut d);
}
#[test]
fn network_failed_radio_and_core_offline() {
    let mut s = status();
    s.wifi.radio = WifiRadio::Failed;
    s.wifi.ssid = None;
    s.wifi.ip = None;
    let mut a = app(800, 1280, Tab::Network, false);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
    shot("network_radio_failed", 800, 1280, &mut a);
    shot(
        "network_core_offline",
        800,
        1280,
        &mut app(800, 1280, Tab::Network, false),
    );
}
#[test]
fn password_sheet_states() {
    let mut a = app(800, 1280, Tab::Network, true);
    scan(&mut a);
    a.model.overlay = Some(Overlay::Password(PasswordSheet::new(
        "4e65696768626f7572".into(),
        "Neighbour".into(),
    )));
    shot("password_sheet_empty", 800, 1280, &mut a);
    let mut s = PasswordSheet::new("4e65696768626f7572".into(), "Neighbour".into());
    s.secret = "correct horse".into();
    s.error = Some("Wrong password".into());
    a.model.overlay = Some(Overlay::Password(s.clone()));
    shot("password_sheet_wrong_password", 800, 1280, &mut a);
    s.error = None;
    s.shown = true;
    s.connecting = true;
    s.pressed_key = Some(3);
    a.model.overlay = Some(Overlay::Password(s));
    a.model.spinner_deg = 60;
    shot("password_sheet_shown_connecting", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Network, true);
    scan(&mut l);
    let mut s = PasswordSheet::new("4e65696768626f7572".into(), "Neighbour".into());
    s.secret = "abc".into();
    l.model.overlay = Some(Overlay::Password(s));
    shot("password_sheet_landscape", 1280, 800, &mut l);
}
#[test]
fn forget_dialog_and_toast() {
    let mut a = app(800, 1280, Tab::Network, true);
    saved(&mut a);
    a.model.overlay = Some(Overlay::Forget {
        id: 1,
        ssid: "Work Office".into(),
    });
    shot("network_forget_dialog", 800, 1280, &mut a);
    let mut b = app(800, 1280, Tab::Network, true);
    saved(&mut b);
    b.on_core(&CoreEvent::Wifi(json!({"kind":"connect","ok":false,"error":"timeout","ssid_hex":"00","ssid":"Cafe Open"})), 0);
    assert_eq!(b.model.toast.as_ref().map(|t| t.tone), Some(Tone::Danger));
    shot("network_toast_failed", 800, 1280, &mut b);
}
