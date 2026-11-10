//! M5.1 golden snapshots for the primitives: colour tokens + shapes, type roles, icons.
use std::path::Path;
use walpad_ui::snapshot::assert_snapshot;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect, Style};

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts")).unwrap()
}

#[test]
fn tokens_and_shapes() {
    let mut f = fonts();
    let mut c = Canvas::new(800, 400, BG).unwrap();
    let tokens = [
        ("bg", BG),
        ("text", TEXT),
        ("label", TEXT_LABEL),
        ("text2", TEXT_SECONDARY),
        ("disabled", TEXT_DISABLED),
        ("outline", OUTLINE),
        ("card", CARD_BORDER),
        ("divider", DIVIDER),
        ("pressed", PRESSED_FILL),
        ("watermark", WATERMARK),
        ("success", SUCCESS),
        ("warning", WARNING),
        ("danger", DANGER),
    ];
    for (i, (name, col)) in tokens.iter().enumerate() {
        let (x, y) = (24 + (i as i32 % 7) * 108, 24 + (i as i32 / 7) * 96);
        c.rrect_border(Rect::new(x, y, 96, 64), RADIUS, *col, DIVIDER);
        f.draw_in(
            &mut c,
            name,
            Style::new(Face::Regular, SIZE_CAPTION, TEXT_SECONDARY),
            Rect::new(x, y + 68, 96, 18),
            Align::Center,
        );
    }
    // card outline, outlined button, pressed button, icon boxes, slider, toggle
    c.rrect_stroke(Rect::new(24, 230, 360, 140), RADIUS_CARD, 1.0, CARD_BORDER);
    c.rrect_stroke(Rect::new(48, 254, 150, BUTTON_H), RADIUS, 2.0, TEXT);
    f.draw_in(
        &mut c,
        "Primary",
        Style::new(Face::Regular, SIZE_BODY, TEXT),
        Rect::new(48, 254, 150, BUTTON_H),
        Align::Center,
    );
    c.rrect(Rect::new(214, 254, 150, BUTTON_H), RADIUS, PRESSED_FILL);
    c.rrect_stroke(Rect::new(214, 254, 150, BUTTON_H), RADIUS, OUTLINE_W, TEXT);
    f.draw_in(
        &mut c,
        "Pressed",
        Style::new(Face::Regular, SIZE_BODY, TEXT),
        Rect::new(214, 254, 150, BUTTON_H),
        Align::Center,
    );
    c.rrect_stroke(Rect::new(48, 322, 64, 40), RADIUS_ICON, OUTLINE_W, OUTLINE);
    c.rrect_stroke(Rect::new(130, 322, 64, 40), 20, OUTLINE_W, OUTLINE);
    c.rrect_stroke(Rect::new(214, 322, 150, 32), RADIUS, OUTLINE_W, DANGER);
    c.rrect_stroke(Rect::new(420, 230, 356, 140), RADIUS_CARD, 1.0, CARD_BORDER);
    c.rrect(Rect::new(444, 302, 220, 4), 2, DIVIDER);
    c.rrect(Rect::new(444, 302, 140, 4), 2, TEXT);
    c.circle(584.0, 304.0, 12.0, TEXT);
    c.circle(584.0, 304.0, 4.0, BG);
    c.rrect_stroke(Rect::new(700, 290, 56, 32), 16, OUTLINE_W, OUTLINE);
    c.circle(716.0, 306.0, 8.0, OUTLINE);
    c.line((444.0, 340.0), (760.0, 340.0), 1.0, DIVIDER);
    assert_snapshot("primitives_tokens_shapes", &c);
}

#[test]
fn type_roles() {
    let mut f = fonts();
    let mut c = Canvas::new(800, 360, BG).unwrap();
    f.draw_in(
        &mut c,
        "14:22",
        Style::new(Face::ExtraLight, SIZE_CLOCK, TEXT),
        Rect::new(24, 8, 400, 110),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Network",
        Style::new(Face::Bold, SIZE_TITLE, TEXT),
        Rect::new(24, 124, 400, 40),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "AVAILABLE NETWORKS",
        Style::new(Face::Bold, SIZE_SECTION, TEXT_SECONDARY),
        Rect::new(24, 168, 400, 24),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Home network (row title)",
        Style::new(Face::Regular, SIZE_BODY, TEXT),
        Rect::new(24, 196, 500, 28),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Secondary text, units and hints",
        Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
        Rect::new(24, 228, 500, 24),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Caption / nav label",
        Style::new(Face::Regular, SIZE_CAPTION, TEXT_SECONDARY),
        Rect::new(24, 256, 500, 20),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "192.168.0.100  wlan0  COMPLETED",
        Style::new(Face::Mono, SIZE_MONO, TEXT),
        Rect::new(24, 280, 500, 24),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Warning: estimated from voltage",
        Style::new(Face::Regular, SIZE_SECONDARY, WARNING),
        Rect::new(24, 308, 400, 24),
        Align::Left,
    );
    f.draw_in(
        &mut c,
        "Café ✓ ünïcödé",
        Style::new(Face::Regular, SIZE_BODY, TEXT),
        Rect::new(420, 196, 360, 28),
        Align::Right,
    );
    f.draw_in(
        &mut c,
        "A very long network name that must be cut off with an ellipsis",
        Style::new(Face::Regular, SIZE_BODY, TEXT),
        Rect::new(420, 228, 360, 28),
        Align::Left,
    );
    assert_snapshot("primitives_type", &c);
}

#[test]
fn icon_sheet() {
    let mut f = fonts();
    let cols = 9;
    let rows = Icon::ALL.len().div_ceil(cols) as i32;
    let mut c = Canvas::new(800, (rows * 120 + 96) as u32, BG).unwrap();
    for (i, icon) in Icon::ALL.iter().enumerate() {
        let (cx, cy) = ((i % cols) as i32 * 88 + 8, (i / cols) as i32 * 120 + 8);
        c.rrect_stroke(Rect::new(cx, cy, 80, 80), RADIUS_ICON, OUTLINE_W, OUTLINE);
        c.icon(*icon, cx + 28, cy + 28, ICON, TEXT);
        f.draw_in(
            &mut c,
            &format!("{icon:?}"),
            Style::new(Face::Regular, 11.0, TEXT_SECONDARY),
            Rect::new(cx, cy + 84, 80, 16),
            Align::Center,
        );
    }
    // scaled 2x, tones
    let y = rows * 120 + 16;
    c.icon(Icon::Wifi, 24, y, 48, TEXT);
    c.icon(Icon::Settings, 90, y, 48, OUTLINE);
    c.icon(Icon::Battery, 156, y, 48, WARNING);
    c.icon(Icon::Warning, 222, y, 48, DANGER);
    c.icon(Icon::Check, 288, y, 48, SUCCESS);
    assert_snapshot("primitives_icons", &c);
}
