//! Unit tests for `LockScreen`.

use super::*;
use crate::app::WallTime;
use walpad_proto::LockKind;
use walpad_ui::components::{keyboard_layout, Key, Painter};
use walpad_ui::tokens::BG;
use walpad_ui::Rect;

const AREAS: [(i32, i32); 2] = [(800, 1280), (1280, 800)];

fn wall() -> WallTime {
    WallTime::new(9, 41, "Tuesday, 10 November", true)
}

fn screen(kind: LockKind, area: (i32, i32)) -> LockScreen {
    LockScreen::new(kind, area, wall())
}

/// Draw once so the hit areas exist, then tap the centre of the hit area `h`.
fn tap(s: &mut LockScreen, h: Hit) -> Out {
    let (mut cv, mut f) = canvas(s.area);
    s.render(&mut Painter {
        cv: &mut cv,
        fonts: &mut f,
    });
    let r = s.hits.iter().find(|(_, x)| *x == h).expect("hit").0;
    let (x, y) = r.center();
    s.down(x, y);
    s.up(x, y)
}

fn canvas(area: (i32, i32)) -> (walpad_ui::Canvas, walpad_ui::Fonts) {
    let f = walpad_ui::Fonts::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"),
    )
    .unwrap();
    (
        walpad_ui::Canvas::new(area.0 as u32, area.1 as u32, BG).unwrap(),
        f,
    )
}

#[test]
fn layouts_fit_the_screen_and_the_targets_are_big_enough() {
    for area in AREAS {
        let r = Rect::new(0, 0, area.0, area.1);
        for kind in [LockKind::Pin, LockKind::Password, LockKind::Pattern] {
            let l = layout(area, kind);
            for rect in [l.clock, l.prompt, l.date]
                .into_iter()
                .chain(l.pad.iter().map(|p| p.0))
                .chain((kind == LockKind::Password).then_some(l.keyboard))
                .chain((kind == LockKind::Password).then_some(l.submit))
                .chain((kind == LockKind::Password).then_some(l.entry))
            {
                assert!(
                    rect.x >= 0 && rect.y >= 0 && rect.right() <= r.w && rect.bottom() <= r.h,
                    "{kind:?} {area:?}: {rect:?}"
                );
            }
            for (rect, _) in &l.pad {
                assert!(rect.w >= 56 && rect.h >= 56);
            }
            if kind == LockKind::Pin {
                assert_eq!(l.pad.len(), 12);
                // the pad stays clear of the prompt
                assert!(l.pad.iter().all(|(p, _)| p.y > l.entry.y));
            }
            if kind == LockKind::Pattern {
                for (x, y) in l.dots {
                    assert!(x > 40 && x < area.0 - 40 && y > 40 && y < area.1 - 40);
                }
            }
            if kind == LockKind::Password {
                assert!(l.date.bottom() < l.prompt.y, "{area:?}: clock above prompt");
                assert!(l.entry.bottom() <= l.keyboard.y);
            }
        }
    }
}

#[test]
fn pin_digits_backspace_and_submit() {
    let mut s = screen(LockKind::Pin, (800, 1280));
    for d in ['4', '3', '2'] {
        assert_eq!(tap(&mut s, Hit::Digit(d)), Out::Redraw);
    }
    // too short: refused with a message
    assert_eq!(tap(&mut s, Hit::Submit), Out::Redraw);
    assert!(s.error.as_deref().unwrap().contains("at least 4"));
    tap(&mut s, Hit::Digit('1'));
    assert!(s.error.is_none(), "typing clears the message");
    tap(&mut s, Hit::Digit('9'));
    tap(&mut s, Hit::Backspace);
    assert_eq!(s.secret_len(), 4);
    assert_eq!(tap(&mut s, Hit::Submit), Out::Submit("4321".into()));
    assert_eq!(
        s.secret_len(),
        0,
        "the secret leaves the screen state at once"
    );
    assert!(s.busy);
    // nothing is accepted while waiting for the core
    assert_eq!(s.down(10, 10), Out::None);
    // a wrong PIN: message, tries left, field empty again
    s.rejected(Some(2), None);
    assert!(!s.busy);
    assert_eq!(s.error.as_deref(), Some("Wrong PIN. 2 tries left"));
    s.rejected(Some(4), None);
    assert_eq!(s.error.as_deref(), Some("Wrong PIN"));
}

