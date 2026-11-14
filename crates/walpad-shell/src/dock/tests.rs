use super::*;
use crate::app::Cmd;
use walpad_hw::Root;
use walpad_ui::tokens::TOUCH_MIN;
use walpad_ui::Rect;

#[test]
fn strip_sits_next_to_the_handle_in_both_orientations() {
    assert_eq!(
        dock_rect((800, 1280), 0),
        Rect::new(
            800 - DOCK_W - HANDLE_MARGIN,
            1280 - HANDLE_H - DOCK_H,
            DOCK_W,
            DOCK_H
        )
    );
    assert_eq!(
        dock_rect((1280, 800), 0),
        Rect::new(
            1280 - DOCK_W - HANDLE_MARGIN,
            800 - HANDLE_H - DOCK_H,
            DOCK_W,
            DOCK_H
        )
    );
    // directly above the handle when the keyboard is up
    assert_eq!(
        dock_rect((800, 1280), 384),
        Rect::new(
            800 - DOCK_W - HANDLE_MARGIN,
            896 - HANDLE_H - DOCK_H,
            DOCK_W,
            DOCK_H
        )
    );
    assert_eq!(
        dock_rect((1280, 800), 240),
        Rect::new(
            1280 - DOCK_W - HANDLE_MARGIN,
            560 - HANDLE_H - DOCK_H,
            DOCK_W,
            DOCK_H
        )
    );
    // when the handle is near the top edge, the strip opens below it
    assert_eq!(
        dock_rect_for_handle((1280, 800), Rect::new(0, 0, HANDLE_W, HANDLE_H)),
        Rect::new(0, HANDLE_H, DOCK_W, DOCK_H)
    );
    // when the handle is on the left side, the strip stays inside the left edge
    assert_eq!(
        dock_rect_for_handle((1280, 800), Rect::new(0, 400, HANDLE_W, HANDLE_H)),
        Rect::new(0, 400 - DOCK_H, DOCK_W, DOCK_H)
    );
}

#[test]
fn handle_clamps_to_every_side_of_the_display() {
    assert_eq!(clamp_handle_pos((-20, -10), (1280, 800), 0), (0, 0));
    assert_eq!(
        clamp_handle_pos((2000, 2000), (1280, 800), 0),
        (1280 - HANDLE_W, 800 - HANDLE_H)
    );
    // movable over the full screen even while keyboard is open
    assert_eq!(
        clamp_handle_pos((500, 790), (1280, 800), 240),
        (500, 800 - HANDLE_H)
    );
}

#[test]
fn area_comes_from_the_aspect_so_the_strut_shrink_does_not_move_the_dock() {
    assert_eq!(visible_area(800, 1280), (800, 1280));
    assert_eq!(
        visible_area(800, 1224),
        (800, 1280),
        "shell shrunk by the strut"
    );
    assert_eq!(visible_area(1280, 800), (1280, 800));
    assert_eq!(visible_area(1280, 744), (1280, 800));
}

#[test]
fn strut_counts_from_the_screen_edge() {
    // Xvfb is 1280x1280: portrait leaves 0 below the panel, landscape 480
    let p = strut_partial((800, 1280), 0, 1280);
    assert_eq!(
        (p[3], p[10], p[11]),
        (
            (DOCK_H + HANDLE_H) as u32,
            (800 - DOCK_W - HANDLE_MARGIN) as u32,
            (800 - HANDLE_MARGIN - 1) as u32
        )
    );
    let l = strut_partial((1280, 800), 0, 1280);
    assert_eq!(
        (l[3], l[10], l[11]),
        (
            (480 + DOCK_H + HANDLE_H) as u32,
            (1280 - DOCK_W - HANDLE_MARGIN) as u32,
            (1280 - HANDLE_MARGIN - 1) as u32
        )
    );
    // combined with the keyboard: dock + keyboard (+ 480 below the panel in landscape)
    assert_eq!(
        strut_partial((800, 1280), 384, 1280)[3],
        440 + HANDLE_H as u32
    );
    assert_eq!(
        strut_partial((1280, 800), 240, 1280)[3],
        776 + HANDLE_H as u32
    );
    assert!(p[..3].iter().chain(&p[4..10]).all(|v| *v == 0));
}

#[test]
fn four_buttons_tile_the_strip_with_big_targets() {
    for w in [800, 1280] {
        let rs = button_rects(w);
        assert_eq!(
            rs.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
            [
                DockButton::Back,
                DockButton::Home,
                DockButton::Keyboard,
                DockButton::CloseApp
            ]
        );
        assert_eq!(rs[0].0.x, 0);
        assert_eq!(rs[3].0.right(), w, "no gap at the right edge");
        for pair in rs.windows(2) {
            assert_eq!(pair[0].0.right(), pair[1].0.x);
        }
        assert!(rs.iter().all(|(r, _)| r.h >= TOUCH_MIN && r.w >= TOUCH_MIN));
    }
    // compact icon-only strip
    assert_eq!(button_at(DOCK_W, 10, 10), Some(DockButton::Back));
    assert_eq!(button_at(DOCK_W, 70, 30), Some(DockButton::Home));
    assert_eq!(button_at(DOCK_W, 130, 30), Some(DockButton::Keyboard));
    assert_eq!(button_at(DOCK_W, 200, 30), Some(DockButton::CloseApp));
    assert_eq!(button_at(DOCK_W, 100, 56), None, "below the strip");
}

