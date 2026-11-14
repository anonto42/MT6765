//! Media screen formatting, layout, and rendering.

use super::{
    player_for, Item, Kind, MediaModel, MediaTarget, Section, Thumb, GAP, LIST_ROW_H, NAME_H,
    SEG_H, TILE, TILE_RADIUS, TITLE_H,
};
use crate::app::Target;
use walpad_proto::Status;
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Color, Face, Icon, Rect, Style};

/// "3:07" / "1:02:09".
pub fn fmt_duration(s: u32) -> String {
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

pub fn fmt_size(b: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if b >= 1024 * MB {
        format!("{:.1} GB", b as f64 / (1024.0 * MB as f64))
    } else if b >= MB {
        format!("{} MB", b / MB)
    } else {
        format!("{} KB", (b / 1024).max(1))
    }
}

/// File name without its extension, for list titles.
pub fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => name,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLayout {
    pub title: Rect,
    pub refresh: Rect,
    pub seg: Rect,
    /// One rectangle per list item (grid cell with its name line, or a list row).
    pub cells: Vec<Rect>,
    /// Where the spinner or the empty message goes.
    pub empty: Rect,
    pub total: i32,
}

pub fn layout(frame: &Frame, section: Section, n: usize) -> MediaLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let refresh = Rect::new(c.right() - TOUCH_MIN, y - 4, TOUCH_MIN, TOUCH_MIN);
    let title = Rect::new(c.x, y, refresh.x - SP_3 - c.x, TITLE_H);
    y += TOUCH_MIN + SP_2;
    let seg = Rect::new(c.x, y, c.w, SEG_H);
    y += SEG_H + SP_4;
    let mut cells = Vec::with_capacity(n);
    let empty = Rect::new(c.x, y, c.w, 220);
    if section.grid() {
        let cols = ((c.w + GAP) / (TILE + GAP)).max(1);
        let used = cols * TILE + (cols - 1) * GAP;
        let x0 = c.x + (c.w - used) / 2;
        let cell_h = TILE + NAME_H + GAP;
        for i in 0..n as i32 {
            cells.push(Rect::new(
                x0 + (i % cols) * (TILE + GAP),
                y + (i / cols) * cell_h,
                TILE,
                TILE + NAME_H,
            ));
        }
        y += (n as i32 + cols - 1) / cols * cell_h;
    } else {
        for i in 0..n as i32 {
            cells.push(Rect::new(c.x, y + i * LIST_ROW_H, c.w, LIST_ROW_H));
        }
        y += n as i32 * LIST_ROW_H;
    }
    let total = if n == 0 { empty.bottom() } else { y } + SP_6;
    MediaLayout {
        title,
        refresh,
        seg,
        cells,
        empty,
        total,
    }
}

/// Item index range (first, last) whose cells touch the view `scroll..scroll+view_h` (content
/// coordinates), plus one screen of look-ahead each way.
pub fn visible_range(l: &MediaLayout, scroll: i32, view_h: i32) -> Option<(usize, usize)> {
    let top = scroll - view_h;
    let bottom = scroll + 2 * view_h;
    let mut first = None;
    let mut last = 0;
    for (i, r) in l.cells.iter().enumerate() {
        if r.bottom() >= top && r.y <= bottom {
            first.get_or_insert(i);
            last = i;
        }
    }
    first.map(|f| (f, last))
}

pub struct View<'a> {
    pub m: &'a MediaModel,
    pub list: &'a [Item],
    pub st: &'a Option<Status>,
    pub pressed: Option<Target>,
    pub spinner_deg: u16,
}

fn blit_at(p: &mut Painter, src: &Canvas, x: i32, y: i32) {
    let (ox, oy) = p.cv.origin();
    p.cv.blit(src, x + ox, y + oy);
}

fn placeholder(p: &mut Painter, r: Rect, kind: Kind) {
    p.cv.rect(r, surface());
    let (cx, cy) = r.center();
    p.cv.icon(kind.icon(), cx - 20, cy - 20, 40, text_disabled());
}

