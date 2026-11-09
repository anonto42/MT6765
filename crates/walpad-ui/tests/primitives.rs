//! M5.1 golden snapshots for the primitives: colour tokens + shapes, type roles, icons.
use std::path::Path;
use walpad_ui::snapshot::assert_snapshot;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect};

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")).unwrap()
}

#[test]
fn tokens_and_shapes() {
    let mut f = fonts();
    let mut c = Canvas::new(800, 400, BG).unwrap();
    let tokens = [
        ("bg", BG),
        ("surface", SURFACE),
        ("raised", SURFACE_RAISED),
        ("border", BORDER),
        ("text", TEXT),
        ("text2", TEXT_SECONDARY),
        ("disabled", TEXT_DISABLED),
        ("accent", ACCENT),
        ("pressed", ACCENT_PRESSED),
        ("on_accent", ON_ACCENT),
        ("success", SUCCESS),
        ("warning", WARNING),
        ("danger", DANGER),
    ];
    for (i, (name, col)) in tokens.iter().enumerate() {
        let (x, y) = (24 + (i as i32 % 7) * 108, 24 + (i as i32 / 7) * 96);
        c.rrect_border(Rect::new(x, y, 96, 64), RADIUS, *col, BORDER);
        f.draw_in(&mut c, name, Face::Regular, SIZE_CAPTION, Rect::new(x, y + 68, 96, 18), Align::Center, TEXT_SECONDARY);
    }
    // card, button states, field, toggle-like pill, circle
    c.rrect_border(Rect::new(24, 230, 360, 140), RADIUS_CARD, SURFACE, BORDER);
    c.rrect(Rect::new(48, 254, 150, BUTTON_H), RADIUS, ACCENT);
    f.draw_in(&mut c, "Primary", Face::Bold, SIZE_BODY, Rect::new(48, 254, 150, BUTTON_H), Align::Center, ON_ACCENT);
    c.rrect(Rect::new(214, 254, 150, BUTTON_H), RADIUS, ACCENT_PRESSED);
    f.draw_in(&mut c, "Pressed", Face::Bold, SIZE_BODY, Rect::new(214, 254, 150, BUTTON_H), Align::Center, ON_ACCENT);
    c.rrect_border(Rect::new(48, 322, 150, 32), RADIUS, SURFACE_RAISED, BORDER);
    c.rrect(Rect::new(214, 322, 150, 32), RADIUS, DANGER);
    c.rrect_border(Rect::new(420, 230, 356, 140), RADIUS_CARD, SURFACE, BORDER);
    c.rrect(Rect::new(444, 300, 220, 8), 4, SURFACE_RAISED);
    c.rrect(Rect::new(444, 300, 140, 8), 4, ACCENT);
    c.circle(584.0, 304.0, 14.0, ACCENT);
    c.circle(584.0, 304.0, 6.0, ON_ACCENT);
    c.rrect(Rect::new(700, 290, 64, 32), 16, SURFACE_RAISED);
    c.circle(716.0, 306.0, 11.0, TEXT_SECONDARY);
    assert_snapshot("primitives_tokens_shapes", &c);
}

#[test]
fn type_roles() {
    let mut f = fonts();
    let mut c = Canvas::new(800, 360, BG).unwrap();
    f.draw_in(&mut c, "14:22", Face::ExtraLight, SIZE_CLOCK, Rect::new(24, 8, 400, 110), Align::Left, TEXT);
    f.draw_in(&mut c, "Network", Face::Bold, SIZE_TITLE, Rect::new(24, 124, 400, 40), Align::Left, TEXT);
    f.draw_in(&mut c, "AVAILABLE NETWORKS", Face::Bold, SIZE_SECTION, Rect::new(24, 168, 400, 24), Align::Left, TEXT_SECONDARY);
    f.draw_in(&mut c, "Home network (row title)", Face::Regular, SIZE_BODY, Rect::new(24, 196, 500, 28), Align::Left, TEXT);
    f.draw_in(&mut c, "Secondary text, units and hints", Face::Regular, SIZE_SECONDARY, Rect::new(24, 228, 500, 24), Align::Left, TEXT_SECONDARY);
    f.draw_in(&mut c, "Caption / nav label", Face::Regular, SIZE_CAPTION, Rect::new(24, 256, 500, 20), Align::Left, TEXT_SECONDARY);
    f.draw_in(&mut c, "192.168.0.100  wlan0  COMPLETED", Face::Mono, SIZE_MONO, Rect::new(24, 280, 500, 24), Align::Left, TEXT);
    f.draw_in(&mut c, "Warning: estimated from voltage", Face::Regular, SIZE_SECONDARY, Rect::new(24, 308, 400, 24), Align::Left, WARNING);
    f.draw_in(&mut c, "Café ✓ ünïcödé", Face::Regular, SIZE_BODY, Rect::new(420, 196, 360, 28), Align::Right, TEXT);
    f.draw_in(&mut c, "A very long network name that must be cut off with an ellipsis", Face::Regular, SIZE_BODY, Rect::new(420, 228, 360, 28), Align::Left, TEXT);
    assert_snapshot("primitives_type", &c);
}

#[test]
fn icon_sheet() {
    let mut f = fonts();
    let cols = 9;
    let mut c = Canvas::new(800, 3 * 120 + 96, BG).unwrap();
    for (i, icon) in Icon::ALL.iter().enumerate() {
        let (cx, cy) = ((i % cols) as i32 * 88 + 8, (i / cols) as i32 * 120 + 8);
        c.rrect_border(Rect::new(cx, cy, 80, 80), RADIUS, SURFACE, BORDER);
        c.icon(*icon, cx + 28, cy + 28, ICON, TEXT);
        f.draw_in(&mut c, &format!("{icon:?}"), Face::Regular, 11.0, Rect::new(cx, cy + 84, 80, 16), Align::Center, TEXT_SECONDARY);
    }
    // accent, scaled 2x, small
    c.icon(Icon::Wifi, 24, 376, 48, ACCENT);
    c.icon(Icon::Settings, 90, 376, 48, ACCENT);
    c.icon(Icon::Battery, 156, 376, 48, WARNING);
    c.icon(Icon::Warning, 222, 376, 48, DANGER);
    c.icon(Icon::Check, 288, 376, 48, SUCCESS);
    assert_snapshot("primitives_icons", &c);
}