#[test]
fn five_wrong_pins_show_a_countdown_and_the_pad_is_dead_during_it() {
    let mut s = screen(LockKind::Pin, (800, 1280));
    s.now = 1000;
    s.rejected(None, Some(30));
    assert!(s.waiting());
    assert_eq!(s.error.as_deref(), Some("Too many wrong attempts"));
    assert_eq!(s.countdown(), Some(30));
    // taps do nothing during the delay
    let (mut cv, mut f) = canvas(s.area);
    s.render(&mut Painter {
        cv: &mut cv,
        fonts: &mut f,
    });
    let r = s
        .hits
        .iter()
        .find(|(_, h)| *h == Hit::Digit('1'))
        .unwrap()
        .0;
    assert_eq!(s.down(r.x + 5, r.y + 5), Out::None);
    assert!(s.tick(2000), "the number changes every second");
    assert_eq!(s.countdown(), Some(29));
    assert!(!s.tick(2100));
    assert!(s.tick(31_000));
    assert!(!s.waiting());
    assert!(s.error.is_none(), "the message goes with the countdown");
    assert_eq!(s.down(r.x + 5, r.y + 5), Out::Redraw);
}

#[test]
fn next_deadline_is_the_next_second_of_the_countdown() {
    let mut s = screen(LockKind::Pin, (800, 1280));
    s.now = 0;
    s.set_retry(Some(30));
    s.tick(400);
    assert_eq!(s.next_deadline(), Some(1000), "29.6 s left: wake in 0.6 s");
    s.tick(1000);
    assert_eq!(s.next_deadline(), Some(2000));
    assert_eq!(screen(LockKind::Pin, (800, 1280)).next_deadline(), None);
}

#[test]
fn password_typed_on_the_keyboard_submits_with_unlock_or_done() {
    let mut s = screen(LockKind::Password, (1280, 800));
    let (mut cv, mut f) = canvas(s.area);
    s.render(&mut Painter {
        cv: &mut cv,
        fonts: &mut f,
    });
    let key = |s: &LockScreen, c: char| {
        let l = layout(s.area, s.kind);
        keyboard_layout(l.keyboard, s.layer)
            .iter()
            .position(|k| k.key == Key::Char(c))
            .unwrap()
    };
    for c in "abcd".chars() {
        let i = key(&s, c);
        tap(&mut s, Hit::Key(i));
    }
    // shift: next letter upper case, then back to lower
    let l = layout(s.area, s.kind);
    let shift = keyboard_layout(l.keyboard, s.layer)
        .iter()
        .position(|k| k.key == Key::Shift)
        .unwrap();
    tap(&mut s, Hit::Key(shift));
    let i = key(&s, 'E');
    tap(&mut s, Hit::Key(i));
    assert_eq!(s.secret_len(), 5);
    assert_eq!(tap(&mut s, Hit::Submit), Out::Submit("abcdE".into()));
    // a hardware keyboard works too
    s.failed("try again");
    for sym in ['h' as u32, 'i' as u32] {
        assert_eq!(s.keysym(sym), Out::Redraw);
    }
    assert_eq!(s.keysym(0xff0d), Out::Submit("hi".into()));
    // a PIN screen takes only digits from a hardware keyboard
    let mut p = screen(LockKind::Pin, (800, 1280));
    assert_eq!(p.keysym('a' as u32), Out::None);
    assert_eq!(p.keysym('7' as u32), Out::Redraw);
}

fn drag(s: &mut LockScreen, dots: &[usize]) -> Out {
    let l = layout(s.area, LockKind::Pattern);
    let (x, y) = l.dots[dots[0]];
    s.down(x, y);
    for d in &dots[1..] {
        let (x, y) = l.dots[*d];
        s.moved(x, y);
    }
    let (x, y) = l.dots[*dots.last().unwrap()];
    s.up(x, y)
}

#[test]
fn pattern_collects_dots_including_the_one_jumped_over() {
    let mut s = screen(LockKind::Pattern, (800, 1280));
    // 0 -> 2 passes over 1; 2 -> 8 passes over 5
    assert_eq!(drag(&mut s, &[0, 2, 8, 7]), Out::Submit("012587".into()));
    s.failed("x");
    // a knight's move jumps nothing; revisiting a dot is ignored
    assert_eq!(
        drag(&mut s, &[0, 5, 0, 5, 4, 3]),
        Out::Submit("0543".into())
    );
}

#[test]
fn a_short_pattern_is_refused_on_the_spot() {
    let mut s = screen(LockKind::Pattern, (1280, 800));
    assert_eq!(drag(&mut s, &[0, 1, 4]), Out::Redraw);
    assert_eq!(s.error.as_deref(), Some("Connect at least 4 dots"));
    assert_eq!(s.secret_len(), 0);
    assert!(!s.busy);
}
