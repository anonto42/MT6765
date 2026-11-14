use super::apps_panel::PANEL_W;
use super::qs_model::QS_W;
use super::*;
use walpad_proto::BatterySource;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::{HEADER_H, SIDEBAR_W, SP_4, TOUCH_MIN};
use walpad_ui::{Icon, Rect};

const PANEL_PAD: i32 = 20;

#[test]
fn battery_time_text_rounds_to_five_minutes() {
    let mut b = walpad_proto::Battery {
        percent: Some(50),
        source: BatterySource::VoltageEstimate,
        state: None,
        mv: None,
        temp_c: None,
        low: false,
        time_left_min: Some(322),
        time_to_full_min: None,
        time_calculating: false,
    };
    assert_eq!(battery_time_text(&b).unwrap(), "5 h 20 min left");
    b.time_left_min = Some(120);
    assert_eq!(battery_time_text(&b).unwrap(), "2 h left");
    b.time_left_min = Some(40);
    assert_eq!(battery_time_text(&b).unwrap(), "40 min left");
    b.time_left_min = None;
    b.time_to_full_min = Some(70);
    assert_eq!(battery_time_text(&b).unwrap(), "1 h 10 min to full");
    b.time_to_full_min = None;
    assert_eq!(battery_time_text(&b), None);
    b.time_calculating = true;
    assert_eq!(battery_time_text(&b).unwrap(), "calculating...");
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[test]
fn quick_settings_hang_under_the_pill_and_fit_both_orientations() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let g = qs_geom(&f);
        assert_eq!(g.panel.right(), f.w, "flush with the right edge");
        assert!(g.panel.y == HEADER_H && g.panel.bottom() < f.h - SP_4);
        let rows = [
            g.wifi,
            g.hotspot,
            g.mobile,
            g.bluetooth,
            g.battery,
            g.slider,
            g.volume_slider,
            g.volume_mute,
            g.rotation,
            g.theme,
            g.screen_off,
        ];
        for (i, a) in rows.iter().enumerate() {
            assert!(a.h >= TOUCH_MIN, "{a:?}");
            assert!(g.panel.contains(a.x, a.y) && a.right() <= g.panel.right());
            for b in &rows[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
        // Rotation, Theme and Screen off are the last three rows
        assert_eq!(g.theme.y, g.rotation.bottom());
        assert_eq!(g.screen_off.y, g.theme.bottom());
        assert_eq!(g.panel.bottom(), g.screen_off.bottom() + 8);
    }
    assert_eq!(QS_W, 280);
}

#[test]
fn apps_panel_sits_next_to_the_sidebar_at_the_bottom() {
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let g = panel_geom(&f, 2, 7);
        assert_eq!(g.panel.x, SIDEBAR_W);
        assert_eq!(g.panel.bottom(), f.h);
        assert_eq!(g.panel.w, PANEL_W);
        assert_eq!(g.tools.len(), 7);
        assert_eq!(
            g.tools.iter().filter(|r| r.y == g.tools[0].y).count(),
            4,
            "four columns"
        );
        for r in g.active.iter().chain(&g.tools) {
            assert!(r.w >= TOUCH_MIN && r.h >= TOUCH_MIN, "{r:?}");
            assert!(r.right() <= g.panel.right() - PANEL_PAD + 1);
            assert!(r.bottom() <= g.panel.bottom());
        }
        assert!(g.active[0].bottom() < g.divider && g.divider < g.tools_label.y);
    }
    let none = panel_geom(&Frame::new(800, 1280), 0, 3);
    assert!(none.active.is_empty() && none.active_empty.is_some());
}

#[test]
fn close_badges_are_big_enough_and_leave_the_icon_centre_alone() {
    let g = panel_geom(&Frame::new(800, 1280), 4, TOOLS.len());
    for cell in &g.active {
        let (badge, hit) = close_badge(*cell);
        let b = cell_box(*cell);
        assert!(hit.w >= 40 && hit.h >= 40, "touch target >= 40 px");
        let (bx, by) = badge.center();
        // the badge sits on the box's top-right corner
        assert!(b.contains(bx - 1, by + 1) && !b.contains(bx + 2, by - 3));
        assert!(hit.contains(bx, by));
        // the icon centre (and the box's left half) still belong to the icon
        let (cx, cy) = b.center();
        assert!(!hit.contains(cx, cy) && !hit.contains(b.x + 2, b.y + 2));
        // inside the panel
        assert!(g.panel.contains(hit.x, hit.y) && hit.right() <= g.panel.right());
    }
}

#[test]
fn tools_grid_is_generic() {
    assert_eq!(TOOLS[0].label, "Mouse pad");
    assert_eq!(TOOLS[0].action, ToolAction::MousePad);
    assert_eq!(TOOLS[0].icon, Icon::Mouse);
    let g = panel_geom(&Frame::new(800, 1280), 1, TOOLS.len());
    assert_eq!(g.tools.len(), TOOLS.len());
}
