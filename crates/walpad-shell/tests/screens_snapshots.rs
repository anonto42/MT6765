//! M5.5 snapshots: Apps and Network screens, the password sheet, the app sheet, the forget dialog.
use serde_json::json;
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, HotspotInternet, HotspotState, HotspotStatus,
    ScreenState, Status, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, Cmd, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::overlay::{AppSheet, Overlay, PasswordSheet, SheetPurpose};
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
            time_left_min: Some(322),
            time_to_full_min: None,
            time_calculating: false,
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
        gps: Default::default(),
        stats: Default::default(),
        bluetooth: Default::default(),
        lock: Default::default(),
        vpn: Default::default(),
        audio: walpad_proto::AudioStatus {
            volume: 60,
            muted: false,
            available: true,
        },
        jack: Default::default(),
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
        notifications: walpad_proto::NotificationsStatus::default(),
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
fn apps_panel_portrait_and_landscape() {
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        let mut a = app(w, h, Tab::Home, true);
        a.model.overlay = Some(Overlay::AppsPanel);
        shot(&format!("apps_panel_{tag}"), w, h, &mut a);
    }
    let mut none = app(800, 1280, Tab::Home, false);
    none.model.overlay = Some(Overlay::AppsPanel);
    shot("apps_panel_offline", 800, 1280, &mut none);
}
#[test]
fn quick_settings_portrait_landscape_and_states() {
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        let mut a = app(w, h, Tab::Home, true);
        a.model.overlay = Some(Overlay::QuickSettings);
        shot(&format!("quick_settings_{tag}"), w, h, &mut a);
    }
    // gauge reading + locked rotation over the Network tab
    let mut s = status();
    s.battery.source = BatterySource::Gauge;
    s.battery.state = Some("Charging".into());
    s.rotation_locked = true;
    let mut b = app(800, 1280, Tab::Network, false);
    b.on_core(&CoreEvent::Status(Box::new(s)), 0);
    b.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_gauge_locked", 800, 1280, &mut b);
    // core offline: everything unknown, the toggles disabled
    let mut c = app(800, 1280, Tab::Home, false);
    c.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_offline", 800, 1280, &mut c);
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

// ------------------------------------------------- Wi-Fi switch and hotspot (owner request 2026-11-10)

fn hotspot_on(s: &mut Status, exclusive: bool) {
    s.hotspot = HotspotStatus {
        state: HotspotState::On,
        reason: None,
        ssid: Some("WalPad Tab".into()),
        has_secret: true,
        clients: Some(2),
        internet: Some(if exclusive {
            HotspotInternet::NoUplink
        } else {
            HotspotInternet::Shared
        }),
        exclusive,
    };
    if exclusive {
        s.wifi.radio = WifiRadio::Down;
        s.wifi.state = Some("hotspot".into());
        s.wifi.ssid = None;
        s.wifi.ip = None;
        s.wifi.signal_dbm = None;
    }
}

#[test]
fn network_wifi_switched_off() {
    let mut s = status();
    s.wifi = WifiStatus {
        radio: WifiRadio::Down,
        state: Some("disabled".into()),
        ssid: None,
        ip: None,
        signal_dbm: None,
        enabled: false,
    };
    let mut a = app(800, 1280, Tab::Network, false);
    a.on_core(&CoreEvent::Status(Box::new(s.clone())), 0);
    saved(&mut a);
    shot("network_wifi_off", 800, 1280, &mut a);
    let mut q = app(800, 1280, Tab::Home, false);
    q.on_core(&CoreEvent::Status(Box::new(s)), 0);
    q.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_wifi_off", 800, 1280, &mut q);
}

#[test]
fn network_hotspot_states() {
    // running next to the client (shared internet), scrolled to the hotspot section
    let mut s = status();
    hotspot_on(&mut s, false);
    let mut a = app(800, 1280, Tab::Network, false);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
    scan(&mut a);
    saved(&mut a);
    a.scroll = 10_000;
    shot("network_hotspot_on_shared", 800, 1280, &mut a);
    // exclusive: the client is paused, local network only
    let mut s = status();
    hotspot_on(&mut s, true);
    let mut b = app(800, 1280, Tab::Network, false);
    b.on_core(&CoreEvent::Status(Box::new(s.clone())), 0);
    saved(&mut b);
    shot("network_hotspot_on_exclusive", 800, 1280, &mut b);
    let mut l = app(1280, 800, Tab::Network, false);
    l.on_core(&CoreEvent::Status(Box::new(s)), 0);
    saved(&mut l);
    l.scroll = 10_000;
    shot("network_hotspot_landscape", 1280, 800, &mut l);
    // failed, no password yet
    let mut s = status();
    s.hotspot.state = HotspotState::Failed;
    s.hotspot.reason = Some("hostapd could not start the access point".into());
    s.hotspot.has_secret = true;
    let mut c = app(800, 1280, Tab::Network, false);
    c.on_core(&CoreEvent::Status(Box::new(s)), 0);
    saved(&mut c);
    shot("network_hotspot_failed", 800, 1280, &mut c);
}

