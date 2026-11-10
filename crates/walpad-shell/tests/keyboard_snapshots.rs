//! System keyboard snapshots (replaces matchbox-keyboard): both orientations, the symbol layer
//! with a pressed key, and latched Shift + Ctrl.
use std::path::{Path, PathBuf};
use walpad_shell::keyboard::keyboard_rect;
use walpad_ui::components::{Painter, SysKbdState, SysKey};
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn shot(name: &str, area: (i32, i32), st: SysKbdState, pressed: Option<SysKey>) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let r = keyboard_rect(area);
    let mut c = Canvas::new(r.w as u32, r.h as u32, BG).unwrap();
    let mut p = Painter {
        cv: &mut c,
        fonts: &mut f,
    };
    let keys = p.system_keyboard(st, None);
    if let Some(k) = pressed {
        let i = keys.iter().position(|b| b.key == k).unwrap();
        p.system_keyboard(st, Some(i));
    }
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn keyboard_portrait_and_landscape() {
    shot(
        "keyboard_portrait",
        (800, 1280),
        SysKbdState::default(),
        None,
    );
    shot(
        "keyboard_landscape",
        (1280, 800),
        SysKbdState::default(),
        None,
    );
}

#[test]
fn keyboard_states() {
    shot(
        "keyboard_portrait_symbols_pressed",
        (800, 1280),
        SysKbdState {
            symbols: true,
            ..Default::default()
        },
        Some(SysKey::Char('|')),
    );
    shot(
        "keyboard_landscape_shift_ctrl_alt",
        (1280, 800),
        SysKbdState {
            symbols: false,
            shift: true,
            ctrl: true,
            alt: true,
        },
        Some(SysKey::Up),
    );
}