#[test]
fn visibility_follows_the_active_window() {
    let (shell, dock) = (10, [20, 21]);
    assert!(!wants_visible(0, shell, &dock, false), "nothing active");
    assert!(
        !wants_visible(shell, shell, &dock, true),
        "the shell is in front"
    );
    assert!(wants_visible(77, shell, &dock, false), "a foreign window");
    assert!(
        wants_visible(20, shell, &dock, true),
        "a tap on the dock keeps it"
    );
    assert!(
        wants_visible(21, shell, &dock, true),
        "... or on its handle"
    );
    assert!(!wants_visible(20, shell, &dock, false));
}

#[test]
fn handle_sits_at_the_right_end_of_the_bottom_edge_or_on_the_strip() {
    // collapsed: on the bottom edge, nothing reserved
    assert_eq!(
        handle_rect((800, 1280), 0, false),
        Rect::new(800 - 56 - 8, 1252, 56, 28)
    );
    assert_eq!(
        handle_rect((1280, 800), 0, false),
        Rect::new(1280 - 56 - 8, 772, 56, 28)
    );
    // expanded: handle stays in place so a second tap collapses cleanly
    assert_eq!(handle_rect((800, 1280), 0, true).bottom(), 1280);
    assert_eq!(handle_rect((1280, 800), 0, true).bottom(), 800);
    // with the keyboard (or the mouse pad) up: above it, folded or not
    assert_eq!(handle_rect((800, 1280), 384, false).bottom(), 896);
    assert_eq!(handle_rect((800, 1280), 384, true).bottom(), 896);
    assert_eq!(handle_rect((1280, 800), 240, false).bottom(), 560);
    assert_eq!(handle_rect((1280, 800), 240, true).bottom(), 560);
    for area in [(800, 1280), (1280, 800)] {
        let h = handle_rect(area, 0, false);
        assert!(h.x >= 0 && h.right() <= area.0);
        assert!(h.w >= TOUCH_MIN, "touch-wide");
    }
}

#[test]
fn handle_shape_is_a_sharp_box() {
    let s = handle_shape();
    let inside = |x, y| s.iter().any(|r| r.contains(x, y));
    assert!(inside(0, 0) && inside(HANDLE_W - 1, 0), "sharp top corners");
    assert!(inside(HANDLE_W / 2, 0), "top edge");
    assert!(inside(0, HANDLE_H - 1) && inside(HANDLE_W - 1, HANDLE_H - 1));
    assert!(s
        .iter()
        .all(|r| r.x >= 0 && r.right() <= HANDLE_W && r.bottom() <= HANDLE_H));
}

#[test]
fn fold_expands_on_a_tap_and_collapses_by_itself() {
    let mut f = Fold::default();
    assert!(!f.expanded, "collapsed by default");
    assert_eq!(f.deadline(), None);
    f.toggle(1000);
    assert!(f.expanded);
    assert_eq!(f.deadline(), Some(1000 + AUTO_COLLAPSE_MS));
    // a touch on the strip restarts the timer
    f.touched(3000);
    assert!(!f.tick(1000 + AUTO_COLLAPSE_MS));
    assert!(f.expanded);
    assert!(f.tick(3000 + AUTO_COLLAPSE_MS), "folds after 5 s idle");
    assert!(!f.expanded);
    assert!(!f.tick(99_999), "only once");
    // a second tap on the handle folds it at once
    f.toggle(0);
    f.toggle(10);
    assert!(!f.expanded);
    assert_eq!(f.deadline(), None);
    // touches while folded do nothing
    f.touched(20);
    assert_eq!(f.deadline(), None);
}

#[test]
fn window_classes_map_to_app_ids() {
    let apps = vec![
        ("browser".to_string(), "firefox-esr".to_string()),
        ("terminal".to_string(), "st".to_string()),
    ];
    let c = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    assert_eq!(
        app_for_class(&apps, &c("Navigator", "firefox-esr")),
        Some("browser".into())
    );
    assert_eq!(
        app_for_class(&apps, &c("st", "St")),
        Some("terminal".into())
    );
    assert_eq!(app_for_class(&apps, &c("xterm", "XTerm")), None);
    assert_eq!(app_for_class(&apps, &[]), None);
}

#[test]
fn buttons_produce_the_right_commands() {
    let apps = vec![("terminal".to_string(), "st".to_string())];
    let st = vec!["st".to_string(), "St".to_string()];
    let other = vec!["xterm".to_string()];
    assert_eq!(command_for(DockButton::Home, &apps, &st), None);
    assert_eq!(
        command_for(DockButton::Back, &apps, &other),
        Some(Cmd::Back)
    );
    assert_eq!(
        command_for(DockButton::Keyboard, &apps, &other),
        Some(Cmd::KeyboardToggle)
    );
    assert_eq!(
        command_for(DockButton::CloseApp, &apps, &st),
        Some(Cmd::CloseApp("terminal".into()))
    );
    assert_eq!(
        command_for(DockButton::CloseApp, &apps, &other),
        None,
        "unknown window class: nothing happens"
    );
}

#[test]
fn wm_classes_are_read_from_apps_json() {
    let d = std::env::temp_dir().join(format!("walpad-dock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
    std::fs::write(
        d.join("etc/walpad/apps.json"),
        r#"[{"id":"browser","wm_class":"firefox-esr"},{"id":"x"},{"id":"t","wm_class":""}]"#,
    )
    .unwrap();
    assert_eq!(
        load_wm_classes(&Root::new(&d)),
        vec![("browser".to_string(), "firefox-esr".to_string())]
    );
    assert!(load_wm_classes(&Root::new(d.join("missing"))).is_empty());
}
