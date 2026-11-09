//! Apps screen: grid of installed apps (2 columns portrait, 4 landscape). Content coordinates.
use crate::app::{Target, Tile};
use walpad_ui::components::Painter;
use walpad_ui::layout::{columns, Frame};
use walpad_ui::tokens::*;
use walpad_ui::Rect;

const TILE_H: i32 = 148;
const TITLE_H: i32 = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct AppsLayout {
    pub title: Rect,
    pub tiles: Vec<Rect>,
    pub total: i32,
}

pub fn layout(frame: &Frame, n: usize) -> AppsLayout {
    let c = frame.padded();
    let title = Rect::new(c.x, SP_4, c.w, TITLE_H);
    let cols = frame.columns();
    let top = SP_4 + TITLE_H + SP_4;
    let col_rects = columns(Rect::new(c.x, 0, c.w, TILE_H), cols, SP_5);
    let tiles: Vec<Rect> = (0..n as i32)
        .map(|i| {
            let r = col_rects[(i % cols) as usize];
            Rect::new(r.x, top + (i / cols) * (TILE_H + SP_5), r.w, TILE_H)
        })
        .collect();
    let rows = (n as i32 + cols - 1) / cols;
    AppsLayout {
        title,
        tiles,
        total: top + rows * (TILE_H + SP_5) + SP_5,
    }
}

pub fn render(
    p: &mut Painter,
    frame: &Frame,
    tiles: &[Tile],
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, tiles.len());
    p.screen_title("Apps", l.title);
    let mut hits = Vec::new();
    for (i, (r, t)) in l.tiles.iter().zip(tiles).enumerate() {
        p.app_tile(
            *r,
            t.icon,
            &t.name,
            pressed == Some(Target::AppTile(i)),
            t.running,
        );
        hits.push((*r, Target::AppTile(i)));
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_two_columns_portrait_four_landscape() {
        let p = layout(&Frame::new(800, 1280), 5);
        assert_eq!((p.tiles[0].w, p.tiles[0].h), (364, 148));
        assert_eq!(
            p.tiles.iter().map(|t| t.x).collect::<Vec<_>>(),
            [24, 412, 24, 412, 24]
        );
        assert_eq!(p.tiles[2].y - p.tiles[0].y, 148 + 24);
        let l = layout(&Frame::new(1280, 800), 5);
        assert_eq!(l.tiles.iter().filter(|t| t.y == l.tiles[0].y).count(), 4);
        assert!(l.tiles[4].y > l.tiles[0].y);
        assert!(p.total > p.tiles[4].bottom());
        assert_eq!(layout(&Frame::new(800, 1280), 0).tiles.len(), 0);
    }
}
