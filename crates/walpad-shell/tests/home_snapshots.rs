//! M5.4 snapshots: Home in both orientations and its important states.
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, ScreenState, Status, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_ui::components::Painter;
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
fn shot(name: &str, w: i32, h: i32, status: Option<Status>, clock_24h: bool) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let mut app = App::new(
        w,
        h,
        catalog(),
        WallTime::new(14, 22, "Monday, 09 November", clock_24h),
    );
    if let Some(s) = status {
        app.on_core(&CoreEvent::Status(Box::new(s)));
    }
    let mut c = Canvas::new(w as u32, h as u32, BG).unwrap();
    app.render(&mut Painter {
        cv: &mut c,
        fonts: &mut f,
    });
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn home_portrait_connected() {
    shot("home_portrait", 800, 1280, Some(status()), true);
}
#[test]
fn home_landscape_connected() {
    shot("home_landscape", 1280, 800, Some(status()), true);
}
#[test]
fn home_portrait_core_offline() {
    shot("home_portrait_offline", 800, 1280, None, true);
}
#[test]
fn home_portrait_low_battery_unknown_wifi_12h() {
    let mut s = status();
    s.battery.low = true;
    s.battery.percent = None;
    s.battery.source = BatterySource::Unknown;
    s.wifi.ssid = None;
    s.wifi.ip = None;
    s.rotation_locked = true;
    s.apps.clear();
    shot("home_portrait_low_unknown", 800, 1280, Some(s), false);
}
#[test]
fn home_landscape_offline() {
    shot("home_landscape_offline", 1280, 800, None, true);
}
#[test]
fn home_portrait_charging_gauge_and_failed_radio() {
    let mut s = status();
    s.battery.source = BatterySource::Gauge;
    s.battery.state = Some("Charging".into());
    s.wifi.radio = WifiRadio::Failed;
    s.wifi.ssid = None;
    shot(
        "home_portrait_charging_wifi_failed",
        800,
        1280,
        Some(s),
        true,
    );
}
