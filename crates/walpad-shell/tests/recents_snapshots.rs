//! `app-switcher-navigation` snapshots: the Recents strip in portrait and landscape (three cards,
//! the front one with a thumbnail, the others with the app icon), a card being dragged up, and the
//! empty strip.
use std::path::{Path, PathBuf};
use walpad_shell::gesture::Gesture;
use walpad_shell::recents::{geometry, Card, Recents};
use walpad_ui::components::Painter;
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::{Color, BG};
use walpad_ui::{Canvas, Fonts, Icon};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap()
}

/// A stand-in window capture: a header bar, two columns of "text" lines on a light page.
fn fake_window(area: (i32, i32)) -> Canvas {
    let (w, h) = (area.0 as usize, area.1 as usize);
    let mut data = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = if y < h / 12 {
                (0x33, 0x66, 0x99)
            } else if (y / 24) % 2 == 0 && x > w / 10 && x < w - w / 10 && (x / 40) % 7 != 0 {
                (0x55, 0x5B, 0x66)
            } else {
                (0xEE, 0xEE, 0xE8)
            };
            let i = (y * w + x) * 4;
            data[i..i + 4].copy_from_slice(&[b, g, r, 255]);
        }
    }
    let (tw, th) = (
        area.0.min(640) as u32,
        (area.0.min(640) * area.1 / area.0) as u32,
    );
    Canvas::from_bgrx_scaled(w, h, &data, tw, th).unwrap()
}

fn cards(area: (i32, i32)) -> Vec<Card> {
    vec![
        Card {
            app: "browser".into(),
            name: "Browser".into(),
            icon: Icon::Globe,
            thumb: Some(fake_window(area)),
        },
        Card {
            app: "terminal".into(),
            name: "Terminal".into(),
            icon: Icon::Terminal,
            thumb: None,
        },
        Card {
            app: "files".into(),
            name: "Files".into(),
            icon: Icon::Folder,
            thumb: None,
        },
    ]
}

fn shot(name: &str, area: (i32, i32), r: &Recents) {
    let mut f = fonts();
    let mut c = Canvas::new(area.0 as u32, area.1 as u32, Color::hex(0x000000)).unwrap();
    r.render(&mut Painter {
        cv: &mut c,
        fonts: &mut f,
    });
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn recents_three_cards_portrait_and_landscape() {
    for (name, area) in [
        ("recents_portrait", (800, 1280)),
        ("recents_landscape", (1280, 800)),
    ] {
        shot(name, area, &Recents::new(area, cards(area)));
    }
}

#[test]
fn recents_card_dragged_up_and_empty() {
    let area = (800, 1280);
    let mut r = Recents::new(area, cards(area));
    let c = r.card_rect(1);
    let (x, y) = (c.x + 60, c.y + 150);
    r.on_gesture(&Gesture::Pressed { x, y }, 0);
    r.on_gesture(&Gesture::Cancel, 1);
    r.on_gesture(&Gesture::DragStart { x, y }, 2);
    r.on_gesture(&Gesture::Drag { dx: 0, dy: -70 }, 3);
    shot("recents_dragging", area, &r);
    shot("recents_empty", area, &Recents::new(area, Vec::new()));
    let _ = (geometry(area, 3), BG);
}

#[test]
fn recents_five_cards_scroll() {
    let area = (800, 1280);
    let mut cs = cards(area);
    cs.push(Card {
        app: "kali".into(),
        name: "Kali".into(),
        icon: Icon::Terminal,
        thumb: None,
    });
    cs.push(Card {
        app: "settings".into(),
        name: "Settings".into(),
        icon: Icon::Settings,
        thumb: None,
    });
    let mut r = Recents::new(area, cs);
    let c = r.card_rect(0);
    r.on_gesture(
        &Gesture::DragStart {
            x: c.x + 60,
            y: c.y + 100,
        },
        0,
    );
    r.on_gesture(&Gesture::Drag { dx: -300, dy: 4 }, 1);
    r.on_gesture(&Gesture::DragEnd, 2);
    assert!(r.scroll() > 0);
    shot("recents_scrolled", area, &r);
}
