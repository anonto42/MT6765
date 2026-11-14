use super::*;
use walpad_ui::components::Tone;

#[test]
fn rows_and_controls_meet_the_touch_minimums_and_do_not_overlap() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let (placed, total) = layout(&f, &None, &DevdSettings::default());
        let mut last = 0;
        for Placed { item, rect } in &placed {
            assert!(rect.y >= last, "{item:?} overlaps the previous item");
            last = rect.bottom();
            if matches!(
                item,
                Item::RotationRow(_) | Item::LidRow | Item::DimSeg | Item::OffSeg | Item::Info(..)
            ) {
                assert!(rect.h >= TOUCH_MIN, "{item:?} is {} px high", rect.h);
            }
            assert!(rect.right() <= f.w);
        }
        assert!(total > last, "content ends after the last item");
        assert!(total > f.content.h, "the Device tab scrolls");
    }
}

#[test]
fn unavailable_hardware_is_labelled_not_hidden() {
    let st = crate::app::tests::status(120, &[]);
    let it = items(&Some(st), &DevdSettings::default());
    let find = |name: &str| {
        it.iter().find_map(|i| match i {
            Item::Info(l, v, t) if l == name => Some((v.clone(), *t)),
            _ => None,
        })
    };
    assert_eq!(find("Audio"), Some(("Unverified".into(), Tone::Warning)));
    assert_eq!(
        find("Bluetooth"),
        Some(("Unavailable".into(), Tone::Danger))
    );
    assert_eq!(find("SD card"), Some(("Unavailable".into(), Tone::Danger)));
    assert_eq!(find("Camera"), Some(("Unsupported".into(), Tone::Danger)));
    assert_eq!(find("USB OTG"), Some(("Not tested".into(), Tone::Warning)));
    assert_eq!(find("Accelerometer"), Some(("Working".into(), Tone::Info)));
    assert_eq!(
        find("Calibration"),
        Some(("Not available yet".into(), Tone::Warning))
    );
    assert_eq!(
        find("Level"),
        Some(("~62% (estimated from voltage)".into(), Tone::Warning)),
        "the estimate is labelled as one"
    );
    assert_eq!(find("Voltage"), Some(("3.92 V".into(), Tone::Info)));
    assert_eq!(find("Temperature"), Some(("34.0 °C".into(), Tone::Info)));
    assert_eq!(
        find("Low battery below"),
        Some(("3550 mV".into(), Tone::Info))
    );
    // offline: everything is unknown, nothing is invented
    let off = items(&None, &DevdSettings::default());
    assert!(off
        .iter()
        .any(|i| matches!(i, Item::Info(l, v, _) if l == "Level" && v == "Unknown")));
    assert!(off
        .iter()
        .any(|i| matches!(i, Item::Info(l, v, _) if l == "Accelerometer" && v == "Unknown")));
}

#[test]
fn rotation_selection_follows_status() {
    let mut st = crate::app::tests::status(120, &[]);
    assert_eq!(
        selected_rotation(&Some(st.clone())),
        Some(0),
        "unlocked = Auto"
    );
    st.rotation_locked = true;
    for (o, want) in [(0, 1), (90, 2), (180, 3), (270, 4)] {
        st.orientation = o;
        assert_eq!(selected_rotation(&Some(st.clone())), Some(want));
    }
    assert_eq!(selected_rotation(&None), None);
    assert_eq!(selected_option(&OFF_OPTIONS, 240), Some(4));
    assert_eq!(selected_option(&OFF_OPTIONS, 0), Some(6), "Never");
    assert_eq!(
        selected_option(&DIM_OPTIONS, 77),
        None,
        "a custom value selects nothing"
    );
}
