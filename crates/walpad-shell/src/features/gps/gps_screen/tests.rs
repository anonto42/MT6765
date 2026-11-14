use super::*;
use walpad_proto::GpsStatus;

fn st(g: GpsStatus) -> Option<Status> {
    let mut s = crate::app::tests::status(120, &[]);
    s.gps = g;
    Some(s)
}

fn fix() -> GpsStatus {
    GpsStatus {
        enabled: true,
        state: GpsState::Fix,
        lat: Some(23.75123),
        lon: Some(90.40988),
        alt_m: Some(12),
        accuracy_m: Some(7),
        satellites_used: Some(5),
        satellites_view: Some(8),
        age_s: Some(1),
        signals: vec![41, 33, 20, 0],
        ..GpsStatus::default()
    }
}

#[test]
fn states_chip_and_lines() {
    assert_eq!(chip(&None).0, "Unknown");
    assert_eq!(chip(&st(GpsStatus::default())).0, "Off");
    assert_eq!(chip(&st(fix())).0, "Fix");
    let (t, s, c) = state_lines(&st(fix()));
    assert_eq!((t.as_str(), c), ("Position found", SUCCESS));
    assert!(s.contains("Within 7 m") && s.contains("5 of 8"), "{s}");
    let failed = GpsStatus {
        enabled: true,
        state: GpsState::Failed,
        reason: Some("turn Wi-Fi on first".into()),
        ..GpsStatus::default()
    };
    let (t, s, c) = state_lines(&st(failed));
    assert_eq!(
        (t.as_str(), s.as_str(), c),
        ("GPS failed", "turn Wi-Fi on first", DANGER)
    );
    let search = GpsStatus {
        enabled: true,
        state: GpsState::Searching,
        satellites_view: Some(3),
        ..GpsStatus::default()
    };
    assert!(state_lines(&st(search.clone()))
        .1
        .contains("3 satellites in view"));
    assert_eq!(qs_line(&st(search)).0, "Searching, 3 satellites");
    assert_eq!(qs_line(&st(fix())).0, "Fix, within 7 m");
    assert_eq!(qs_line(&st(GpsStatus::default())).0, "Off");
}

#[test]
fn switch_and_busy() {
    assert!(!switchable(&None));
    assert!(switchable(&st(GpsStatus::default())));
    let starting = GpsStatus {
        enabled: true,
        state: GpsState::Starting,
        ..GpsStatus::default()
    };
    assert!(busy(&st(starting.clone())) && !switchable(&st(starting)));
    assert!(switch_on(&st(fix())) && !switch_on(&st(GpsStatus::default())));
}

#[test]
fn pill_mark_only_while_on() {
    assert_eq!(pill_mark(&st(GpsStatus::default())), None);
    assert_eq!(pill_mark(&st(fix())), Some(TEXT));
    let s = GpsStatus {
        enabled: true,
        state: GpsState::Searching,
        ..GpsStatus::default()
    };
    assert_eq!(pill_mark(&st(s)), Some(TEXT_DISABLED));
    let f = GpsStatus {
        state: GpsState::Failed,
        ..GpsStatus::default()
    };
    assert_eq!(pill_mark(&st(f)), None);
    assert_eq!(pill_mark(&None), None);
}

#[test]
fn coordinates_and_link() {
    let g = fix();
    assert_eq!(coords_text(&g).unwrap(), "23.75123, 90.40988");
    assert_eq!(
        map_link(&g).unwrap(),
        "https://www.openstreetmap.org/?mlat=23.75123&mlon=90.40988#map=17/23.75123/90.40988"
    );
    let south_west = GpsStatus {
        lat: Some(-33.8688),
        lon: Some(-70.5),
        ..GpsStatus::default()
    };
    assert_eq!(coords_text(&south_west).unwrap(), "-33.86880, -70.50000");
    assert!(coords_text(&GpsStatus::default()).is_none());
    assert!(map_link(&GpsStatus::default()).is_none());
}

#[test]
fn age_wording() {
    assert_eq!(age_text(0), "just now");
    assert_eq!(age_text(12), "12 s ago");
    assert_eq!(age_text(150), "2 min ago");
    assert_eq!(age_text(4000), "over an hour ago");
}

#[test]
fn bar_heights() {
    assert_eq!(bar_height(0, 100), 4);
    assert_eq!(bar_height(25, 100), 50);
    assert_eq!(bar_height(99, 100), 100);
}

#[test]
fn layout_has_the_position_block_only_with_a_position() {
    let f = Frame::new(800, 1280);
    let none = layout(&f, &st(GpsStatus::default()));
    assert!(none.pos_card.is_none() && none.copy_coords.is_none());
    let with = layout(&f, &st(fix()));
    let (a, b) = (with.copy_coords.unwrap(), with.copy_link.unwrap());
    assert!(a.h >= 56 && b.h >= 56 && a.w >= 56);
    assert!(a.right() < b.x, "buttons do not overlap");
    assert!(with.total > none.total);
    assert!(with.switch_row.h >= 56);
}
