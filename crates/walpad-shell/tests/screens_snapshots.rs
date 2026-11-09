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

// ---------------------------------------------------------------------------------- M5.6

use walpad_shell::settings_screen::{Page, SysInfo};

fn sys(telnet_open: bool) -> SysInfo {
    SysInfo {
        alpine: Some("3.24.0".into()),
        kernel: Some("4.19.127".into()),
        storage: Some((3_800_000_000, 1_200_000_000)),
        memory_kb: Some((3_800_000, 1_900_000)),
        uptime_s: Some(8040),
        ssh_port_open: true,
        ssh_keys: 1,
        listening: vec![
            ("0.0.0.0".into(), 22),
            (
                if telnet_open {
                    "0.0.0.0"
                } else {
                    "192.168.42.1"
                }
                .into(),
                23,
            ),
        ],
        dns: vec!["1.1.1.1".into(), "8.8.8.8".into()],
        gateway: Some("192.168.0.1".into()),
        logs: (0..40)
            .map(|i| format!("walpad-core: line {i} orientation -> 90 accel x=4000 psk=hidden{i}"))
            .map(|l| walpad_hw::sysinfo::redact_log_line(&l))
            .collect(),
        gestures: vec![
            ("Volume up · tap".into(), "Key Up".into()),
            ("Volume up · hold".into(), "Toggle keyboard".into()),
            ("Volume down · tap".into(), "Key Down".into()),
            ("Volume down · hold".into(), "Home".into()),
            ("Both buttons".into(), "Rotate toggle".into()),
        ],
    }
}

#[test]
fn device_tab_portrait_landscape_offline() {
    let mut a = app(800, 1280, Tab::Device, true);
    shot("device_portrait", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Device, true);
    shot("device_landscape", 1280, 800, &mut l);
    // locked to landscape, custom timeouts, lid option off, battery gauge + low
    let mut s = status();
    s.rotation_locked = true;
    s.orientation = 90;
    s.battery.source = BatterySource::Gauge;
    s.battery.low = true;
    s.hardware.audio = "ok".into();
    let mut b = app(800, 1280, Tab::Device, false);
    b.on_core(&CoreEvent::Status(Box::new(s)), 0);
    b.set_config(
        walpad_shell::config::DevdSettings {
            dim_after_s: 30,
            off_after_s: 600,
            lid_off: false,
            ..Default::default()
        },
        true,
    );
    shot("device_locked_custom", 800, 1280, &mut b);
    shot(
        "device_offline",
        800,
        1280,
        &mut app(800, 1280, Tab::Device, false),
    );
}

#[test]
fn settings_list_and_pages() {
    let mut a = app(800, 1280, Tab::Settings, true);
    a.set_sysinfo(sys(false));
    shot("settings_list", 800, 1280, &mut a);
    for (pg, name) in [
        (Page::System, "settings_system"),
        (Page::Network, "settings_network"),
        (Page::Security, "settings_security"),
        (Page::Maintenance, "settings_maintenance"),
        (Page::Input, "settings_input"),
        (Page::Power, "settings_power"),
        (Page::Clock, "settings_clock"),
        (Page::About, "settings_about"),
        (Page::Logs, "settings_logs"),
    ] {
        a.model.settings_page = Some(pg);
        shot(name, 800, 1280, &mut a);
    }
    // telnet open on the network: the owner must see the warning
    a.set_sysinfo(sys(true));
    a.model.settings_page = Some(Page::Security);
    shot("settings_security_telnet_open", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Settings, true);
    l.set_sysinfo(sys(false));
    shot("settings_list_landscape", 1280, 800, &mut l);
}

#[test]
fn power_confirm_dialogs() {
    let mut a = app(800, 1280, Tab::Settings, true);
    a.model.settings_page = Some(Page::Maintenance);
    a.model.overlay = Some(Overlay::PowerConfirm {
        poweroff: true,
        armed_until: None,
    });
    shot("power_confirm_shutdown", 800, 1280, &mut a);
    a.model.overlay = Some(Overlay::PowerConfirm {
        poweroff: true,
        armed_until: Some(5000),
    });
    shot("power_confirm_shutdown_armed", 800, 1280, &mut a);
    a.model.overlay = Some(Overlay::PowerConfirm {
        poweroff: false,
        armed_until: None,
    });
    shot("power_confirm_restart", 800, 1280, &mut a);
}

#[test]
fn power_menu_snapshots() {
    use walpad_proto::ShellInput;
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        // opened by a long power press: Cancel is focused
        let mut a = app(w, h, Tab::Settings, true);
        a.on_core(&CoreEvent::Input(ShellInput::PowerMenuOpen), 1000);
        shot(&format!("power_menu_cancel_{tag}"), w, h, &mut a);
        // volume up -> Shut down focused, short power press -> armed ("Tap again to shut down")
        a.on_core(&CoreEvent::Input(ShellInput::VolumeUp), 1100);
        a.on_core(&CoreEvent::Input(ShellInput::PowerShort), 1200);
        shot(&format!("power_menu_shutdown_armed_{tag}"), w, h, &mut a);
    }
}
