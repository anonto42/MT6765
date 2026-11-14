use super::*;
use walpad_ui::Icon;

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[test]
fn grid_is_centred_fits_and_wraps() {
    for (f, cols) in [(Frame::new(800, 1280), 5), (Frame::new(1280, 800), 9)] {
        for banner in [0, 56] {
            let l = layout(&f, banner, 12);
            assert_eq!(l.tiles.len(), 12);
            let first_row = l.tiles.iter().filter(|t| t.y == l.tiles[0].y).count();
            assert_eq!(first_row, cols, "{}x{}", f.w, f.h);
            let row = &l.tiles[..cols];
            let (left, right) = (row[0].x, row[cols - 1].right());
            let c = f.padded();
            assert!((left - c.x - (c.right() - right)).abs() <= 1, "centred");
            assert!(l.bottom <= f.h, "fits without scrolling");
            assert!(l.tiles[0].y >= f.content.y + banner);
            for (i, a) in l.tiles.iter().enumerate() {
                assert!(a.w >= TOUCH_MIN && a.h >= TOUCH_MIN);
                for b in &l.tiles[i + 1..] {
                    assert!(!overlaps(*a, *b));
                }
            }
        }
    }
    // three apps: one centred row of three
    let l = layout(&Frame::new(800, 1280), 0, 3);
    let c = Frame::new(800, 1280).padded();
    assert_eq!(l.tiles[0].x - c.x, c.right() - l.tiles[2].right());
    assert!(layout(&Frame::new(800, 1280), 0, 0).tiles.is_empty());
}

#[test]
fn wifi_card_is_honest_about_what_is_known() {
    use crate::app::{AppEntry, WallTime};
    let _ = AppEntry {
        id: String::new(),
        name: String::new(),
        icon: Icon::Apps,
    };
    let mut st = crate::app::tests::status(120, &[]);
    let model = |st: walpad_proto::Status| {
        let mut m = Model::blank(WallTime::new(1, 1, "x", true));
        m.status = Some(st);
        m.connected = true;
        m
    };
    assert_eq!(
        wifi_card(&model(st.clone())).0,
        CardValue::Value("Home".into())
    );
    st.wifi.ssid = None;
    assert_eq!(
        wifi_card(&model(st.clone())),
        (
            CardValue::Value("Connected".into()),
            Some("192.168.0.100".into())
        ),
        "address but no SSID"
    );
    st.wifi.ip = None;
    assert_eq!(
        wifi_card(&model(st.clone())),
        (CardValue::Value("Not connected".into()), None)
    );
    st.wifi.radio = WifiRadio::Failed;
    assert_eq!(
        wifi_card(&model(st)).0,
        CardValue::Error("Wi-Fi failed".into())
    );
}
