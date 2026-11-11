//! Media screen snapshots (owner request 2026-11-11, change media-apps-capture).
use std::path::{Path, PathBuf};
use walpad_proto::{
    AppState, Battery, BatterySource, Hardware, ScreenState, Status, WifiRadio, WifiStatus,
};
use walpad_shell::app::{App, AppEntry, WallTime};
use walpad_shell::core_client::CoreEvent;
use walpad_shell::media::{Info, Item, Kind, Section};
use walpad_ui::components::{Painter, Tab};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Color, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn status(apps: &[&str]) -> Status {
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
        bluetooth: Default::default(),
        lock: Default::default(),
        audio: Default::default(),
        vpn: Default::default(),
        phone: Default::default(),
        gps: Default::default(),
        apps: apps
            .iter()
            .map(|a| AppState {
                app: a.to_string(),
                running: false,
            })
            .collect(),
        hardware: Hardware {
            touch: "ok".into(),
            gsensor: "ok".into(),
            audio: "unverified".into(),
            bluetooth: "unavailable".into(),
            sd_card: "unavailable".into(),
        },
    }
}

fn app(w: i32, h: i32) -> App {
    let mut a = App::new(
        w,
        h,
        vec![AppEntry {
            id: "browser".into(),
            name: "Browser".into(),
            icon: Icon::Globe,
        }],
        WallTime::new(14, 22, "Wednesday, 11 November", true),
    );
    a.on_core(
        &CoreEvent::Status(Box::new(status(&["browser", "mpv", "gpicview"]))),
        0,
    );
    a.active = Tab::Media;
    a
}

/// A thumbnail-like picture: a coloured field with a light shape, as `walpad-thumb` would give.
fn thumb(base: u32, shape: u32, kind: usize) -> Vec<u8> {
    let mut c = Canvas::new(192, 192, Color::hex(base)).unwrap();
    match kind % 3 {
        0 => c.circle(96.0, 96.0, 52.0, Color::hex(shape)),
        1 => c.rrect(walpad_ui::Rect::new(40, 60, 112, 80), 10, Color::hex(shape)),
        _ => c.circle(60.0, 130.0, 70.0, Color::hex(shape)),
    }
    c.encode_png().unwrap()
}

fn item(dir: &str, name: &str, kind: Kind, age: u64, size: u64) -> Item {
    Item {
        path: format!("{dir}/{name}"),
        name: name.into(),
        kind,
        mtime: 1_700_000_000 - age,
        size,
    }
}

fn library(a: &mut App) {
    let m = &mut a.model.media;
    let mut items = Vec::new();
    let palette = [
        (0x234a6e, 0xf2c14e),
        (0x3b5d3a, 0xe8e0c8),
        (0x6a2f3b, 0xf4a259),
        (0x2d3a4f, 0x9ad1d4),
        (0x4b3f72, 0xf7b267),
        (0x1f5f5b, 0xfcefb4),
    ];
    for i in 0..22 {
        items.push(item(
            "/root/Pictures",
            &format!("IMG_{:04}.jpg", 2040 - i),
            Kind::Image,
            i * 3000,
            2_400_000,
        ));
    }
    items.push(item(
        "/mnt/sd/DCIM/100ANDRO",
        "a-very-long-holiday-photo-name-from-the-camera.jpg",
        Kind::Image,
        70_000,
        3_100_000,
    ));
    for (i, n) in ["Family trip.mp4", "Lecture 12.mkv", "clip.webm"]
        .iter()
        .enumerate()
    {
        items.push(item(
            "/root/Videos",
            n,
            Kind::Video,
            500 + i as u64 * 900,
            90_000_000,
        ));
    }
    for (i, n) in [
        "track01.mp3",
        "morning-raga.flac",
        "podcast-episode-41.m4a",
        "voice memo.wav",
    ]
    .iter()
    .enumerate()
    {
        items.push(item(
            "/root/Music",
            n,
            Kind::Audio,
            200 + i as u64 * 100,
            7_000_000,
        ));
    }
    m.set_items(items.clone());
    // thumbnails for most photos and the videos; two photos stay as placeholders (no thumbnail yet)
    for (i, it) in items.iter().enumerate() {
        if it.kind == Kind::Audio || i == 3 || i == 4 {
            continue;
        }
        let (b, s) = palette[i % palette.len()];
        m.on_thumb(&it.path, Some(&thumb(b, s, i)));
    }
    m.on_info(
        "/root/Videos/Family trip.mp4",
        Info {
            duration: Some(754),
            ..Default::default()
        },
    );
    m.on_info(
        "/root/Videos/Lecture 12.mkv",
        Info {
            duration: Some(3729),
            ..Default::default()
        },
    );
    m.on_info(
        "/root/Videos/clip.webm",
        Info {
            duration: Some(8),
            ..Default::default()
        },
    );
    m.on_info(
        "/root/Music/track01.mp3",
        Info {
            duration: Some(187),
            title: Some("Blue in Green".into()),
            artist: Some("Miles Davis".into()),
        },
    );
    m.on_info(
        "/root/Music/morning-raga.flac",
        Info {
            duration: Some(1260),
            ..Default::default()
        },
    );
    m.set_recent(vec![
        "/root/Music/track01.mp3".into(),
        "/root/Videos/Family trip.mp4".into(),
        "/root/Pictures/IMG_2040.jpg".into(),
    ]);
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
fn photos_grid_portrait_and_landscape() {
    let mut a = app(800, 1280);
    library(&mut a);
    shot("media_photos_portrait", 800, 1280, &mut a);
    let mut l = app(1280, 800);
    library(&mut l);
    shot("media_photos_landscape", 1280, 800, &mut l);
}

#[test]
fn videos_music_and_recent() {
    let mut a = app(800, 1280);
    library(&mut a);
    a.model.media.section = Section::Videos;
    shot("media_videos_portrait", 800, 1280, &mut a);
    a.model.media.section = Section::Music;
    shot("media_music_portrait", 800, 1280, &mut a);
    a.model.media.section = Section::Recent;
    shot("media_recent_portrait", 800, 1280, &mut a);
}

#[test]
fn scanning_and_empty_states() {
    let mut a = app(800, 1280);
    a.model.media.scanning = true;
    shot("media_scanning", 800, 1280, &mut a);
    let mut b = app(800, 1280);
    b.model.media.set_items(Vec::new());
    shot("media_photos_empty", 800, 1280, &mut b);
    b.model.media.section = Section::Recent;
    shot("media_recent_empty", 800, 1280, &mut b);
}
