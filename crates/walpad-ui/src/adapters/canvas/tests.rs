use super::*;
use crate::tokens::*;

#[test]
fn fills_and_reads_back() {
    let mut c = Canvas::new(20, 20, BG).unwrap();
    c.rect(Rect::new(5, 5, 10, 10), ACCENT);
    assert_eq!(c.pixel(0, 0), Some(BG));
    assert_eq!(c.pixel(10, 10), Some(ACCENT));
    assert_eq!(c.pixel(20, 20), None);
    assert_eq!(c.rgba().len(), 20 * 20 * 4);
}

#[test]
fn rounded_rect_corners_are_cut_and_border_shows() {
    let mut c = Canvas::new(100, 60, BG).unwrap();
    c.rrect_border(Rect::new(10, 10, 80, 40), 16, PRESSED_FILL, DIVIDER);
    assert_eq!(
        c.pixel(11, 11),
        Some(BG),
        "corner pixel is outside the radius"
    );
    assert_eq!(
        c.pixel(50, 10),
        Some(DIVIDER),
        "top edge is the 1 px border"
    );
    assert_eq!(c.pixel(50, 30), Some(PRESSED_FILL));
}

#[test]
fn blit_and_crop_roundtrip() {
    let mut a = Canvas::new(40, 40, BG).unwrap();
    let mut s = Canvas::new(10, 10, WATERMARK).unwrap();
    s.rect(Rect::new(0, 0, 5, 5), DANGER);
    a.blit(&s, 20, 20);
    assert_eq!(a.pixel(21, 21), Some(DANGER));
    assert_eq!(a.pixel(28, 28), Some(WATERMARK));
    let c = a.crop(Rect::new(20, 20, 10, 10)).unwrap();
    assert_eq!(c.pixel(1, 1), Some(DANGER));
}

#[test]
fn origin_offsets_drawing_but_not_reads() {
    let mut c = Canvas::new(40, 40, BG).unwrap();
    c.set_origin(10, -100);
    assert!(!c.visible(Rect::new(0, 0, 5, 5)), "above the canvas");
    assert!(c.visible(Rect::new(0, 100, 5, 5)));
    c.rect(Rect::new(0, 100, 5, 5), DANGER);
    c.set_origin(0, 0);
    assert_eq!(c.pixel(12, 2), Some(DANGER));
    assert_eq!(c.pixel(2, 2), Some(BG));
}

#[test]
fn outlines_leave_the_inside_untouched() {
    let mut c = Canvas::new(60, 60, BG).unwrap();
    c.rrect_stroke(Rect::new(4, 4, 52, 52), 16, OUTLINE_W, TEXT);
    assert_eq!(c.pixel(30, 30), Some(BG), "transparent inside");
    assert_ne!(c.pixel(30, 4), Some(BG), "top edge is drawn");
    assert_eq!(c.pixel(30, 2), Some(BG), "nothing outside the rect");
    let mut d = Canvas::new(60, 60, DANGER).unwrap();
    assert!(d.copy_from(&c));
    assert_eq!(d.pixel(30, 30), Some(BG));
    assert!(!d.copy_from(&Canvas::new(10, 10, BG).unwrap()));
    let mut e = Canvas::new(60, 60, DANGER).unwrap();
    assert!(e.copy_rect_from(&c, Rect::new(20, -5, 20, 15)));
    assert_eq!(e.pixel(30, 8), Some(BG), "copied (clipped at the top)");
    assert_eq!(e.pixel(30, 20), Some(DANGER), "outside the rect untouched");
    assert_eq!(e.pixel(10, 5), Some(DANGER));
}

#[test]
fn translucent_fills_blend_and_stay_opaque() {
    let mut c = Canvas::new(40, 40, BG).unwrap();
    c.rect(Rect::new(0, 0, 20, 40), SURFACE_STRONG);
    let (px, want) = (c.pixel(5, 5).unwrap(), SURFACE_STRONG.over(BG));
    assert!(
        px.r.abs_diff(want.r) <= 1 && px.g.abs_diff(want.g) <= 1 && px.b.abs_diff(want.b) <= 1,
        "{px:?} ~ {want:?} (blend rounding)"
    );
    assert_eq!(c.pixel(30, 5), Some(BG), "outside untouched");
    assert!(c.rgba().chunks(4).all(|p| p[3] == 255), "still opaque");
    // a second layer darkens further (real blending, not a replace)
    c.rect(Rect::new(0, 0, 10, 10), SURFACE_STRONG);
    let twice = SURFACE_STRONG.over(SURFACE_STRONG.over(BG));
    let px = c.pixel(5, 5).unwrap();
    assert!(
        px.r.abs_diff(twice.r) <= 1 && px.b.abs_diff(twice.b) <= 1,
        "{px:?}"
    );
    // fully transparent: nothing drawn
    c.rrect(Rect::new(20, 0, 20, 20), 4, Tint::new(TEXT, 0));
    assert_eq!(c.pixel(30, 10), Some(BG));
    // opaque colours keep the plain write
    c.circle(30.0, 30.0, 6.0, DANGER);
    assert_eq!(c.pixel(30, 30), Some(DANGER));
}

#[test]
fn copy_rect_from_a_strip() {
    let strip = Canvas::new(10, 40, DANGER).unwrap();
    let mut c = Canvas::new(40, 40, BG).unwrap();
    c.copy_rect_from_at(&strip, (5, 0), Rect::new(0, 10, 12, 5));
    assert_eq!(c.pixel(4, 12), Some(BG), "left of the strip");
    assert_eq!(c.pixel(5, 12), Some(DANGER));
    assert_eq!(c.pixel(11, 14), Some(DANGER));
    assert_eq!(c.pixel(12, 12), Some(BG), "right of the rect");
    assert_eq!(c.pixel(8, 15), Some(BG), "below the rect");
}

#[test]
fn rect_helpers() {
    let r = Rect::new(10, 10, 100, 50);
    assert!(r.contains(10, 10) && !r.contains(110, 10));
    assert_eq!(r.inset(5), Rect::new(15, 15, 90, 40));
    assert_eq!(r.center(), (60, 35));
    assert_eq!((r.right(), r.bottom()), (110, 60));
}
