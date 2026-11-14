use super::model::CARD_H;
use super::*;
use walpad_proto::HotspotInternet;

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

fn net(results: usize, saved: usize) -> NetModel {
    NetModel {
        results: (0..results)
            .map(|i| NetEntry {
                ssid_hex: format!("{i:02x}"),
                ssid: format!("Net {i}"),
                bars: 3,
                secured: i % 2 == 0,
            })
            .collect(),
        saved: (0..saved)
            .map(|i| SavedEntry {
                id: i as u32,
                ssid: format!("Saved {i}"),
            })
            .collect(),
        ..NetModel::default()
    }
}

#[test]
fn signal_steps() {
    assert_eq!(
        [-40, -55, -56, -65, -70, -75, -80, -95].map(bars_from_dbm),
        [4, 4, 3, 3, 2, 2, 1, 1]
    );
}

#[test]
fn layout_has_no_overlaps_and_rows_are_big_enough() {
    for (frame, n) in [
        (Frame::new(800, 1280), net(7, 3)),
        (Frame::new(1280, 800), net(0, 0)),
        (Frame::new(800, 1280), net(1, 0)),
    ] {
        let l = layout(&frame, &None, &n);
        let mut rs = vec![
            l.title,
            l.avail_label,
            l.scan_btn,
            l.avail_card,
            l.saved_label,
            l.saved_card,
            l.chip,
            l.switch,
            l.hotspot_label,
            l.hotspot_switch,
            l.hotspot_card,
            l.hotspot_note,
        ];
        rs.retain(|r| r.h > 0);
        // the scan button sits on the label row, to the right of the label
        for (i, a) in rs.iter().enumerate() {
            for b in &rs[i + 1..] {
                if (*a == l.scan_btn && *b == l.avail_label)
                    || (*b == l.scan_btn && *a == l.avail_label)
                {
                    continue;
                }
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
        assert!(
            l.avail_rows.iter().all(|r| r.h >= ROW_WIFI_H)
                && l.saved_rows
                    .iter()
                    .all(|(r, f)| r.h >= ROW_H && f.w >= TOUCH_MIN && r.contains(f.x, f.y))
        );
        assert!(l.scan_btn.h >= TOUCH_MIN);
        assert!(l.total > l.saved_card.bottom());
        assert!(l.total >= l.hotspot_note.bottom());
        for s in [l.switch, l.hotspot_switch] {
            let h = switch_hit(s);
            assert!(h.w >= TOUCH_MIN && h.h >= TOUCH_MIN && h.contains(s.x, s.y));
            assert!(h.right() <= frame.content.right(), "{h:?}");
        }
        assert!(l.hotspot_name.h >= ROW_H && l.hotspot_pass.y == l.hotspot_name.bottom());
    }
}

#[test]
fn many_networks_make_the_content_scroll() {
    let f = Frame::new(800, 1280);
    let short = layout(&f, &None, &net(1, 0)).total;
    let long = layout(&f, &None, &net(12, 4)).total;
    assert!(
        long > f.content.h,
        "12 networks do not fit one screen ({long})"
    );
    assert!(long > short);
    assert_eq!(layout(&f, &None, &net(12, 0)).avail_rows.len(), 12);
}

#[test]
fn connected_card_and_failed_hint_take_space_only_when_present() {
    let f = Frame::new(800, 1280);
    let base = layout(&f, &None, &net(2, 1));
    assert!(base.connected.is_none() && base.hint.is_none());
    let mut st = crate::app::tests::status(120, &[]);
    let with = layout(&f, &Some(st.clone()), &net(2, 1));
    assert!(with.connected.is_some());
    assert_eq!(with.total - base.total, CARD_H + SP_5);
    st.wifi.radio = WifiRadio::Failed;
    st.wifi.ssid = None;
    assert!(layout(&f, &Some(st), &net(2, 1)).hint.is_some());
}

#[test]
fn switched_off_wifi_shows_no_connection() {
    let mut st = crate::app::tests::status(120, &[]);
    assert_eq!(is_connected(&Some(st.clone())), Some("Home"));
    st.wifi.enabled = false;
    assert_eq!(is_connected(&Some(st.clone())), None);
    assert!(!wifi_enabled(&Some(st)));
    assert!(!wifi_enabled(&None));
}

#[test]
fn hotspot_status_lines() {
    use walpad_proto::HotspotStatus;
    let mut st = crate::app::tests::status(120, &[]);
    let lines = |s: &Status| -> Vec<String> {
        hotspot_lines(&Some(s.clone()))
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    };
    assert_eq!(lines(&st), ["Off", "Set a password to use the hotspot"]);
    st.hotspot = HotspotStatus {
        state: HotspotState::On,
        has_secret: true,
        clients: Some(1),
        internet: Some(HotspotInternet::Unavailable),
        exclusive: true,
        ..Default::default()
    };
    assert!(hotspot_on(&Some(st.clone())));
    assert_eq!(
        lines(&st),
        [
            "On, 1 device connected",
            "Internet sharing is not available: local network only",
            "Wi-Fi client paused while the hotspot is on"
        ]
    );
    st.hotspot.state = HotspotState::Failed;
    st.hotspot.reason = Some("hostapd is not installed".into());
    assert!(!hotspot_on(&Some(st.clone())));
    assert_eq!(lines(&st), ["Failed: hostapd is not installed"]);
}
