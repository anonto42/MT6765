//! Lock screen snapshots (OpenSpec `lock-screen`): PIN, password and pattern, both orientations,
//! typed digits, the wrong-PIN message and the countdown.
use std::path::{Path, PathBuf};
use walpad_proto::LockKind;
use walpad_shell::app::WallTime;
use walpad_shell::lock_screen::{layout, LockScreen};
use walpad_ui::components::Painter;
use walpad_ui::snapshot::assert_snapshot_in;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn screen(kind: LockKind, area: (i32, i32)) -> LockScreen {
    LockScreen::new(
        kind,
        area,
        WallTime::new(9, 41, "Tuesday, 10 November", true),
    )
}

fn shot(name: &str, s: &mut LockScreen) {
    let mut f =
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
            .unwrap();
    let mut c = Canvas::new(s.area.0 as u32, s.area.1 as u32, BG).unwrap();
    s.render(&mut Painter {
        cv: &mut c,
        fonts: &mut f,
    });
    assert_snapshot_in(&golden(), name, &c);
}

#[test]
fn pin_portrait_and_landscape() {
    let mut p = screen(LockKind::Pin, (800, 1280));
    shot("lock_pin_portrait", &mut p);
    let mut l = screen(LockKind::Pin, (1280, 800));
    for d in ['4', '3', '2'] {
        l.keysym(d as u32);
    }
    shot("lock_pin_landscape_typing", &mut l);
}

#[test]
fn pin_wrong_and_countdown() {
    let mut s = screen(LockKind::Pin, (800, 1280));
    for d in ['1', '2', '3', '4', '5'] {
        s.keysym(d as u32);
    }
    s.keysym(0xff0d);
    s.rejected(Some(2), None);
    s.keysym('9' as u32);
    s.keysym('9' as u32);
    shot("lock_pin_wrong_two_tries_left", &mut s);
    s.rejected(None, Some(30));
    s.tick(3000);
    shot("lock_pin_countdown", &mut s);
}

#[test]
fn password_and_pattern() {
    let mut p = screen(LockKind::Password, (1280, 800));
    for c in "walpad".chars() {
        p.keysym(c as u32);
    }
    shot("lock_password_landscape", &mut p);
    let mut q = screen(LockKind::Password, (800, 1280));
    shot("lock_password_portrait", &mut q);
    let mut t = screen(LockKind::Pattern, (800, 1280));
    let l = layout((800, 1280), LockKind::Pattern);
    let (x, y) = l.dots[0];
    t.down(x, y);
    for d in [1, 2, 5, 4] {
        let (x, y) = l.dots[d];
        t.moved(x, y);
    }
    let (x, y) = (l.dots[4].0 + 60, l.dots[4].1 + 70);
    t.moved(x, y);
    shot("lock_pattern_portrait_drawing", &mut t);
    let mut t2 = screen(LockKind::Pattern, (1280, 800));
    shot("lock_pattern_landscape", &mut t2);
}
