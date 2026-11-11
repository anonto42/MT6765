//! Volume UI snapshots (owner request 2026-11-10): the Quick Settings block, the key pill and the
//! Settings > Sound page.
use serde_json::json;
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, AudioStatus, Battery, BatterySource, Hardware, ScreenState, Status, WifiRadio,
    WifiStatus,
};
use walpad_shell::app::{App, AppEntry, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::overlay::Overlay;
use walpad_shell::settings_screen::Page;
use walpad_ui::components::{Painter, Tab};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}
fn status(volume: u8, muted: bool, available: bool) -> Status {
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
        gps: Default::default(),
        bluetooth: Default::default(),
        lock: Default::default(),
        vpn: Default::default(),
        audio: AudioStatus {
            volume,
            muted,
            available,
        },
        apps: vec![AppState {
            app: "browser".into(),
            running: true,
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
fn app(w: i32, h: i32, tab: Tab, st: Status) -> App {
    let catalog = vec![AppEntry {
        id: "browser".into(),
        name: "Browser".into(),
        icon: Icon::Globe,
    }];
    let mut a = App::new(
        w,
        h,
        catalog,
        WallTime::new(14, 22, "Monday, 09 November", true),
    );
    a.on_core(&CoreEvent::Status(Box::new(st)), 0);
    a.active = tab;
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

#[test]
fn quick_settings_volume_states() {
    for (st, name) in [
        (status(35, false, true), "quick_settings_volume_35"),
        (status(60, true, true), "quick_settings_volume_muted"),
        (
            status(60, false, false),
            "quick_settings_volume_unavailable",
        ),
    ] {
        let mut a = app(800, 1280, Tab::Home, st);
        a.model.overlay = Some(Overlay::QuickSettings);
        shot(name, 800, 1280, &mut a);
    }
    let mut l = app(1280, 800, Tab::Home, status(80, false, true));
    l.model.overlay = Some(Overlay::QuickSettings);
    shot("quick_settings_volume_landscape", 1280, 800, &mut l);
}

#[test]
fn volume_pill_for_a_key_press() {
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        let mut a = app(w, h, Tab::Home, status(65, false, true));
        a.on_core(
            &CoreEvent::Audio(
                json!({"volume": 65, "muted": false, "available": true, "source": "key"}),
            ),
            1000,
        );
        shot(&format!("volume_pill_{tag}"), w, h, &mut a);
    }
    let mut m = app(800, 1280, Tab::Home, status(0, true, true));
    m.on_core(
        &CoreEvent::Audio(json!({"volume": 0, "muted": true, "available": true, "source": "key"})),
        1000,
    );
    shot("volume_pill_muted", 800, 1280, &mut m);
}

#[test]
fn settings_sound_page() {
    let mut a = app(800, 1280, Tab::Settings, status(35, false, true));
    a.model.settings_page = Some(Page::Sound);
    shot("settings_sound", 800, 1280, &mut a);
    let mut u = app(800, 1280, Tab::Settings, status(60, false, false));
    u.model.settings_page = Some(Page::Sound);
    shot("settings_sound_unavailable", 800, 1280, &mut u);
    let mut l = app(1280, 800, Tab::Settings, status(100, true, true));
    l.model.settings_page = Some(Page::Sound);
    shot("settings_sound_landscape", 1280, 800, &mut l);
}
