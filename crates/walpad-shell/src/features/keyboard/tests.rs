use super::keymap::*;
use super::*;
use walpad_ui::components::{SysKbdState, SysKey};
use walpad_ui::Rect;

#[test]
fn thirty_percent_of_the_visible_height_flush_with_the_bottom() {
    assert_eq!(height((800, 1280)), 384);
    assert_eq!(height((1280, 800)), 240);
    assert_eq!(keyboard_rect((800, 1280)), Rect::new(0, 896, 800, 384));
    assert_eq!(keyboard_rect((1280, 800)), Rect::new(0, 560, 1280, 240));
}

#[test]
fn strut_counts_from_the_screen_edge() {
    let p = strut_partial((800, 1280), 1280);
    assert_eq!(p, [0, 0, 0, 384, 0, 0, 0, 0, 0, 0, 0, 799]);
    let l = strut_partial((1280, 800), 1280);
    assert_eq!(l, [0, 0, 0, 720, 0, 0, 0, 0, 0, 0, 0, 1279], "480 + 240");
}

#[test]
fn layout_fills_both_orientations_with_big_enough_keys() {
    for area in [(800, 1280), (1280, 800)] {
        let r = keyboard_rect(area);
        for symbols in [false, true] {
            let keys = system_keyboard_layout(Rect::new(0, 0, r.w, r.h), symbols);
            assert!(keys.iter().all(|k| k.rect.x >= 0 && k.rect.right() <= r.w));
            assert!(keys.iter().all(|k| k.rect.y >= 0 && k.rect.bottom() <= r.h));
            assert!(
                keys.iter().all(|k| k.rect.h >= 40 && k.rect.w >= 40),
                "{area:?}: keys >= 40 px"
            );
            // no overlaps
            for (i, a) in keys.iter().enumerate() {
                for b in &keys[i + 1..] {
                    let overlap = a.rect.x < b.rect.right()
                        && b.rect.x < a.rect.right()
                        && a.rect.y < b.rect.bottom()
                        && b.rect.y < a.rect.bottom();
                    assert!(!overlap, "{:?} overlaps {:?}", a.key, b.key);
                }
            }
            for k in [
                SysKey::Esc,
                SysKey::Tab,
                SysKey::Ctrl,
                SysKey::Alt,
                SysKey::Paste,
                SysKey::Layer,
                SysKey::Space,
                SysKey::Backspace,
                SysKey::Enter,
                SysKey::Up,
                SysKey::Down,
                SysKey::Left,
                SysKey::Right,
                SysKey::Hide,
            ] {
                assert_eq!(keys.iter().filter(|b| b.key == k).count(), 1, "{k:?} once");
            }
            assert_eq!(
                keys.iter().any(|b| b.key == SysKey::Shift),
                !symbols,
                "Shift on the letter layer"
            );
            // inverted T: Up sits right above Down
            let at = |k| keys.iter().find(|b| b.key == k).unwrap().rect;
            let (up, down) = (at(SysKey::Up), at(SysKey::Down));
            assert_eq!((up.x, up.w), (down.x, down.w));
            assert!(up.bottom() < down.y);
            // the right edges line up, with the side padding
            let right = keys.iter().map(|k| k.rect.right()).max().unwrap();
            assert!(r.w - right <= 8);
        }
        let lower = system_keyboard_layout(Rect::new(0, 0, r.w, r.h), false);
        let letters = lower
            .iter()
            .filter(|k| matches!(k.key, SysKey::Char(c) if c.is_ascii_lowercase()))
            .count();
        assert_eq!(letters, 26);
        let digits = lower
            .iter()
            .filter(|k| matches!(k.key, SysKey::Char(c) if c.is_ascii_digit()))
            .count();
        assert_eq!(digits, 10);
    }
    let land = keyboard_rect((1280, 800));
    let k = system_keyboard_layout(Rect::new(0, 0, land.w, land.h), false);
    assert!(k.iter().all(|b| b.rect.h >= 40), "landscape keys >= 40 px");
}

fn stroke(keysym: u32, ctrl: bool, shift: bool) -> Tap {
    Tap::Send(Stroke {
        keysym,
        ctrl,
        shift,
        alt: false,
    })
}