#[test]
fn quick_settings_with_hotspot() {
    let mut s = status();
    hotspot_on(&mut s, true);
    let mut a = app(800, 1280, Tab::Home, false);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
    a.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_hotspot_on", 800, 1280, &mut a);
    let mut s = status();
    s.hotspot.state = HotspotState::Starting;
    s.hotspot.has_secret = true;
    let mut l = app(1280, 800, Tab::Home, false);
    l.on_core(&CoreEvent::Status(Box::new(s)), 0);
    l.model.overlay = Some(Overlay::QuickSettings);
    shot(
        "quick_settings_hotspot_starting_landscape",
        1280,
        800,
        &mut l,
    );
}

#[test]
fn hotspot_sheets() {
    let mut a = app(800, 1280, Tab::Network, true);
    let mut s = PasswordSheet::hotspot(SheetPurpose::HotspotPassword, "");
    s.error = Some("Set a password first".into());
    a.model.overlay = Some(Overlay::Password(s));
    shot("hotspot_password_sheet", 800, 1280, &mut a);
    let s = PasswordSheet::hotspot(SheetPurpose::HotspotName, "WalPad Tab");
    a.model.overlay = Some(Overlay::Password(s));
    shot("hotspot_name_sheet", 800, 1280, &mut a);
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

/// The sidebar's round Apps toggle, pressed (closed and open panel, both orientations): only its
/// circle changes; the corners of its bounding box keep the background (no square flash), and
/// releasing it restores the resting frame exactly.
#[test]
fn pressed_round_toggle_paints_nothing_outside_its_circle() {
    use walpad_shell::app::Target;
    use walpad_shell::chrome::{pinned_items, sidebar_layout, NAV_APPS};
    use walpad_ui::layout::Frame;
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    for (w, h) in [(800, 1280), (1280, 800)] {
        for open in [false, true] {
            let mut a = app(w, h, Tab::Home, true);
            if open {
                a.model.overlay = Some(Overlay::AppsPanel);
            }
            let mut draw = |a: &mut App| {
                let mut c = Canvas::new(w as u32, h as u32, BG).unwrap();
                a.render(&mut Painter {
                    cv: &mut c,
                    fonts: &mut f,
                });
                c
            };
            let rest = draw(&mut a);
            a.model.pressed = Some(Target::Nav(NAV_APPS));
            let pressed = draw(&mut a);
            a.model.pressed = None;
            let again = draw(&mut a);
            let toggle = sidebar_layout(&Frame::new(w, h), &pinned_items(&a.model), open)
                .items
                .last()
                .unwrap()
                .rect;
            let (cx, cy) = (
                toggle.x as f32 + toggle.w as f32 / 2.0,
                toggle.y as f32 + toggle.h as f32 / 2.0,
            );
            let rad = toggle.w as f32 / 2.0;
            let mut inside_changed = 0;
            // the bounding box and a 4 px margin around it
            for y in toggle.y - 4..toggle.bottom() + 4 {
                for x in toggle.x - 4..toggle.right() + 4 {
                    let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                    let (p, r) = (pressed.pixel(x, y), rest.pixel(x, y));
                    if d > rad + 1.0 {
                        assert_eq!(p, r, "{w}x{h} open={open}: ({x},{y}) outside the circle");
                    } else if d < rad - 3.0 && p != r {
                        inside_changed += 1;
                    }
                    assert_eq!(again.pixel(x, y), r, "release restores ({x},{y})");
                }
            }
            assert!(
                inside_changed > 500,
                "the pressed tint fills the circle: {inside_changed}"
            );
        }
    }
}

#[test]
fn sidebar_toggle_pressed_golden() {
    use walpad_shell::app::Target;
    use walpad_shell::chrome::NAV_APPS;
    let mut a = app(800, 1280, Tab::Home, true);
    a.model.pressed = Some(Target::Nav(NAV_APPS));
    shot("sidebar_toggle_pressed", 800, 1280, &mut a);
}

// ------------------------------------------------- mobile network (owner request 2026-11-10)
fn with_mobile(a: &mut App, f: impl FnOnce(&mut walpad_proto::MobileStatus)) {
    let mut s = status();
    f(&mut s.mobile);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
}
fn gp(m: &mut walpad_proto::MobileStatus) {
    use walpad_proto::{MobileReg, ModemState, SimState};
    m.modem = ModemState::Ready;
    m.radio_on = Some(true);
    m.sim = SimState::Ready;
    m.registration = MobileReg::Home;
    m.operator = Some("Grameenphone".into());
    m.tech = Some("4G".into());
    m.signal_bars = Some(3);
    m.signal_dbm = Some(-71);
    m.firmware = Some("MOLY.LR12A.R3.MP.V98.P10".into());
}

#[test]
fn mobile_network_screen_states() {
    use walpad_proto::{MobileData, ModemState, SimState};
    let mut a = app(800, 1280, Tab::Mobile, true);
    with_mobile(&mut a, |m| {
        gp(m);
        m.data = MobileData::On;
        m.ip = Some("10.120.4.17".into());
        m.apn = Some("gpinternet".into());
        m.apn_in_use = Some("gpinternet".into());
    });
    shot("mobile_registered_data_on", 800, 1280, &mut a);

    let mut a = app(800, 1280, Tab::Mobile, true);
    with_mobile(&mut a, |m| {
        gp(m);
        m.data = MobileData::Failed;
        m.data_reason = Some("data link not supported yet".into());
        m.apn_in_use = Some("gpinternet".into());
    });
    a.model.imei_shown = true;
    a.model.mobile_info = Some(walpad_shell::mobile::MobileInfo {
        imei: Some("351234567890123".into()),
        iccid: None,
    });
    shot("mobile_data_failed_imei_shown", 800, 1280, &mut a);

    let mut a = app(1280, 800, Tab::Mobile, true);
    with_mobile(&mut a, |m| {
        m.modem = ModemState::Ready;
        m.radio_on = Some(true);
        m.sim = SimState::PinRequired;
        m.firmware = Some("MOLY.LR12A.R3.MP.V98.P10".into());
    });
    shot("mobile_pin_required_landscape", 1280, 800, &mut a);
    a.model.overlay = Some(Overlay::Password(PasswordSheet::mobile(
        SheetPurpose::SimPin,
        "12",
    )));
    shot("mobile_pin_sheet_landscape", 1280, 800, &mut a);

    let mut a = app(800, 1280, Tab::Mobile, true);
    with_mobile(&mut a, |m| {
        m.modem = ModemState::Ready;
        m.radio_on = Some(false);
        m.sim = SimState::Absent;
    });
    shot("mobile_no_sim_radio_off", 800, 1280, &mut a);
    let mut a = app(800, 1280, Tab::Mobile, true);
    with_mobile(&mut a, |m| {
        m.modem = ModemState::Failed;
        m.reason = Some("the modem did not start: after 60s: md_en=0".into());
    });
    shot("mobile_modem_failed", 800, 1280, &mut a);
}

#[test]
fn quick_settings_mobile_row_and_pill_bars() {
    let mut a = app(800, 1280, Tab::Home, true);
    with_mobile(&mut a, gp);
    a.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_mobile_registered", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Home, true);
    with_mobile(&mut l, gp);
    shot("home_landscape_mobile_bars", 1280, 800, &mut l);
}

// ------------------------------------------------- Bluetooth (owner request 2026-11-10)
fn with_bt(a: &mut App, f: impl FnOnce(&mut walpad_proto::BluetoothStatus)) {
    let mut s = status();
    f(&mut s.bluetooth);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
}
fn bt_dev(
    a: &str,
    name: &str,
    paired: bool,
    connected: bool,
    rssi: Option<i32>,
    icon: &str,
) -> walpad_proto::BluetoothDevice {
    walpad_proto::BluetoothDevice {
        address: a.into(),
        name: name.into(),
        paired,
        connected,
        rssi,
        icon: Some(icon.into()),
        class: None,
    }
}
fn bt_on(b: &mut walpad_proto::BluetoothStatus) {
    use walpad_proto::BluetoothController;
    b.controller = BluetoothController::Present;
    b.powered = true;
    b.devices = vec![
        bt_dev(
            "AA:BB:CC:00:11:22",
            "Logitech K380",
            true,
            true,
            None,
            "input-keyboard",
        ),
        bt_dev(
            "AA:BB:CC:00:11:33",
            "WH-1000XM4",
            true,
            false,
            None,
            "audio-headset",
        ),
        bt_dev(
            "66:55:44:33:22:11",
            "Pixel 7",
            false,
            false,
            Some(-52),
            "phone",
        ),
        bt_dev(
            "66:55:44:33:22:12",
            "Living Room TV",
            false,
            false,
            Some(-71),
            "video-display",
        ),
        bt_dev(
            "5C:AA:BB:CC:DD:EE",
            "5C:AA:BB:CC:DD:EE",
            false,
            false,
            Some(-88),
            "input-mouse",
        ),
    ];
}

#[test]
fn bluetooth_screen_states() {
    use walpad_proto::BluetoothController;
    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, bt_on);
    shot("bluetooth_devices_portrait", 800, 1280, &mut a);

    let mut a = app(1280, 800, Tab::Bluetooth, true);
    with_bt(&mut a, |b| {
        bt_on(b);
        b.discovering = true;
    });
    a.model.spinner_deg = 60;
    shot("bluetooth_scanning_landscape", 1280, 800, &mut a);

    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, |b| {
        b.controller = BluetoothController::Present;
        b.powered = false;
    });
    shot("bluetooth_off", 800, 1280, &mut a);

    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, |b| b.controller = BluetoothController::Starting);
    shot("bluetooth_starting", 800, 1280, &mut a);

    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, |b| {
        b.reason = Some("bridge did not create an HCI device".into())
    });
    shot("bluetooth_failed", 800, 1280, &mut a);
}

