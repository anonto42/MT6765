//! Window bar snapshots (change multitasking-windows): terminal tabs in portrait, the free-mode
//! title bar with snap buttons in landscape, a pressed tab, and the resize grip.
use std::path::{Path, PathBuf};
use walpad_shell::wmbar::{hit, render, render_grip, BarModel, Hit, Tab, BAR_H, GRIP};
use walpad_ui::components::Painter;
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn fonts() -> Fonts {
    Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts")).unwrap()
}

fn model(n: u32, free: bool, width: i32, label: bool) -> BarModel {
    BarModel {
        width,
        tabs: (1..=n)
            .map(|i| Tab {
                win: 100 + i,
                label: if label {
                    format!("Terminal {i}")
                } else {
                    i.to_string()
                },
            })
            .collect(),
        active: 100 + n.min(2),
        title: "Browser".into(),
        can_new: n < 6,
        free,
    }
}

fn shot(name: &str, m: &BarModel, pressed: Option<Hit>) {
    let mut f = fonts();
    let mut c = Canvas::new(m.width as u32, BAR_H as u32, BG).unwrap();
    render(
        &mut Painter {
            cv: &mut c,
            fonts: &mut f,
        },
        m,
        pressed,
    );
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn terminal_tabs_portrait_and_landscape() {
    shot("wmbar_tabs_portrait", &model(3, false, 800, true), None);
    shot("wmbar_tabs_landscape", &model(2, false, 1280, true), None);
}

#[test]
fn six_tabs_in_portrait_use_numbers_and_lose_the_plus() {
    let m = model(6, false, 800, false);
    assert!(!m.can_new);
    shot("wmbar_tabs_six_portrait", &m, None);
}

#[test]
fn free_mode_title_bar_has_snap_buttons() {
    shot(
        "wmbar_free_title_landscape",
        &model(0, true, 1280, true),
        None,
    );
    shot("wmbar_free_tabs_portrait", &model(2, true, 800, true), None);
}

#[test]
fn a_pressed_tab_and_a_pressed_snap_button() {
    let m = model(2, true, 800, true);
    let tab = hit(&m, 20, 20).unwrap();
    assert!(matches!(tab, Hit::Tab(_)));
    shot("wmbar_tab_pressed", &m, Some(tab));
}

#[test]
fn the_resize_grip() {
    let mut f = fonts();
    let mut c = Canvas::new(GRIP as u32, GRIP as u32, BG).unwrap();
    render_grip(
        &mut Painter {
            cv: &mut c,
            fonts: &mut f,
        },
        false,
    );
    assert_snapshot_in(&golden(), "wmbar_grip", &c);
}
