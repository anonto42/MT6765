use super::layout::{pad_layout, pad_rect, strip_color, surface_bg, PadButton};
use super::state::PadState;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

/// The mini-map: the visible area in small, the pad's band greyed, a crosshair and a square marker
/// where the cursor is.
pub fn paint_map(p: &mut Painter, map: Rect, area: (i32, i32), cursor: (i32, i32)) {
    p.cv.rect(map.inset(-2), surface_bg());
    p.cv.rect(map, pressed_tint());
    let pad = pad_rect(area);
    let band_y = map.y + pad.y * map.h / area.1.max(1);
    p.cv.rect(
        Rect::new(map.x, band_y, map.w, map.bottom() - band_y),
        divider(),
    );
    let dx = map.x + cursor.0 * map.w / area.0.max(1);
    let dy = map.y + cursor.1 * map.h / area.1.max(1);
    // crosshair through the cursor (inside the map), then the square marker
    p.cv.rect(Rect::new(map.x, dy, map.w, 1), divider());
    p.cv.rect(Rect::new(dx, map.y, 1, map.h), divider());
    p.cv.rect(Rect::new(dx - 6, dy - 6, 13, 13), Color::hex(0x000000));
    p.cv.rect(Rect::new(dx - 4, dy - 4, 9, 9), text());
}

/// Draws the whole pad over the canvas (pad-sized).
pub fn paint_pad(p: &mut Painter, st: &PadState) {
    let area = st.area();
    let bounds = p.cv.bounds();
    let l = pad_layout(area);
    p.cv.rect(bounds, strip_color());
    p.divider(0, 0, bounds.w);
    p.outline_box(l.surface, 0, false, false, false);
    let compact = l.surface.h < 200;
    let hint = Rect::new(
        l.surface.x + SP_5,
        l.surface.y,
        l.map.x - l.surface.x - 2 * SP_5,
        l.surface.h,
    );
    let (title, lines) = (
        "Mouse pad",
        [
            "Drag to move the pointer",
            "Tap to click \u{b7} hold for a right click",
        ],
    );
    let lh = if compact { 20 } else { 24 };
    let block = lh + 6 + 2 * lh;
    let mut y = hint.y + (hint.h - block) / 2;
    p.text(
        title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(hint.x, y, hint.w, lh),
        Align::Left,
    );
    y += lh + 6;
    for s in lines {
        p.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
            Rect::new(hint.x, y, hint.w, lh),
            Align::Left,
        );
        y += lh;
    }
    paint_map(p, l.map, area, st.cursor);
    let pressed = st.pressed_button();
    for (r, b) in &l.buttons {
        p.outline_box(*r, 0, pressed == Some(*b), false, false);
        let label = |s: &str, p: &mut Painter| {
            p.text(
                s,
                Style::new(Face::Bold, SIZE_SECONDARY, text()),
                *r,
                Align::Center,
            )
        };
        match b {
            PadButton::Left => label("Left click", p),
            PadButton::Right => label("Right click", p),
            PadButton::ScrollUp => label("\u{2191} Scroll", p),
            PadButton::ScrollDown => label("\u{2193} Scroll", p),
            PadButton::Close => {
                // mouse with a chevron down: "put the pad away" (the keyboard's minimise key)
                let w = ICON + 2 + 16;
                let (x, y) = (r.x + (r.w - w) / 2, r.y + (r.h - ICON) / 2);
                p.cv.icon(Icon::Mouse, x, y, ICON, text());
                p.cv.icon(Icon::ChevronDown, x + ICON + 2, y + 4, 16, text());
            }
        }
    }
}