#[test]
fn bluetooth_busy_row_and_dialogs() {
    use walpad_shell::bluetooth::PairState;
    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, bt_on);
    a.model.bt_busy = Some("AA:BB:CC:00:11:33".into());
    a.model.spinner_deg = 90;
    shot("bluetooth_connecting_row", 800, 1280, &mut a);
    for (tag, state) in [
        ("waiting", PairState::Waiting),
        ("confirm", PairState::Confirm("123456".into())),
        ("passkey", PairState::Display("482913".into())),
        ("failed", PairState::Failed("Authentication failed".into())),
    ] {
        let mut a = app(800, 1280, Tab::Bluetooth, true);
        with_bt(&mut a, bt_on);
        a.model.spinner_deg = 90;
        a.model.overlay = Some(Overlay::BtPair {
            address: "66:55:44:33:22:11".into(),
            name: "Pixel 7".into(),
            state,
        });
        shot(&format!("bluetooth_pair_{tag}"), 800, 1280, &mut a);
    }
    let mut a = app(800, 1280, Tab::Bluetooth, true);
    with_bt(&mut a, bt_on);
    a.model.overlay = Some(Overlay::BtForget {
        address: "AA:BB:CC:00:11:33".into(),
        name: "WH-1000XM4".into(),
    });
    shot("bluetooth_forget_dialog", 800, 1280, &mut a);
}

