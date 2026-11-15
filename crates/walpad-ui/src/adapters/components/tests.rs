use super::*;

#[test]
fn keyboard_fits_800_and_keys_are_big_enough() {
    for layer in [
        KeyboardLayer::Lower,
        KeyboardLayer::Upper,
        KeyboardLayer::Symbols,
    ] {
        let area = Rect::new(0, 900, 800, 280);
        let keys = keyboard_layout(area, layer);
        assert!(
            keys.iter().all(|k| k.rect.x >= 0 && k.rect.right() <= 800),
            "{layer:?} stays on screen"
        );
        assert!(
            keys.iter()
                .all(|k| k.rect.h >= TOUCH_MIN && k.rect.w >= TOUCH_MIN),
            "{layer:?} touch targets"
        );
        assert!(
            keys.iter().any(|k| k.key == Key::Space)
                && keys.iter().any(|k| k.key == Key::Backspace)
                && keys.iter().any(|k| k.key == Key::Layer)
        );
        // no overlaps
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                let overlap = a.rect.x < b.rect.right()
                    && b.rect.x < a.rect.right()
                    && a.rect.y < b.rect.bottom()
                    && b.rect.y < a.rect.bottom();
                assert!(!overlap, "{layer:?}: {:?} overlaps {:?}", a.key, b.key);
            }
        }
    }
    let lower = keyboard_layout(Rect::new(0, 0, 800, 280), KeyboardLayer::Lower);
    assert_eq!(
        lower
            .iter()
            .filter(|k| matches!(k.key, Key::Char(_)))
            .count(),
        26
    );
    let top_row: Vec<_> = lower.iter().filter(|k| k.rect.y == 0).collect();
    assert_eq!(top_row.len(), 10);
    assert_eq!(
        (top_row[1].rect.x - top_row[0].rect.right()),
        6,
        "6 px gaps"
    );
}

#[test]
fn row_runs_merge_adjacent_grouped_rows_only() {
    let r = |y, h| Rect::new(0, y, 100, h);
    let runs = row_runs([
        (r(0, 64), true),
        (r(64, 64), true),
        (r(128, 56), false),
        (r(184, 64), true),
        (r(260, 64), true), // gap above: a new run
    ]);
    assert_eq!(runs, vec![r(0, 128), r(184, 64), r(260, 64)]);
    assert!(row_runs([(r(0, 10), false)]).is_empty());
}

#[test]
fn tabs_are_in_spec_order() {
    let l: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
    assert_eq!(
        l,
        [
            "Home",
            "Apps",
            "Network",
            "Device",
            "Settings",
            "Mobile network",
            "Bluetooth",
            "VPN and DNS",
            "Media",
            "Phone",
            "GPS",
            "Notifications",
        ]
    );
}
