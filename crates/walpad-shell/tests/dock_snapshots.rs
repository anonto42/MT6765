//! M5.8 snapshots: the system dock strip in both orientations and with a pressed button.
use std::path::{Path, PathBuf};
use walpad_shell::dock::{render, DockButton, DOCK_H};
use walpad_ui::components::Painter;
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn shot(name: &str, w: i32, pressed: Option<DockButton>) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let mut c = Canvas::new(w as u32, DOCK_H as u32, BG).unwrap();
    render(
        &mut Painter {
            cv: &mut c,
            fonts: &mut f,
        },
        pressed,
    );
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn dock_portrait_and_landscape() {
    shot("dock_portrait", 800, None);
    shot("dock_landscape", 1280, None);
}

#[test]
fn dock_pressed_buttons() {
    shot("dock_portrait_home_pressed", 800, Some(DockButton::Home));
    shot(
        "dock_landscape_close_pressed",
        1280,
        Some(DockButton::CloseApp),
    );
}
