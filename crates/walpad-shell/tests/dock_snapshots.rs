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

/// The bottom of the screen over an app (a plain blue stand-in): collapsed = only the handle at
/// the right end of the edge; expanded = the strip with the handle (arrow down) on top of it.
fn bottom_shot(name: &str, area: (i32, i32), expanded: bool, pressed: bool) {
    use walpad_shell::dock::{dock_rect, handle_rect, render_handle, HANDLE_H};
    use walpad_ui::tokens::Color;
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let h = HANDLE_H + if expanded { DOCK_H } else { 0 };
    let top = area.1 - h;
    let mut c = Canvas::new(area.0 as u32, h as u32, Color::hex(0x336699)).unwrap();
    if expanded {
        let mut strip = Canvas::new(area.0 as u32, DOCK_H as u32, BG).unwrap();
        render(
            &mut Painter {
                cv: &mut strip,
                fonts: &mut f,
            },
            None,
        );
        c.blit(&strip, 0, dock_rect(area, 0).y - top);
    }
    let hr = handle_rect(area, 0, expanded);
    render_handle(
        &mut Painter {
            cv: &mut c,
            fonts: &mut f,
        },
        (hr.x, hr.y - top),
        expanded,
        pressed,
    );
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn dock_handle_collapsed_and_expanded() {
    bottom_shot("dock_handle_portrait", (800, 1280), false, false);
    bottom_shot("dock_handle_landscape", (1280, 800), false, false);
    bottom_shot("dock_handle_portrait_pressed", (800, 1280), false, true);
    bottom_shot("dock_expanded_portrait", (800, 1280), true, false);
    bottom_shot("dock_expanded_landscape", (1280, 800), true, false);
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