#[test]
fn quick_settings_bluetooth_row_and_header_mark() {
    let mut a = app(800, 1280, Tab::Home, true);
    with_bt(&mut a, bt_on);
    a.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_bluetooth_on", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Home, true);
    with_bt(&mut l, bt_on);
    shot("home_landscape_bluetooth_mark", 1280, 800, &mut l);
}

// ------------------------------------------------- GPS (owner request 2026-11-11)
fn with_gps(a: &mut App, f: impl FnOnce(&mut walpad_proto::GpsStatus)) {
    let mut s = status();
    f(&mut s.gps);
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
}

#[test]
fn gps_screen_states() {
    use walpad_proto::GpsState;
    let mut a = app(800, 1280, Tab::Gps, true);
    shot("gps_off", 800, 1280, &mut a);
    let mut a = app(800, 1280, Tab::Gps, true);
    with_gps(&mut a, |g| {
        g.enabled = true;
        g.state = GpsState::Searching;
        g.satellites_view = Some(6);
        g.satellites_used = Some(0);
        g.signals = vec![31, 24, 18, 12, 9, 0];
    });
    shot("gps_searching", 800, 1280, &mut a);
    let mut a = app(800, 1280, Tab::Gps, true);
    with_gps(&mut a, |g| {
        g.enabled = true;
        g.state = GpsState::Fix;
        g.lat = Some(23.75123);
        g.lon = Some(90.40988);
        g.alt_m = Some(12);
        g.accuracy_m = Some(7);
        g.satellites_used = Some(5);
        g.satellites_view = Some(8);
        g.age_s = Some(1);
        g.signals = vec![41, 38, 33, 29, 22, 20, 14, 0];
    });
    shot("gps_fix", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Home, true);
    with_gps(&mut l, |g| {
        g.enabled = true;
        g.state = GpsState::Fix;
    });
    shot("home_landscape_gps_mark", 1280, 800, &mut l);
    let mut q = app(800, 1280, Tab::Home, true);
    with_gps(&mut q, |g| {
        g.enabled = true;
        g.state = GpsState::Searching;
        g.satellites_view = Some(4);
    });
    q.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_gps_searching", 800, 1280, &mut q);
}

