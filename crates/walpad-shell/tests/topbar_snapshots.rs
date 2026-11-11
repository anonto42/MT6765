//! Header stats snapshots (owner request 2026-11-11, change `top-bar-stats`): the figures between
//! the clock and the Wi-Fi/battery pill in both orientations, the tight and the unavailable cases,
//! and Settings > Top bar items.
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, GpsState, Hardware, ScreenState, StatsItems, StatsStatus,
    Status, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::settings_screen::Page;
use walpad_ui::components::{Painter, Tab};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

const GIB: u64 = 1024 * 1024 * 1024;

fn stats(items: StatsItems) -> StatsStatus {
    StatsStatus {
        items,
        cpu_pct: Some(23),
        ram_used_kb: Some(1_310_720),
        ram_total_kb: Some(2_097_152),
        rom_used_b: Some(12 * GIB),
        rom_total_b: Some(54 * GIB),
        net_down_bps: Some(1_258_291),
        net_up_bps: Some(30 * 1024),
    }
}
const ALL: StatsItems = StatsItems {
    cpu: true,
    ram: true,
    rom: true,
    net: true,
};

fn status(st: StatsStatus) -> Status {
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
        stats: st,
        bluetooth: Default::default(),
        lock: Default::default(),
        vpn: Default::default(),
        audio: Default::default(),
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

fn app(w: i32, h: i32, tab: Tab, clock_24h: bool, s: Status) -> App {
    let catalog = vec![AppEntry {
        id: "browser".into(),
        name: "Browser".into(),
        icon: Icon::Globe,
    }];
    let mut a = App::new(
        w,
        h,
        catalog,
        WallTime::new(14, 22, "Monday, 09 November", clock_24h),
    );
    a.on_core(&CoreEvent::Status(Box::new(s)), 0);
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
fn header_default_cpu_and_ram() {
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        let mut a = app(w, h, Tab::Home, true, status(stats(StatsItems::default())));
        shot(&format!("topbar_default_{tag}"), w, h, &mut a);
    }
}

#[test]
fn header_all_four_items() {
    for (w, h, tag) in [(800, 1280, "portrait"), (1280, 800, "landscape")] {
        let mut a = app(w, h, Tab::Home, true, status(stats(ALL)));
        shot(&format!("topbar_all_{tag}"), w, h, &mut a);
    }
}

/// The tightest header: 12-hour clock with AM/PM, the GPS mark and a high CPU load; the date goes.
#[test]
fn header_tight_portrait_12h_gps_mark_high_load() {
    let mut s = status(StatsStatus {
        cpu_pct: Some(96),
        ..stats(ALL)
    });
    s.gps.enabled = true;
    s.gps.state = GpsState::Fix;
    let mut a = app(800, 1280, Tab::Home, false, s);
    shot("topbar_tight_portrait_12h_gps", 800, 1280, &mut a);
}

/// Storage and network only; a figure that could not be read says "--".
#[test]
fn header_unavailable_figures() {
    let s = status(StatsStatus {
        items: StatsItems {
            cpu: true,
            ram: false,
            rom: true,
            net: true,
        },
        cpu_pct: None,
        rom_used_b: None,
        rom_total_b: None,
        net_up_bps: None,
        ..stats(ALL)
    });
    let mut a = app(800, 1280, Tab::Home, true, s);
    shot("topbar_unavailable_portrait", 800, 1280, &mut a);
}

#[test]
fn header_without_items_or_before_the_first_sample_is_unchanged() {
    let off = StatsItems {
        cpu: false,
        ram: false,
        rom: false,
        net: false,
    };
    let mut a = app(800, 1280, Tab::Home, true, status(stats(off)));
    let mut b = app(800, 1280, Tab::Home, true, status(StatsStatus::default()));
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let draw = |a: &mut App, f: &mut Fonts| {
        let mut c = Canvas::new(800, 1280, BG).unwrap();
        a.render(&mut Painter {
            cv: &mut c,
            fonts: f,
        });
        c.rgba().to_vec()
    };
    assert_eq!(draw(&mut a, &mut f), draw(&mut b, &mut f));
}

#[test]
fn settings_top_bar_items_page() {
    let mut a = app(
        800,
        1280,
        Tab::Settings,
        true,
        status(stats(StatsItems::default())),
    );
    a.model.settings_page = Some(Page::TopBar);
    shot("settings_top_bar", 800, 1280, &mut a);
    let mut l = app(1280, 800, Tab::Settings, true, status(stats(ALL)));
    l.model.settings_page = Some(Page::TopBar);
    shot("settings_top_bar_landscape", 1280, 800, &mut l);
}
