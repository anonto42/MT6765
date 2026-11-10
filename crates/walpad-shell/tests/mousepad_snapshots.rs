//! Mouse pad snapshots: both orientations (cursor in the middle), a moved cursor with a pressed
//! button.
use std::path::{Path, PathBuf};
use walpad_shell::mousepad::{pad_layout, pad_rect, paint_pad, PadButton, PadState};
use walpad_ui::components::Painter;
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn shot(name: &str, st: &PadState) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let r = pad_rect(st.area());
    let mut c = Canvas::new(r.w as u32, r.h as u32, BG).unwrap();
    paint_pad(
        &mut Painter {
            cv: &mut c,
            fonts: &mut f,
        },
        st,
    );
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn mouse_pad_portrait_and_landscape() {
    shot("mousepad_portrait", &PadState::new((800, 1280)));
    shot("mousepad_landscape", &PadState::new((1280, 800)));
}

#[test]
fn mouse_pad_moved_cursor_and_pressed_button() {
    let area = (1280, 800);
    let mut st = PadState::new(area);
    st.cursor = (100, 60);
    let r = pad_rect(area);
    let (b, _) = pad_layout(area)
        .buttons
        .into_iter()
        .find(|(_, x)| *x == PadButton::Right)
        .unwrap();
    st.press(r.x + b.center().0, r.y + b.center().1, 0);
    shot("mousepad_landscape_corner_right_pressed", &st);
    let mut p = PadState::new((800, 1280));
    p.cursor = (790, 880);
    shot("mousepad_portrait_bottom_right", &p);
}
