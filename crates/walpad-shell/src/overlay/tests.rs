use super::*;
use crate::app::Target;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::TOUCH_MIN;

#[test]
fn password_sheet_fits_both_orientations_and_targets_are_big_enough() {
    for (w, h) in [(800, 1280), (1280, 800)] {
        let g = password_geom(&Frame::new(w, h));
        assert!(
            g.sheet.x >= 0 && g.sheet.right() <= w && g.sheet.y >= 0 && g.sheet.bottom() == h,
            "{w}x{h}: {:?}",
            g.sheet
        );
        assert!(g.field.h >= TOUCH_MIN && g.cancel.h >= TOUCH_MIN && g.connect.h >= TOUCH_MIN);
        assert!(
            g.field.bottom() < g.keyboard.y
                && g.keyboard.bottom() < g.cancel.y
                && g.cancel.bottom() <= g.sheet.bottom()
        );
        assert!(g.cancel.right() < g.connect.x);
    }
}

#[test]
fn secret_validity_follows_wpa2_rules() {
    let mut s = PasswordSheet::new("00".into(), "Home".into());
    for (v, ok) in [
        ("", false),
        ("1234567", false),
        ("12345678", true),
        (&"a".repeat(63), true),
        (&"a".repeat(64), true),
        (&"g".repeat(64), false),
        (&"a".repeat(65), false),
    ] {
        s.secret = v.to_string();
        assert_eq!(s.valid(), ok, "{} chars", v.len());
    }
}

#[test]
fn app_sheet_has_close_only_when_running() {
    let f = Frame::new(800, 1280);
    let t = |r| {
        app_sheet_geom(&f, r)
            .1
            .into_iter()
            .map(|(_, t)| t)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        t(true),
        [Target::CloseApp, Target::PinToggle, Target::SheetCancel]
    );
    assert_eq!(t(false), [Target::PinToggle, Target::SheetCancel]);
    let (sheet, btns) = app_sheet_geom(&f, true);
    assert!(btns
        .iter()
        .all(|(r, _)| r.h >= TOUCH_MIN && r.y >= sheet.y && r.bottom() <= sheet.bottom()));
}