// ---- app store (change app-catalogue-manager)
fn store_app(w: i32, h: i32, view: walpad_shell::store::StoreView) -> App {
    let mut a = app(w, h, Tab::Settings, true);
    a.model.settings_page = Some(Page::Apps);
    const GIB: u64 = 1024 * 1024;
    a.model.store.apply_catalogue(&json!({
        "entries": [
            {"id":"browser","name":"Browser","category":"Built in","size_kb":224000,"installed":true,"system":true,"roles":["browser"]},
            {"id":"files","name":"Files","category":"Built in","size_kb":2700,"installed":true,"system":true,"roles":["files"]},
            {"id":"mousepad","name":"Mousepad","category":"Documents","summary":"Simple text editor","size_kb":700,"installed":true,"system":false,"roles":[]},
            {"id":"evince","name":"Evince","category":"Documents","summary":"PDF viewer","size_kb":15000,"installed":true,"system":false,"roles":["pdf"]},
            {"id":"zathura","name":"Zathura","category":"Documents","summary":"Light PDF viewer","size_kb":1500,"installed":false,"system":false,"roles":["pdf"]},
            {"id":"mpv","name":"mpv","category":"Media","summary":"Video player","size_kb":37600,"installed":false,"system":false,"roles":["video"]},
            {"id":"vlc","name":"VLC","category":"Media","summary":"Video and audio player","size_kb":28000,"installed":false,"system":false,"roles":["video"]},
            {"id":"libreoffice-writer","name":"LibreOffice Writer","category":"Office","summary":"Word processor (large)","size_kb":322000,"installed":false,"system":false,"roles":[]}
        ],
        "free_kb": 5 * GIB, "reserve_kb": 2 * GIB, "busy": null,
        "defaults": {"browser":"browser","files":"files","pdf":"evince"}
    }));
    a.model.store.view = view;
    a
}

#[test]
fn app_store_screens() {
    use walpad_shell::store::{Busy, StoreView};
    let mut a = store_app(800, 1280, StoreView::Installed);
    shot("store_installed", 800, 1280, &mut a);
    let mut a = store_app(800, 1280, StoreView::Browse);
    shot("store_browse", 800, 1280, &mut a);
    a.model.store.busy = Some(Busy {
        op: "install".into(),
        id: "mpv".into(),
        percent: 45,
        text: "(3/8) Installing ffmpeg-libs (6.1-r0)".into(),
    });
    shot("store_browse_installing", 800, 1280, &mut a);
    a.model.store.busy = None;
    a.model.store.error = Some("LibreOffice Writer: Not enough free space: 315 MB needed, 5.0 GB free, 2.0 GB must stay free. Free some space and try again.".into());
    shot("store_browse_error", 800, 1280, &mut a);
    let mut a = store_app(800, 1280, StoreView::Role(6));
    shot("store_role_pdf", 800, 1280, &mut a);
    let mut a = store_app(1280, 800, StoreView::Browse);
    shot("store_browse_landscape", 1280, 800, &mut a);
    let mut a = store_app(800, 1280, StoreView::Installed);
    a.model.store.armed = Some(2);
    shot("store_installed_armed", 800, 1280, &mut a);
    let mut a = app(800, 1280, Tab::Settings, true);
    a.model.settings_page = Some(Page::Apps);
    shot("store_loading", 800, 1280, &mut a);
}
