use super::*;
use crate::canvas::{Canvas, Rect};
use crate::tokens::*;

#[test]
fn the_cup_fits_both_orientations_and_keeps_its_aspect() {
    for area in [Rect::new(96, 64, 704, 1216), Rect::new(96, 64, 1184, 736)] {
        let r = coffee_rect(area);
        assert!(r.x >= area.x && r.right() <= area.right());
        assert!(r.y >= area.y && r.bottom() <= area.bottom());
        let aspect = r.w as f32 / r.h as f32;
        assert!((aspect - VIEW_W / VIEW_H).abs() < 0.01, "{aspect}");
    }
}

#[test]
fn strokes_stay_inside_the_rect() {
    let mut c = Canvas::new(300, 400, BG).unwrap();
    let r = Rect::new(50, 50, 200, 300);
    draw_coffee(&mut c, r, 3.0, WATERMARK);
    let lit: Vec<(i32, i32)> = (0..300)
        .flat_map(|x| (0..400).map(move |y| (x, y)))
        .filter(|&(x, y)| c.pixel(x, y) != Some(BG))
        .collect();
    assert!(lit.len() > 1500, "drew {}", lit.len());
    assert!(lit.iter().all(|&(x, y)| r.inset(-3).contains(x, y)));
}

#[test]
fn both_wallpapers_decode_at_panel_size_with_the_background_colour_on_top() {
    for (w, h) in [(1280, 800), (800, 1280)] {
        let c = wallpaper(w, h).expect("decodes");
        assert_eq!((c.width(), c.height()), (w, h));
        // the top is plain background (the art sits along the bottom)
        assert_eq!(c.pixel(w / 2, 10), Some(crate::tokens::BG));
    }
}

#[test]
fn live_wallpapers_render_cleanly_in_both_themes() {
    for mode in [
        crate::tokens::ThemeMode::Dark,
        crate::tokens::ThemeMode::Light,
    ] {
        for kind in [
            WallpaperKind::LiveCoffee,
            WallpaperKind::LiveAurora,
            WallpaperKind::LiveStars,
        ] {
            let mut cv =
                crate::canvas::Canvas::new(320, 200, crate::tokens::DARK_PALETTE.bg).unwrap();
            draw_live_wallpaper(&mut cv, None, kind, 1500, mode);
            assert_eq!((cv.width(), cv.height()), (320, 200));
            assert!(kind.is_live());
            assert_eq!(WallpaperKind::parse(kind.as_str()), kind);
        }
    }
    assert_eq!(WallpaperKind::parse("unknown"), WallpaperKind::Default);
}
