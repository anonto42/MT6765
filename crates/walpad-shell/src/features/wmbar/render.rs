use super::model::{layout, tab_width, BarModel, Hit, BAR_H, CLOSE_W, PLUS_W};
use walpad_proto::wm::SnapZone;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

/// A small picture of a snap zone: a sharp rectangular frame with the target part filled.
fn snap_glyph(p: &mut Painter, zone: SnapZone, at: Rect, colour: Color) {
    let s = 22;
    let g = Rect::new(at.x + (at.w - s) / 2, at.y + (at.h - s) / 2, s, s);
    p.cv.rect(Rect::new(g.x, g.y, s, 1), colour);
    p.cv.rect(Rect::new(g.x, g.bottom() - 1, s, 1), colour);
    p.cv.rect(Rect::new(g.x, g.y, 1, s), colour);
    p.cv.rect(Rect::new(g.right() - 1, g.y, 1, s), colour);
    let half = (s - 4) / 2;
    let fill = match zone {
        SnapZone::Left => Rect::new(g.x + 2, g.y + 2, half, s - 4),
        SnapZone::Right => Rect::new(g.x + 2 + half, g.y + 2, half, s - 4),
        SnapZone::Top => Rect::new(g.x + 2, g.y + 2, s - 4, half),
        SnapZone::Bottom => Rect::new(g.x + 2, g.y + 2 + half, g.x + s - 2 - (g.x + 2), half),
        SnapZone::Full => Rect::new(g.x + 2, g.y + 2, s - 4, s - 4),
    };
    p.cv.rect(fill, colour);
}

pub fn render(p: &mut Painter, m: &BarModel, pressed: Option<Hit>) {
    let w = p.cv.width();
    p.cv.rect(Rect::new(0, 0, w, BAR_H), surface_strong().over(bg()));
    p.divider(0, BAR_H - 1, w);
    let tw = tab_width(m);
    let mut x = SP_2;
    for (i, t) in m.tabs.iter().enumerate() {
        let r = Rect::new(x, 0, tw, BAR_H - 1);
        let down = matches!(pressed, Some(Hit::Tab(wn) | Hit::TabClose(wn)) if wn == t.win);
        let active = t.win == m.active;
        p.cv.rect(r, if active { surface() } else { surface_icon() });
        if active {
            p.cv.rect(Rect::new(r.x, BAR_H - 2, r.w, 2), text());
        }
        if down {
            p.cv.rect(r, pressed_tint());
        }
        p.cv.rect(Rect::new(r.right() - 1, 0, 1, BAR_H - 1), divider());
        let label = if tw >= 120 {
            t.label.clone()
        } else {
            (i + 1).to_string()
        };
        let colour = if active { text() } else { text_secondary() };
        p.text(
            &label,
            Style::new(Face::Regular, SIZE_SECONDARY, colour),
            Rect::new(r.x + SP_3, 0, r.w - CLOSE_W - SP_2, BAR_H),
            Align::Left,
        );
        let cx = x + tw - CLOSE_W + (CLOSE_W - 16) / 2 - 2;
        p.cv.icon(Icon::Close, cx, (BAR_H - 16) / 2, 16, text_secondary());
        x += tw;
    }
    if m.tabs.is_empty() && !m.title.is_empty() {
        p.text(
            &m.title,
            Style::new(Face::Regular, SIZE_SECONDARY, text_label()),
            Rect::new(SP_3, 0, w / 2, BAR_H),
            Align::Left,
        );
    }
    if m.can_new {
        let r = Rect::new(x, 0, PLUS_W, BAR_H - 1);
        p.cv.rect(r, surface_icon());
        if pressed == Some(Hit::New) {
            p.cv.rect(r, pressed_tint());
        }
        p.cv.rect(Rect::new(r.right() - 1, 0, 1, BAR_H - 1), divider());
        let (cx, cy) = r.center();
        let (cx, cy) = (cx as f32, cy as f32);
        p.cv.line((cx - 8.0, cy), (cx + 8.0, cy), 2.0, text());
        p.cv.line((cx, cy - 8.0), (cx, cy + 8.0), 2.0, text());
    }
    if m.free {
        for (r, h) in layout(m) {
            if let Hit::Snap(z) = h {
                if pressed == Some(h) {
                    p.cv.rect(Rect::new(r.x, 0, r.w, BAR_H - 1), pressed_tint());
                }
                p.cv.rect(Rect::new(r.x, 0, 1, BAR_H - 1), divider());
                let colour = if pressed == Some(h) {
                    text()
                } else {
                    text_label()
                };
                snap_glyph(p, z, r, colour);
            }
        }
    }
}

/// The resize grip: sharp square box with two diagonal strokes in the corner.
pub fn render_grip(p: &mut Painter, pressed: bool) {
    let s = p.cv.width();
    p.cv.rect(
        Rect::new(0, 0, s, s),
        if pressed {
            pressed_tint().over(surface_strong().over(bg()))
        } else {
            surface_strong().over(bg())
        },
    );
    p.divider(0, 0, s);
    p.cv.rect(Rect::new(0, 0, 1, s), divider());
    let m = s as f32;
    for k in [0.0f32, 9.0] {
        p.cv.line(
            (m - 14.0 - k, m - 10.0),
            (m - 10.0, m - 14.0 - k),
            2.0,
            text(),
        );
    }
}