fn duration_badge(p: &mut Painter, tile: Rect, secs: u32) {
    let s = fmt_duration(secs);
    let w = 12 + 8 * s.len() as i32;
    let b = Rect::new(tile.right() - w - SP_2, tile.bottom() - 26 - SP_2, w, 26);
    p.cv.rect(b, Tint::new(Color::hex(0x000000), 82));
    p.text(
        &s,
        Style::new(Face::Regular, SIZE_CAPTION, text()),
        b,
        Align::Center,
    );
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let sec = v.m.section;
    let l = layout(frame, sec, v.list.len());
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    p.screen_title("Media", l.title);
    let rt = Target::Media(MediaTarget::Refresh);
    p.round_button(l.refresh, Icon::Refresh, pr(rt), false);
    hits.push((l.refresh, rt));
    let pressed_seg = Section::ALL
        .iter()
        .position(|s| pr(Target::Media(MediaTarget::Section(*s as usize))));
    let segs = p.segmented(
        l.seg,
        &Section::ALL.map(Section::label),
        sec as usize,
        pressed_seg,
        false,
    );
    for (i, r) in segs.iter().enumerate() {
        hits.push((*r, Target::Media(MediaTarget::Section(i))));
    }

    if v.list.is_empty() {
        if v.m.scanning || !v.m.scanned {
            let (cx, cy) = l.empty.center();
            p.spinner(cx, cy - 20, 40, v.spinner_deg as f32);
            p.text(
                "Looking for media...",
                Style::new(Face::Regular, SIZE_BODY, text_secondary()),
                Rect::new(l.empty.x, cy + 16, l.empty.w, 28),
                Align::Center,
            );
        } else {
            let (title, hint) = sec.empty_text();
            let icon = match sec {
                Section::Photos => Icon::Image,
                Section::Videos => Icon::Media,
                Section::Music => Icon::Music,
                Section::Recent => Icon::Folder,
            };
            p.empty_state(l.empty, icon, title, hint);
        }
        return hits;
    }

    for (i, (it, r)) in v.list.iter().zip(&l.cells).enumerate() {
        let t = Target::Media(MediaTarget::Item(i));
        let info = v.m.infos.get(&it.path);
        let installed = player_for(it.kind, v.st).is_some();
        if sec.grid() {
            let tile = Rect::new(r.x, r.y, TILE, TILE);
            match v.m.thumbs.get(&it.path) {
                Some(Thumb::Ready(c)) => blit_at(p, c, tile.x, tile.y),
                _ => placeholder(p, tile, it.kind),
            }
            let _ = TILE_RADIUS;
            if pr(t) {
                p.cv.rect(tile, pressed_tint());
                p.cv.rect(Rect::new(tile.x, tile.bottom() - 3, tile.w, 3), text());
            } else {
                p.cv.rect(Rect::new(tile.x, tile.bottom() - 1, tile.w, 1), divider());
            }
            if it.kind == Kind::Video {
                let (cx, cy) = tile.center();
                if !matches!(v.m.thumbs.get(&it.path), Some(Thumb::Ready(_))) {
                    p.cv.icon(Icon::Media, cx - 20, cy - 20, 40, text_disabled());
                }
                if let Some(d) = info.and_then(|i| i.duration) {
                    duration_badge(p, tile, d);
                }
            }
            p.text(
                stem(&it.name),
                Style::new(
                    Face::Regular,
                    SIZE_CAPTION,
                    if installed {
                        text_secondary()
                    } else {
                        text_disabled()
                    },
                ),
                Rect::new(r.x, tile.bottom() + 2, TILE, NAME_H),
                Align::Left,
            );
        } else {
            if pr(t) {
                p.row_pressed(*r);
            }
            let icon_box = Rect::new(r.x + SP_2, r.y + (r.h - 48) / 2, 48, 48);
            match v.m.thumbs.get(&it.path) {
                Some(Thumb::Ready(c)) => {
                    if let Some(small) = c.scaled(48, 48) {
                        blit_at(p, &small, icon_box.x, icon_box.y);
                    }
                }
                _ => {
                    p.cv.rect(icon_box, surface_strong());
                    p.cv.icon(
                        it.kind.icon(),
                        icon_box.x + 12,
                        icon_box.y + 12,
                        24,
                        text_secondary(),
                    );
                }
            }
            let tx = icon_box.right() + SP_3;
            let tw = r.right() - tx - SP_3;
            let title = info
                .and_then(|i| i.title.clone())
                .unwrap_or_else(|| stem(&it.name).to_string());
            p.text(
                &title,
                Style::new(
                    Face::Regular,
                    SIZE_BODY,
                    if installed { text() } else { text_disabled() },
                ),
                Rect::new(tx, r.y + 10, tw, 26),
                Align::Left,
            );
            let mut sub = Vec::new();
            if let Some(a) = info.and_then(|i| i.artist.clone()) {
                sub.push(a);
            }
            if let Some(d) = info.and_then(|i| i.duration) {
                sub.push(fmt_duration(d));
            }
            if sub.is_empty() && it.size > 0 {
                sub.push(fmt_size(it.size));
            }
            if sec == Section::Recent {
                sub.push(it.kind.label().to_string());
            }
            p.text(
                &sub.join(" · "),
                Style::new(Face::Regular, SIZE_CAPTION, text_secondary()),
                Rect::new(tx, r.y + 38, tw, 22),
                Align::Left,
            );
            p.divider(r.x, r.bottom() - 1, r.w);
        }
        hits.push((*r, t));
    }
    hits
}
