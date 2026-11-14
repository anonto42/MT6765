use super::sidebar::PAD_TOP;
use super::*;
use walpad_ui::Icon;

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

#[test]
fn sidebar_has_home_pinned_settings_and_the_toggle_at_the_bottom() {
    let pinned = [(0, Icon::Globe), (1, Icon::Terminal)];
    for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
        let l = sidebar_layout(&f, &pinned, false);
        let t: Vec<Target> = l.items.iter().map(|i| i.target).collect();
        assert_eq!(
            t,
            [
                Target::Nav(NAV_HOME),
                Target::Tile(0),
                Target::Tile(1),
                Target::Nav(NAV_SETTINGS),
                Target::Nav(NAV_APPS)
            ]
        );
        let toggle = l.items.last().unwrap();
        assert!(toggle.round && toggle.icon == Icon::ChevronRight);
        assert_eq!(toggle.rect.bottom(), f.h - PAD_TOP);
        for (i, a) in l.items.iter().enumerate() {
            assert!(a.rect.w >= TOUCH_MIN && a.hit.w == SIDEBAR_W);
            assert!(f.sidebar.contains(a.rect.x, a.rect.y));
            for b in &l.items[i + 1..] {
                assert!(!overlaps(a.hit, b.hit), "{a:?} / {b:?}");
            }
        }
        assert!(l.items[2].rect.bottom() < l.divider_y && l.divider_y < l.items[3].rect.y);
    }
    let open = sidebar_layout(&Frame::new(800, 1280), &pinned, true);
    assert_eq!(open.items.last().unwrap().icon, Icon::Back);
}

#[test]
fn too_many_pins_never_push_settings_off_the_bar() {
    let pinned: Vec<(usize, Icon)> = (0..12).map(|i| (i, Icon::Apps)).collect();
    let l = sidebar_layout(&Frame::new(1280, 800), &pinned, false);
    let settings = l
        .items
        .iter()
        .find(|i| i.target == Target::Nav(NAV_SETTINGS))
        .unwrap();
    let toggle = l.items.last().unwrap();
    assert!(settings.hit.bottom() <= toggle.hit.y);
}
