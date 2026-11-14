use super::layout::{button_rects, DockButton, BUTTONS, DOCK_H, HANDLE_H, HANDLE_W};
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

/// Draw the handle with its top-left corner at `at` (in its own window: (0, 0)). Sharp rectangular
/// box with the opaque strip colour, a 1 px top divider hairline, and an arrow pointing where a tap
/// moves the strip: up = expand, down = collapse.
pub fn render_handle(p: &mut Painter, at: (i32, i32), expanded: bool, pressed: bool) {
    let r = Rect::new(at.0, at.1, HANDLE_W, HANDLE_H);
    let strip = surface_strong().over(bg());
    let fill = if pressed {
        pressed_tint().over(strip)
    } else {
        strip
    };
    p.cv.rect(r, fill);
    p.divider(r.x, r.y, r.w);
    let (cx, cy) = (
        r.x as f32 + HANDLE_W as f32 / 2.0,
        r.y as f32 + HANDLE_H as f32 / 2.0 + 1.0,
    );
    let (dx, dy) = (8.0, 4.5);
    let (tip, base) = if expanded {
        (cy + dy, cy - dy)
    } else {
        (cy - dy, cy + dy)
    };
    p.cv.line((cx - dx, base), (cx, tip), 2.0, text());
    p.cv.line((cx, tip), (cx + dx, base), 2.0, text());
}

/// Draw the compact icon-only strip (`cv` is `w` x 56). `pressed` highlights one button.
pub fn render(p: &mut Painter, pressed: Option<DockButton>) {
    let w = p.cv.width();
    // its own window over other apps (no compositor): SURFACE_STRONG over BG flattened into one
    // opaque strip, a hairline border, and icon-only cells separated by 1 px dividers
    p.cv.rect(Rect::new(0, 0, w, DOCK_H), surface_strong().over(bg()));
    p.divider(0, 0, w);
    p.divider(0, DOCK_H - 1, w);
    p.cv.rect(Rect::new(0, 0, 1, DOCK_H), divider());
    p.cv.rect(Rect::new(w - 1, 0, 1, DOCK_H), divider());
    for (i, (r, b)) in button_rects(w).iter().enumerate() {
        let (_, _, icon) = BUTTONS[i];
        if pressed == Some(*b) {
            p.cv.rect(*r, pressed_tint());
        }
        if i > 0 {
            p.cv.rect(Rect::new(r.x, 1, 1, DOCK_H - 2), divider());
        }
        let colour = if *b == DockButton::CloseApp {
            danger()
        } else {
            text()
        };
        p.cv.icon(
            icon,
            r.x + (r.w - ICON) / 2,
            (DOCK_H - ICON) / 2,
            ICON,
            colour,
        );
    }
}
