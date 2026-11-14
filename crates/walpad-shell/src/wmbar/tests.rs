use super::*;
use walpad_hw::Root;
use walpad_proto::wm::{LayoutMode, SnapZone, WRect};
use walpad_ui::tokens::SP_2;

fn apps() -> Vec<AppInfo> {
    vec![
        AppInfo {
            id: "browser".into(),
            name: "Browser".into(),
            class: "firefox-esr".into(),
            max: 1,
        },
        AppInfo {
            id: "terminal".into(),
            name: "Terminal".into(),
            class: "st".into(),
            max: 6,
        },
    ]
}

fn model(n: u32, free: bool, width: i32) -> BarModel {
    BarModel {
        width,
        tabs: (1..=n)
            .map(|i| Tab {
                win: 100 + i,
                label: format!("Terminal {i}"),
            })
            .collect(),
        active: 101,
        title: "Terminal".into(),
        can_new: n < 6,
        free,
    }
}

#[test]
fn the_bar_shows_for_multi_window_apps_and_in_free_mode_only() {
    let a = apps();
    let term = app_for(&a, &["st".into(), "St".into()]);
    let ff = app_for(&a, &["Navigator".into(), "firefox-esr".into()]);
    assert!(wants_visible(term, LayoutMode::Single));
    assert!(wants_visible(term, LayoutMode::Split));
    assert!(!wants_visible(ff, LayoutMode::Single));
    assert!(!wants_visible(ff, LayoutMode::Split));
    assert!(wants_visible(ff, LayoutMode::Free));
    assert!(!wants_visible(None, LayoutMode::Free));
}

#[test]
fn tabs_plus_and_snap_buttons_fit_in_both_widths_and_never_overlap() {
    for width in [800, 1280] {
        for n in 1..=6 {
            for free in [false, true] {
                let m = model(n, free, width);
                let l = layout(&m);
                for (i, (a, _)) in l.iter().enumerate() {
                    assert!(a.x >= 0 && a.right() <= width, "{a:?} in {width}");
                    assert!(a.h == BAR_H);
                    for (b, _) in l.iter().skip(i + 1) {
                        let overlap = a.x < b.right() && b.x < a.right() && a.w > 0 && b.w > 0;
                        assert!(
                            !overlap,
                            "{a:?} overlaps {b:?} (n={n} free={free} w={width})"
                        );
                    }
                }
                // every tab is big enough to hit
                assert!(tab_width(&m) >= TAB_MIN_W);
                if free {
                    let snaps = l.iter().filter(|(_, h)| matches!(h, Hit::Snap(_))).count();
                    assert_eq!(snaps, 5);
                }
            }
        }
    }
}

#[test]
fn hits_name_the_tab_its_close_the_plus_the_snaps_and_the_drag_area() {
    let m = model(2, true, 800);
    let tw = tab_width(&m);
    assert_eq!(hit(&m, SP_2 + 10, 20), Some(Hit::Tab(101)));
    assert_eq!(hit(&m, SP_2 + tw - 10, 20), Some(Hit::TabClose(101)));
    assert_eq!(hit(&m, SP_2 + tw + 10, 20), Some(Hit::Tab(102)));
    assert_eq!(hit(&m, SP_2 + 2 * tw + 10, 20), Some(Hit::New));
    assert_eq!(hit(&m, 800 - 10, 20), Some(Hit::Snap(SnapZone::Full)));
    assert_eq!(
        hit(&m, 800 - 5 * SNAP_W + 10, 20),
        Some(Hit::Snap(SnapZone::Left))
    );
    assert_eq!(hit(&m, SP_2 + 2 * tw + PLUS_W + 5, 20), Some(Hit::Drag));
    assert_eq!(hit(&m, 10, BAR_H + 5), None);
    // no snap buttons outside free mode
    let m = model(2, false, 800);
    assert_eq!(hit(&m, 800 - 10, 20), Some(Hit::Drag));
}

#[test]
fn plus_goes_away_at_the_window_limit_and_labels_shrink_with_the_tab() {
    let m = model(6, false, 800);
    assert!(!m.can_new);
    assert!(!layout(&m).iter().any(|(_, h)| *h == Hit::New));
    assert_eq!(tab_label("Terminal", 2, 168), "Terminal 2");
    assert_eq!(tab_label("Terminal", 2, 80), "2");
}

#[test]
fn strut_is_a_top_strut_over_the_visible_width() {
    let s = strut_partial(800);
    assert_eq!(&s[..4], &[0, 0, BAR_H as u32, 0]);
    assert_eq!((s[8], s[9]), (0, 799));
}

#[test]
fn grip_sits_in_the_window_corner_and_stays_on_screen() {
    let g = grip_rect(WRect::new(100, 100, 400, 300), (800, 1280));
    assert_eq!((g.x, g.y), (500 - GRIP, 400 - GRIP));
    let g = grip_rect(WRect::new(0, 0, 800, 1280), (800, 1280));
    assert_eq!((g.right(), g.bottom()), (800, 1280));
    let g = grip_rect(WRect::new(0, 0, 10, 10), (800, 1280));
    assert_eq!((g.x, g.y), (0, 0));
}

#[test]
fn apps_json_gives_names_classes_and_limits() {
    let dir = std::env::temp_dir().join(format!("walpad-wmbar-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("etc/walpad")).unwrap();
    std::fs::write(
        dir.join("etc/walpad/apps.json"),
        r#"[{"id":"terminal","name":"Terminal","exec":["st"],"wm_class":"st","max_instances":6},{"id":"files","name":"Files","exec":["pcmanfm"],"wm_class":"pcmanfm"},{"id":"x","name":"X","exec":["x"]}]"#,
    )
    .unwrap();
    let a = load_apps(&Root::new(&dir));
    assert_eq!(a.len(), 2);
    assert_eq!((a[0].max, a[1].max), (6, 1));
    let _ = std::fs::remove_dir_all(&dir);
}