#[test]
fn taps_map_to_keysyms_with_one_shot_modifiers() {
    let mut st = SysKbdState::default();
    assert_eq!(
        tap(&mut st, SysKey::Char('a')),
        stroke('a' as u32, false, false)
    );
    // shift: one upper-case letter, then back to lower case
    assert_eq!(tap(&mut st, SysKey::Shift), Tap::State);
    assert!(st.shift);
    assert_eq!(
        tap(&mut st, SysKey::Char('a')),
        stroke('A' as u32, false, false)
    );
    assert!(!st.shift);
    assert_eq!(
        tap(&mut st, SysKey::Char('a')),
        stroke('a' as u32, false, false)
    );
    // ctrl+c
    tap(&mut st, SysKey::Ctrl);
    assert!(st.ctrl);
    assert_eq!(
        tap(&mut st, SysKey::Char('c')),
        stroke('c' as u32, true, false)
    );
    assert!(!st.ctrl);
    // tapping a latched modifier again releases it
    tap(&mut st, SysKey::Ctrl);
    tap(&mut st, SysKey::Ctrl);
    assert!(!st.ctrl);
    // named keys
    for (k, sym) in [
        (SysKey::Esc, XK_ESCAPE),
        (SysKey::Tab, XK_TAB),
        (SysKey::Backspace, XK_BACKSPACE),
        (SysKey::Enter, XK_RETURN),
        (SysKey::Up, XK_UP),
        (SysKey::Down, XK_DOWN),
        (SysKey::Left, XK_LEFT),
        (SysKey::Right, XK_RIGHT),
        (SysKey::Space, 0x20),
    ] {
        assert_eq!(tap(&mut st, k), stroke(sym, false, false), "{k:?}");
    }
    // shift+Tab carries an explicit shift; digits are unaffected by shift
    tap(&mut st, SysKey::Shift);
    assert_eq!(tap(&mut st, SysKey::Tab), stroke(XK_TAB, false, true));
    tap(&mut st, SysKey::Shift);
    assert_eq!(
        tap(&mut st, SysKey::Char('1')),
        stroke('1' as u32, false, false)
    );
    // ctrl+shift+left
    tap(&mut st, SysKey::Ctrl);
    tap(&mut st, SysKey::Shift);
    assert_eq!(tap(&mut st, SysKey::Left), stroke(XK_LEFT, true, true));
    // Alt is one-shot too: Alt+Tab
    assert_eq!(tap(&mut st, SysKey::Alt), Tap::State);
    assert!(st.alt);
    assert_eq!(
        tap(&mut st, SysKey::Tab),
        Tap::Send(Stroke {
            keysym: XK_TAB,
            ctrl: false,
            shift: false,
            alt: true
        })
    );
    assert!(!st.alt);
    // the paste key clears every latched modifier (the stroke is chosen by the keyboard)
    tap(&mut st, SysKey::Ctrl);
    tap(&mut st, SysKey::Alt);
    assert_eq!(tap(&mut st, SysKey::Paste), Tap::Paste);
    assert_eq!(st, SysKbdState::default());
    // layer toggle clears shift, hide resets everything
    tap(&mut st, SysKey::Shift);
    assert_eq!(tap(&mut st, SysKey::Layer), Tap::State);
    assert!(st.symbols && !st.shift);
    tap(&mut st, SysKey::Ctrl);
    assert_eq!(tap(&mut st, SysKey::Hide), Tap::Hide);
    assert_eq!(st, SysKbdState::default());
}

/// A tiny US-like keymap: keycode 10 = a/A, 11 = 1/!, 12 = Tab, 13 = Shift_L, 14 = Control_L,
/// 15 = Left.
fn keymap() -> KeyMap {
    KeyMap {
        min_keycode: 10,
        per: 2,
        syms: vec![
            'a' as u32,
            'A' as u32,
            '1' as u32,
            '!' as u32,
            XK_TAB,
            0,
            XK_SHIFT_L,
            0,
            XK_CONTROL_L,
            0,
            XK_LEFT,
            0,
            XK_ALT_L,
            0,
        ],
    }
}

#[test]
fn keymap_plans_modifiers_before_the_key() {
    let m = keymap();
    let s = |keysym, ctrl, shift| Stroke {
        keysym,
        ctrl,
        shift,
        alt: false,
    };
    assert_eq!(m.lookup('a' as u32), Some((10, false)));
    assert_eq!(m.lookup('A' as u32), Some((10, true)));
    assert_eq!(m.plan(s('a' as u32, false, false)), Some(vec![10]));
    assert_eq!(m.plan(s('A' as u32, false, false)), Some(vec![13, 10]));
    assert_eq!(m.plan(s('!' as u32, false, false)), Some(vec![13, 11]));
    assert_eq!(m.plan(s('a' as u32, true, false)), Some(vec![14, 10]));
    assert_eq!(m.plan(s(XK_TAB, false, true)), Some(vec![13, 12]));
    assert_eq!(m.plan(s(XK_LEFT, true, true)), Some(vec![14, 13, 15]));
    assert_eq!(m.plan(s('z' as u32, false, false)), None, "not in the map");
    let alt_tab = Stroke {
        keysym: XK_TAB,
        ctrl: false,
        shift: false,
        alt: true,
    };
    assert_eq!(m.plan(alt_tab), Some(vec![16, 12]));
    assert_eq!(m.plan(Stroke::ctrl('a' as u32)), Some(vec![14, 10]));
}
